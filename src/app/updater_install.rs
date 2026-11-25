//! Platform-aware release download and installation helpers.
//!
//! Only assets published by the official GitHub release endpoint are accepted.
//! Downloading and package installation are always run from the updater worker,
//! never from the egui frame.

use super::updater::{ReleaseInfo, UpdateInstallStatus};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::Sender;
use eframe::egui;
use std::time::{SystemTime, UNIX_EPOCH};

const OFFICIAL_DOWNLOAD_PREFIX: &str =
    "https://github.com/MobinWeekend/Jpeg-Viewer/releases/download/";
#[cfg(target_os = "linux")]
const FLATPAK_APP_ID: &str = "com.jpegviewer.JpegViewer";

pub(crate) enum InstallOutcome {
    /// A helper must wait for the current process to exit, replace the running
    /// executable/application bundle, and launch the updated version.
    CloseAndRestart,
    /// A native package installer has been started and can finish independently.
    InstallerLaunched,
}

enum InstallPlan {
    #[cfg(windows)]
    WindowsSetup,
    #[cfg(windows)]
    WindowsPortable { executable: PathBuf },
    #[cfg(target_os = "macos")]
    MacApplicationBundle { app_bundle: PathBuf },
    #[cfg(target_os = "linux")]
    LinuxFlatpak { system_install: bool },
    #[cfg(target_os = "linux")]
    LinuxAppImage { executable: PathBuf },
    #[cfg(target_os = "linux")]
    LinuxDeb,
    #[cfg(target_os = "linux")]
    LinuxRpm,
    #[cfg(target_os = "linux")]
    LinuxArch,
    #[cfg(target_os = "linux")]
    LinuxPortable,
}

impl InstallPlan {
    fn asset_name(&self, architecture: &str) -> String {
        match self {
            #[cfg(windows)]
            Self::WindowsSetup => format!("JPEG-Viewer-Setup-{architecture}.exe"),
            #[cfg(windows)]
            Self::WindowsPortable { .. } => format!("JPEG-Viewer-Windows-{architecture}.exe"),
            #[cfg(target_os = "macos")]
            Self::MacApplicationBundle { .. } => {
                let platform = if architecture == "ARM64" {
                    "Apple-Silicon-ARM64"
                } else {
                    "Intel-x64"
                };
                format!("JPEG-Viewer-macOS-{platform}.zip")
            }
            #[cfg(target_os = "linux")]
            Self::LinuxFlatpak { .. } => format!("JPEG-Viewer-Linux-{architecture}.flatpak"),
            #[cfg(target_os = "linux")]
            Self::LinuxAppImage { .. } => format!("JPEG-Viewer-Linux-{architecture}.AppImage"),
            #[cfg(target_os = "linux")]
            Self::LinuxDeb => format!("JPEG-Viewer-Linux-{architecture}.deb"),
            #[cfg(target_os = "linux")]
            Self::LinuxRpm => format!("JPEG-Viewer-Fedora-{architecture}.rpm"),
            #[cfg(target_os = "linux")]
            Self::LinuxArch => format!("JPEG-Viewer-Arch-{architecture}.pkg.tar.zst"),
            #[cfg(target_os = "linux")]
            Self::LinuxPortable => format!("JPEG-Viewer-Linux-{architecture}.tar.gz"),
        }
    }
}

pub(crate) fn run_update(
    release: &ReleaseInfo,
    progress: &Sender<UpdateInstallStatus>,
    ctx: &egui::Context,
) -> Result<InstallOutcome, String> {
    let architecture = release_architecture()?;
    let plan = detect_install_plan()?;
    let asset_name = plan.asset_name(&architecture);
    let _ = progress.send(UpdateInstallStatus::Downloading {
        asset_name: asset_name.clone(),
    });
    ctx.request_repaint();
    let download_url = release
        .assets
        .get(&asset_name)
        .filter(|url| url.starts_with(OFFICIAL_DOWNLOAD_PREFIX))
        .ok_or_else(|| {
            format!(
                "This release does not contain the matching build ({asset_name}). Open the release page to download a supported package manually."
            )
        })?;

    #[cfg(target_os = "linux")]
    let host_visible_update_dir = matches!(&plan, InstallPlan::LinuxFlatpak { .. });
    #[cfg(not(target_os = "linux"))]
    let host_visible_update_dir = false;
    let update_dir = create_update_directory(
        &release.version.to_string(),
        host_visible_update_dir,
    )?;
    let downloaded_asset = update_dir.join(&asset_name);
    download_asset(download_url, &downloaded_asset)?;
    let _ = progress.send(UpdateInstallStatus::Installing {
        asset_name: asset_name.clone(),
    });
    ctx.request_repaint();

    install_downloaded_asset(
        plan,
        &downloaded_asset,
        &update_dir,
    )
}

fn release_architecture() -> Result<String, String> {
    match env::consts::ARCH {
        "x86_64" => Ok("x64".to_owned()),
        "aarch64" => Ok("ARM64".to_owned()),
        architecture => Err(format!(
            "Automatic updates are not available for this CPU architecture ({architecture})."
        )),
    }
}

fn detect_install_plan() -> Result<InstallPlan, String> {
    #[cfg(windows)]
    {
        let executable = env::current_exe()
            .map_err(|error| format!("Could not locate the running executable: {error}"))?;
        // Inno Setup can reuse a user-selected installation directory. Detect
        // its uninstaller beside the running binary instead of assuming the
        // default %LOCALAPPDATA% path.
        let installed_by_setup = executable
            .parent()
            .is_some_and(|directory| directory.join("unins000.exe").is_file());
        let default_install = env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .map(|path| path.join("Programs").join("JPEG Viewer").join("jpeg_viewer.exe"))
            .is_some_and(|installed| same_path(&executable, &installed));
        if installed_by_setup || default_install {
            Ok(InstallPlan::WindowsSetup)
        } else {
            Ok(InstallPlan::WindowsPortable { executable })
        }
    }

    #[cfg(target_os = "macos")]
    {
        let executable = env::current_exe()
            .map_err(|error| format!("Could not locate the running application: {error}"))?;
        let app_bundle = executable
            .parent()
            .and_then(Path::parent)
            .and_then(Path::parent)
            .filter(|path| path.extension().is_some_and(|extension| extension == "app"))
            .map(Path::to_path_buf)
            .ok_or_else(|| {
                "The running executable is not inside a JPEG Viewer.app bundle, so it cannot be replaced automatically. Open the official release page to install the macOS ZIP manually.".to_owned()
            })?;
        Ok(InstallPlan::MacApplicationBundle { app_bundle })
    }

    #[cfg(target_os = "linux")]
    {
        if env::var_os("FLATPAK_ID").is_some() || Path::new("/.flatpak-info").exists() {
            let location = host_command_output("flatpak", &["info", "--show-location", FLATPAK_APP_ID])
                .ok_or_else(|| {
                    "Could not query the host Flatpak installation. Automatic Flatpak updates require the host Flatpak integration permission.".to_owned()
                })?;
            // Read the host's XDG data directory: Flatpak can redirect the
            // sandbox's XDG_DATA_HOME into ~/.var/app, which is not where the
            // host stores user-installed Flatpak deployments.
            let user_data_home = host_command_output(
                "sh",
                &["-c", "printf '%s' \"${XDG_DATA_HOME:-$HOME/.local/share}\""]
            )
            .map(PathBuf::from)
            .or_else(|| {
                env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
            });
            let user_install = user_data_home
                .is_some_and(|data_home| Path::new(&location).starts_with(data_home.join("flatpak")));
            return Ok(InstallPlan::LinuxFlatpak {
                system_install: !user_install,
            });
        }

        if let Some(appimage) = env::var_os("APPIMAGE").map(PathBuf::from) {
            if appimage.is_file() {
                return Ok(InstallPlan::LinuxAppImage {
                    executable: appimage,
                });
            }
        }

        let executable = env::current_exe()
            .map_err(|error| format!("Could not locate the running executable: {error}"))?;
        if package_owns_file("dpkg-query", &["-S"], &executable) {
            return Ok(InstallPlan::LinuxDeb);
        }
        if package_owns_file("rpm", &["-qf"], &executable) {
            return Ok(InstallPlan::LinuxRpm);
        }
        if package_owns_file("pacman", &["-Qo"], &executable) {
            return Ok(InstallPlan::LinuxArch);
        }

        if let Some(home) = env::var_os("HOME").map(PathBuf::from) {
            let portable_executable = home
                .join(".local")
                .join("opt")
                .join("jpeg-viewer")
                .join("jpeg_viewer");
            if same_path(&executable, &portable_executable) {
                return Ok(InstallPlan::LinuxPortable);
            }
        }

        Err(format!(
            "Could not identify the Linux installation type for {}. Automatic updates support Flatpak, AppImage, Debian, RPM, Arch, and the official per-user portable install. Open the release page to choose your package manually.",
            executable.display()
        ))
    }

    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    {
        Err("Automatic updates are not supported on this operating system.".to_owned())
    }
}

fn create_update_directory(version: &str, host_visible: bool) -> Result<PathBuf, String> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let safe_version = version.replace(&['/', '\\', ':'][..], "_");
    // A Flatpak host-side updater cannot read the sandbox's private /tmp.
    // Its home directory is explicitly shared with the host.
    let base = if host_visible {
        env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| "Could not locate the shared user home directory.".to_owned())?
            .join(".cache")
            .join("jpeg-viewer")
            .join("updates")
    } else {
        env::temp_dir()
    };
    let directory = base.join(format!(
        "jpeg-viewer-update-{}-{timestamp}-{safe_version}",
        std::process::id()
    ));
    fs::create_dir_all(&directory)
        .map_err(|error| format!("Could not create the update download folder: {error}"))?;
    Ok(directory)
}

fn download_asset(url: &str, destination: &Path) -> Result<(), String> {
    if !url.starts_with(OFFICIAL_DOWNLOAD_PREFIX) {
        return Err("Refusing to download an update asset from an untrusted URL.".to_owned());
    }

    let output = curl_command()
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--connect-timeout",
            "10",
            "--max-time",
            "1800",
            "--retry",
            "2",
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--user-agent",
            concat!("Jpeg-Viewer/", env!("CARGO_PKG_VERSION")),
            "--output",
        ])
        .arg(destination)
        .arg(url)
        .output();

    match output {
        Ok(output) if output.status.success() => {}
        Ok(output) => {
            let details = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            return Err(if details.is_empty() {
                format!("The update download failed ({})", output.status)
            } else {
                format!("The update download failed: {details}")
            });
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let output = wget_command()
                .args(["--quiet", "--timeout=30", "--tries=2", "--output-document"])
                .arg(destination)
                .arg(url)
                .output()
                .map_err(|error| {
                    format!("Could not start curl or wget to download the update: {error}")
                })?;
            if !output.status.success() {
                let details = String::from_utf8_lossy(&output.stderr).trim().to_owned();
                return Err(if details.is_empty() {
                    format!("The update download through wget failed ({})", output.status)
                } else {
                    format!("The update download through wget failed: {details}")
                });
            }
        }
        Err(error) => return Err(format!("Could not start curl for the update download: {error}")),
    }

    let size = fs::metadata(destination)
        .map_err(|error| format!("The downloaded update could not be verified: {error}"))?
        .len();
    if size == 0 {
        return Err("GitHub returned an empty update download.".to_owned());
    }
    Ok(())
}

fn install_downloaded_asset(
    plan: InstallPlan,
    downloaded_asset: &Path,
    update_dir: &Path,
) -> Result<InstallOutcome, String> {
    #[cfg(windows)]
    match plan {
        InstallPlan::WindowsSetup => {
            let mut installer = Command::new(downloaded_asset);
            installer.args(["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART"]);
            spawn_installer(installer, "Windows setup installer")?;
            Ok(InstallOutcome::InstallerLaunched)
        }
        InstallPlan::WindowsPortable { executable } => {
            ensure_parent_writable(&executable)?;
            let replacement = write_windows_replacement_helper(downloaded_asset, &executable, update_dir)?;
            let mut helper = powershell_command();
            helper.args([
                "-NoProfile",
                "-WindowStyle",
                "Hidden",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ]);
            helper.arg(replacement);
            spawn_installer(helper, "portable Windows update helper")?;
            Ok(InstallOutcome::CloseAndRestart)
        }
    }

    #[cfg(target_os = "macos")]
    match plan {
        InstallPlan::MacApplicationBundle { app_bundle } => {
            let helper = write_macos_replacement_helper(downloaded_asset, &app_bundle, update_dir)?;
            spawn_macos_replacement_helper(&helper)?;
            Ok(InstallOutcome::CloseAndRestart)
        }
    }

    #[cfg(target_os = "linux")]
    match plan {
        InstallPlan::LinuxFlatpak { system_install } => {
            // Flatpak applications run in a sandbox, so invoke the host package
            // manager through flatpak-spawn. The manifest grants only the
            // Flatpak service name needed for this operation.
            let install_command = if system_install {
                "pkexec flatpak install --system --noninteractive --or-update --bundle \"$1\""
            } else {
                "flatpak install --user --noninteractive --or-update --bundle \"$1\""
            };
            // The host cannot access the sandbox's private /tmp, so the bundle
            // lives in the shared cache directory. Remove it after flatpak exits.
            let script = format!(
                "{install_command}; status=$?; rm -rf -- \"$2\"; exit \"$status\""
            );
            let mut command = Command::new("flatpak-spawn");
            command
                .args(["--host", "sh", "-c", &script, "jpeg-viewer-flatpak-update"])
                .arg(downloaded_asset)
                .arg(update_dir);
            spawn_installer(command, "host Flatpak installer")?;
            Ok(InstallOutcome::InstallerLaunched)
        }
        InstallPlan::LinuxAppImage { executable } => {
            let helper = write_linux_appimage_helper(downloaded_asset, &executable, update_dir)?;
            let mut command = Command::new("sh");
            command.arg(helper);
            spawn_installer(command, "AppImage replacement helper")?;
            Ok(InstallOutcome::CloseAndRestart)
        }
        InstallPlan::LinuxDeb => {
            let mut command = Command::new("pkexec");
            command.args(["dpkg", "--install"]).arg(downloaded_asset);
            spawn_installer(command, "Debian package installer")?;
            Ok(InstallOutcome::InstallerLaunched)
        }
        InstallPlan::LinuxRpm => {
            let mut command = Command::new("pkexec");
            command.args(["rpm", "-Uvh", "--replacepkgs"]).arg(downloaded_asset);
            spawn_installer(command, "RPM package installer")?;
            Ok(InstallOutcome::InstallerLaunched)
        }
        InstallPlan::LinuxArch => {
            let mut command = Command::new("pkexec");
            command.args(["pacman", "-U", "--noconfirm"]).arg(downloaded_asset);
            spawn_installer(command, "Arch package installer")?;
            Ok(InstallOutcome::InstallerLaunched)
        }
        InstallPlan::LinuxPortable => {
            let extract_dir = update_dir.join("extracted");
            fs::create_dir_all(&extract_dir)
                .map_err(|error| format!("Could not prepare the portable update: {error}"))?;
            let status = Command::new("tar")
                .args(["-xzf"])
                .arg(downloaded_asset)
                .args(["-C"])
                .arg(&extract_dir)
                .status()
                .map_err(|error| format!("Could not extract the Linux portable update: {error}"))?;
            if !status.success() {
                return Err(format!("Extracting the Linux portable update failed ({status})."));
            }
            let install_script = extract_dir.join("JPEG-Viewer").join("install.sh");
            if !install_script.is_file() {
                return Err("The Linux portable release did not contain its install script.".to_owned());
            }
            let helper = write_linux_portable_replacement_helper(
                &install_script,
                update_dir,
            )?;
            let mut command = Command::new("sh");
            command.arg(helper);
            spawn_installer(command, "Linux portable update helper")?;
            Ok(InstallOutcome::CloseAndRestart)
        }
    }

    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    match plan {}
}

fn spawn_installer(mut command: Command, description: &str) -> Result<(), String> {
    let child = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                format!("Could not start the {description}; a required system tool is unavailable.")
            } else {
                format!("Could not start the {description}: {error}")
            }
        })?;
    // Detach the installer/helper. Portable replacement scripts intentionally
    // outlive this process until the current viewer has exited.
    drop(child);
    Ok(())
}

#[cfg(windows)]
fn ensure_parent_writable(target: &Path) -> Result<(), String> {
    let parent = target
        .parent()
        .ok_or_else(|| "Could not determine the installation folder.".to_owned())?;
    let probe = parent.join(format!(".jpeg-viewer-write-test-{}", std::process::id()));
    fs::write(&probe, b"test").map_err(|error| {
        format!(
            "The current user cannot replace the installed program in {}: {error}",
            parent.display()
        )
    })?;
    let _ = fs::remove_file(probe);
    Ok(())
}

#[cfg(windows)]
fn write_windows_replacement_helper(
    downloaded_asset: &Path,
    executable: &Path,
    update_dir: &Path,
) -> Result<PathBuf, String> {
    let helper = update_dir.join("install-update.ps1");
    let contents = format!(
        "$ErrorActionPreference = 'Stop'\n$processId = {}\n$source = {}\n$target = {}\nwhile (Get-Process -Id $processId -ErrorAction SilentlyContinue) {{ Start-Sleep -Milliseconds 300 }}\nCopy-Item -LiteralPath $source -Destination $target -Force\nStart-Process -FilePath $target\nRemove-Item -LiteralPath {} -Recurse -Force -ErrorAction SilentlyContinue\nRemove-Item -LiteralPath $PSCommandPath -Force -ErrorAction SilentlyContinue\n",
        std::process::id(),
        powershell_literal(downloaded_asset),
        powershell_literal(executable),
        powershell_literal(update_dir),
    );
    fs::write(&helper, contents)
        .map_err(|error| format!("Could not prepare the portable update helper: {error}"))?;
    Ok(helper)
}

#[cfg(windows)]
fn powershell_literal(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "''"))
}

#[cfg(windows)]
fn powershell_command() -> Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut command = Command::new("powershell.exe");
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

#[cfg(target_os = "macos")]
fn write_macos_replacement_helper(
    downloaded_asset: &Path,
    app_bundle: &Path,
    update_dir: &Path,
) -> Result<PathBuf, String> {
    use std::os::unix::fs::PermissionsExt;

    let helper = update_dir.join("install-update.sh");
    let privileged_helper = update_dir.join("replace-app.sh");
    let extracted = update_dir.join("extracted");
    let new_app = extracted.join("JPEG Viewer.app");
    let parent = app_bundle
        .parent()
        .ok_or_else(|| "Could not determine the macOS application folder.".to_owned())?;
    let probe = parent.join(format!(".jpeg-viewer-write-test-{}", std::process::id()));
    let can_write = fs::write(&probe, b"test").is_ok();
    let _ = fs::remove_file(probe);

    // Keep the privileged script limited to replacing the bundle. The outer
    // helper remains a normal user process so `open` relaunches JPEG Viewer as
    // the logged-in user rather than accidentally starting it as root.
    let privileged_contents = format!(
        "#!/bin/sh\nset -eu\nAPP={}\nNEW_APP={}\nBACKUP=\"$APP.jpeg-viewer-old\"\nrm -rf \"$BACKUP\"\nmv \"$APP\" \"$BACKUP\"\nif /usr/bin/ditto \"$NEW_APP\" \"$APP\"; then rm -rf \"$BACKUP\"; else rm -rf \"$APP\"; mv \"$BACKUP\" \"$APP\"; exit 1; fi\n",
        shell_literal(app_bundle),
        shell_literal(&new_app),
    );
    fs::write(&privileged_helper, privileged_contents)
        .map_err(|error| format!("Could not prepare the macOS bundle replacement script: {error}"))?;
    fs::set_permissions(&privileged_helper, fs::Permissions::from_mode(0o700))
        .map_err(|error| format!("Could not mark the macOS bundle replacement script executable: {error}"))?;

    let replacement_command = if can_write {
        format!("/bin/sh {}", shell_literal(&privileged_helper))
    } else {
        // Ask for authorization only for the bundle replacement. The helper
        // that waits for this app and relaunches it continues running as user.
        let shell_command = format!("/bin/sh {}", shell_literal(&privileged_helper));
        let apple_script = format!(
            "do shell script \"{}\" with administrator privileges",
            shell_command.replace('\\', "\\\\").replace('"', "\\\"")
        );
        format!("/usr/bin/osascript -e {}", shell_quote(&apple_script))
    };

    let contents = format!(
        "#!/bin/sh\nset -eu\nPID='{}'\nAPP={}\nARCHIVE={}\nEXTRACTED={}\nUPDATE_DIR={}\nwhile kill -0 \"$PID\" 2>/dev/null; do sleep 0.25; done\nmkdir -p \"$EXTRACTED\"\n/usr/bin/ditto -x -k \"$ARCHIVE\" \"$EXTRACTED\"\nNEW_APP=\"$EXTRACTED/JPEG Viewer.app\"\n[ -d \"$NEW_APP\" ] || exit 1\n{replacement_command}\nopen \"$APP\"\nrm -rf \"$UPDATE_DIR\"\nrm -f \"$0\"\n",
        std::process::id(),
        shell_literal(app_bundle),
        shell_literal(downloaded_asset),
        shell_literal(&extracted),
        shell_literal(update_dir),
    );
    fs::write(&helper, contents)
        .map_err(|error| format!("Could not prepare the macOS update helper: {error}"))?;
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o700))
        .map_err(|error| format!("Could not mark the macOS update helper executable: {error}"))?;
    Ok(helper)
}

#[cfg(target_os = "macos")]
fn spawn_macos_replacement_helper(helper: &Path) -> Result<(), String> {
    let mut command = Command::new("sh");
    command.arg(helper);
    spawn_installer(command, "macOS application replacement helper")?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn write_linux_appimage_helper(
    downloaded_asset: &Path,
    executable: &Path,
    update_dir: &Path,
) -> Result<PathBuf, String> {
    use std::os::unix::fs::PermissionsExt;

    let parent = executable.parent().ok_or_else(|| {
        "Could not determine the AppImage's installation folder.".to_owned()
    })?;
    let staged_update = parent.join(format!(
        ".jpeg-viewer-update-{}-{}",
        std::process::id(),
        executable.file_name().and_then(|name| name.to_str()).unwrap_or("appimage")
    ));
    fs::copy(downloaded_asset, &staged_update).map_err(|error| {
        format!(
            "Could not stage the AppImage beside the current installation ({}): {error}",
            parent.display()
        )
    })?;
    fs::set_permissions(&staged_update, fs::Permissions::from_mode(0o755))
        .map_err(|error| format!("Could not mark the downloaded AppImage executable: {error}"))?;

    let helper = update_dir.join("install-update.sh");
    let contents = format!(
        "#!/bin/sh\nset -eu\nPID='{}'\nSOURCE={}\nTARGET={}\nUPDATE_DIR={}\nwhile kill -0 \"$PID\" 2>/dev/null; do sleep 0.25; done\nmv -f \"$SOURCE\" \"$TARGET\"\nchmod 755 \"$TARGET\"\nnohup \"$TARGET\" >/dev/null 2>&1 &\nrm -rf -- \"$UPDATE_DIR\"\nrm -f \"$0\"\n",
        std::process::id(),
        shell_literal(&staged_update),
        shell_literal(executable),
        shell_literal(update_dir),
    );
    fs::write(&helper, contents)
        .map_err(|error| format!("Could not prepare the AppImage update helper: {error}"))?;
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o700))
        .map_err(|error| format!("Could not mark the AppImage update helper executable: {error}"))?;
    Ok(helper)
}


#[cfg(target_os = "linux")]
fn write_linux_portable_replacement_helper(
    install_script: &Path,
    update_dir: &Path,
) -> Result<PathBuf, String> {
    use std::os::unix::fs::PermissionsExt;

    let helper = update_dir.join("install-update.sh");
    let executable = env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "Could not determine the current user's home directory.".to_owned())?
        .join(".local")
        .join("opt")
        .join("jpeg-viewer")
        .join("jpeg_viewer");
    let contents = format!(
        "#!/bin/sh\nset -eu\nPID='{}'\nINSTALL_SCRIPT={}\nAPP={}\nUPDATE_DIR={}\nwhile kill -0 \"$PID\" 2>/dev/null; do sleep 0.25; done\n/bin/sh \"$INSTALL_SCRIPT\"\nnohup \"$APP\" >/dev/null 2>&1 &\nrm -rf -- \"$UPDATE_DIR\"\nrm -f \"$0\"\n",
        std::process::id(),
        shell_literal(install_script),
        shell_literal(&executable),
        shell_literal(update_dir),
    );
    fs::write(&helper, contents)
        .map_err(|error| format!("Could not prepare the Linux portable update helper: {error}"))?;
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o700))
        .map_err(|error| format!("Could not mark the Linux portable update helper executable: {error}"))?;
    Ok(helper)
}

#[cfg(unix)]
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(unix)]
fn shell_literal(path: &Path) -> String {
    shell_quote(&path.to_string_lossy())
}

#[cfg(target_os = "linux")]
fn package_owns_file(command: &str, prefix_args: &[&str], path: &Path) -> bool {
    let mut process = Command::new(command);
    process.args(prefix_args).arg(path);
    process
        .output()
        .is_ok_and(|output| output.status.success())
}

#[cfg(target_os = "linux")]
fn host_command_output(command: &str, args: &[&str]) -> Option<String> {
    if env::var_os("FLATPAK_ID").is_some() || Path::new("/.flatpak-info").exists() {
        let output = Command::new("flatpak-spawn")
            .arg("--host")
            .arg(command)
            .args(args)
            .output()
            .ok()?;
        return output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned());
    }
    command_output(command, args)
}

#[cfg(target_os = "linux")]
fn command_output(command: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(command).args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

#[cfg(any(windows, target_os = "linux"))]
fn same_path(first: &Path, second: &Path) -> bool {
    let first = fs::canonicalize(first).unwrap_or_else(|_| first.to_path_buf());
    let second = fs::canonicalize(second).unwrap_or_else(|_| second.to_path_buf());
    #[cfg(windows)]
    {
        first.to_string_lossy().eq_ignore_ascii_case(&second.to_string_lossy())
    }
    #[cfg(not(windows))]
    {
        first == second
    }
}

#[cfg(windows)]
fn curl_command() -> Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut command = Command::new("curl");
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

#[cfg(not(windows))]
fn curl_command() -> Command {
    Command::new("curl")
}

#[cfg(windows)]
fn wget_command() -> Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut command = Command::new("wget");
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

#[cfg(not(windows))]
fn wget_command() -> Command {
    Command::new("wget")
}

#[cfg(test)]
mod tests {
    use super::release_architecture;

    #[test]
    fn release_architecture_matches_supported_build_targets() {
        let expected = match std::env::consts::ARCH {
            "x86_64" => "x64",
            "aarch64" => "ARM64",
            _ => return,
        };
        assert_eq!(release_architecture().unwrap(), expected);
    }
}
