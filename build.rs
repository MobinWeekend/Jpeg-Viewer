fn main() {
    // Re-run when UI assets or displayed toolchain/dependency versions change.
    println!("cargo:rerun-if-changed=assets/icons");
    println!("cargo:rerun-if-changed=rust-toolchain.toml");
    println!("cargo:rerun-if-changed=Cargo.lock");
    println!("cargo:rerun-if-env-changed=CARGO_PKG_VERSION");
    emit_project_versions();

    #[cfg(target_os = "windows")]
    build_windows();

    #[cfg(target_os = "linux")]
    build_linux();

    #[cfg(target_os = "macos")]
    build_macos();
}

#[cfg(target_os = "windows")]
fn build_windows() {
    let mut res = winresource::WindowsResource::new();

    res.set_icon("assets/icons/icon.ico");
    res.set("FileDescription", "JPEG Viewer");
    res.set("ProductName", "JPEG Viewer");
    res.set("CompanyName", "MobinWeekend");
    res.set("LegalCopyright", "Copyright © MobinWeekend");

    let version =
        std::env::var("CARGO_PKG_VERSION").expect("Cargo did not provide CARGO_PKG_VERSION");
    res.set("FileVersion", &version);
    res.set("ProductVersion", &version);

    res.compile().expect("Failed to compile Windows resources");
}

#[cfg(target_os = "linux")]
fn build_linux() {
    // Linux icons are installed separately by the packaging system.
    println!("cargo:rerun-if-changed=assets/icons/icon.png");
}

#[cfg(target_os = "macos")]
fn build_macos() {
    println!("cargo:rerun-if-changed=packaging/macos/Info.plist");
    println!("cargo:rerun-if-changed=packaging/macos/jpeg_viewer.icns");
}

fn emit_project_versions() {
    let toolchain =
        std::fs::read_to_string("rust-toolchain.toml").expect("Could not read rust-toolchain.toml");
    let rust_version = toolchain
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once('=')?;
            (key.trim() == "channel").then(|| value.trim().trim_matches('"'))
        })
        .expect("rust-toolchain.toml must define a toolchain channel");
    println!("cargo:rustc-env=JPEG_VIEWER_RUST_VERSION={rust_version}");

    let lockfile = std::fs::read_to_string("Cargo.lock").expect("Could not read Cargo.lock");
    for (crate_name, env_name) in [
        ("eframe", "JPEG_VIEWER_EFRAME_VERSION"),
        ("egui", "JPEG_VIEWER_EGUI_VERSION"),
        ("winit", "JPEG_VIEWER_WINIT_VERSION"),
    ] {
        let version = locked_package_version(&lockfile, crate_name)
            .unwrap_or_else(|| panic!("{crate_name} was not found in Cargo.lock"));
        println!("cargo:rustc-env={env_name}={version}");
    }
}

fn locked_package_version<'a>(lockfile: &'a str, crate_name: &str) -> Option<&'a str> {
    let mut current_package: Option<&str> = None;

    for line in lockfile.lines() {
        let line = line.trim();
        if line == "[[package]]" {
            current_package = None;
            continue;
        }

        if let Some(name) = line.strip_prefix("name = ") {
            current_package = Some(name.trim_matches('"'));
            continue;
        }

        if current_package == Some(crate_name) {
            if let Some(version) = line.strip_prefix("version = ") {
                return Some(version.trim_matches('"'));
            }
        }
    }

    None
}
