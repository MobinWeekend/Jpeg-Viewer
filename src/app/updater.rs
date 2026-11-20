//! Background checker for the latest stable GitHub release.
//!
//! The request is intentionally small and short-lived. It only reads release
//! metadata; downloading and installing an update remain explicit user actions.

use eframe::egui;
use semver::Version;
use std::process::Command;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{SystemTime, UNIX_EPOCH};

const LATEST_RELEASE_API: &str =
    "https://api.github.com/repos/MobinWeekend/Jpeg-Viewer/releases/latest";
const OFFICIAL_RELEASE_PREFIX: &str =
    "https://github.com/MobinWeekend/Jpeg-Viewer/releases/";
const AUTOMATIC_CHECK_INTERVAL_SECONDS: i64 = 7 * 24 * 60 * 60;
const MAX_RELEASE_RESPONSE_BYTES: usize = 1_048_576;
const MAX_RELEASE_RESPONSE_BYTES_TEXT: &str = "1048576";
const HTTP_STATUS_MARKER: &str = "\n__JPEG_VIEWER_HTTP_STATUS__:";

#[derive(Clone, Debug)]
pub(crate) struct ReleaseInfo {
    pub version: Version,
    pub tag_name: String,
    pub title: String,
    pub url: String,
    pub published_at: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub(crate) enum UpdateStatus {
    #[default]
    Idle,
    Checking,
    UpToDate {
        latest_version: Version,
    },
    UpdateAvailable {
        release: ReleaseInfo,
    },
    Failed {
        message: String,
    },
}

impl ReleaseInfo {
    /// Stores only the fields needed to keep an available update visible across launches.
    pub fn to_settings_json(&self) -> String {
        serde_json::json!({
            "tag_name": self.tag_name,
            "name": self.title,
            "html_url": self.url,
            "published_at": self.published_at.as_deref(),
            "draft": false,
            "prerelease": false
        })
        .to_string()
    }
}

pub(crate) struct Updater {
    status: UpdateStatus,
    receiver: Option<Receiver<Result<ReleaseInfo, String>>>,
    automatic_check_considered: bool,
}

impl Default for Updater {
    fn default() -> Self {
        Self {
            status: UpdateStatus::Idle,
            receiver: None,
            automatic_check_considered: false,
        }
    }
}

impl Updater {
    /// Restores a previously discovered update, but only if it is still newer than
    /// the version running now and its stored release URL passes the same validation.
    pub fn from_cached_release(cached_release_json: &str) -> Self {
        let mut updater = Self::default();
        if !cached_release_json.trim().is_empty()
            && let Ok(release) = parse_release_response(cached_release_json.as_bytes())
        {
            let current = Version::parse(env!("CARGO_PKG_VERSION"))
                .expect("Cargo package version must be valid SemVer");
            if release.version > current {
                updater.status = UpdateStatus::UpdateAvailable { release };
            }
        }
        updater
    }

    pub fn status(&self) -> &UpdateStatus {
        &self.status
    }

    pub fn is_checking(&self) -> bool {
        matches!(&self.status, UpdateStatus::Checking)
    }

    /// Starts one launch-time check, unless an attempt was made within the last
    /// seven days. Returns the timestamp to persist when a worker was started.
    pub fn maybe_start_automatic_check(
        &mut self,
        last_attempt_unix: i64,
        ctx: &egui::Context,
    ) -> Option<i64> {
        if self.automatic_check_considered {
            return None;
        }
        self.automatic_check_considered = true;

        let now = unix_now();
        let due = last_attempt_unix <= 0
            || now < last_attempt_unix
            || now.saturating_sub(last_attempt_unix) >= AUTOMATIC_CHECK_INTERVAL_SECONDS;
        (due && self.start_check(ctx)).then_some(now)
    }

    /// Starts a manual check, bypassing the automatic weekly cooldown.
    /// Returns the attempt timestamp for persistence when a worker was started.
    pub fn start_manual_check(&mut self, ctx: &egui::Context) -> Option<i64> {
        let now = unix_now();
        self.start_check(ctx).then_some(now)
    }

    fn start_check(&mut self, ctx: &egui::Context) -> bool {
        if self.is_checking() {
            return false;
        }

        let (sender, receiver) = mpsc::channel();
        let repaint_context = ctx.clone();
        let thread = std::thread::Builder::new()
            .name("jpeg-viewer-update-check".to_owned())
            .spawn(move || {
                let result = fetch_latest_release();
                let _ = sender.send(result);
                repaint_context.request_repaint();
            });

        match thread {
            Ok(_) => {
                self.receiver = Some(receiver);
                self.status = UpdateStatus::Checking;
                true
            }
            Err(error) => {
                self.receiver = None;
                self.status = UpdateStatus::Failed {
                    message: format!("Could not start the update-check worker: {error}"),
                };
                false
            }
        }
    }

    /// Applies a completed background result to the UI-owned status state.
    pub fn poll(&mut self) -> Option<UpdateStatus> {
        let result = match self.receiver.as_ref() {
            Some(receiver) => match receiver.try_recv() {
                Ok(result) => Some(result),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => Some(Err(
                    "The update-check worker stopped before returning a result.".to_owned(),
                )),
            },
            None => None,
        };

        if let Some(result) = result {
            self.receiver = None;
            self.status = match result {
                Ok(release) => {
                    let current = Version::parse(env!("CARGO_PKG_VERSION"))
                        .expect("Cargo package version must be valid SemVer");
                    if release.version > current {
                        UpdateStatus::UpdateAvailable { release }
                    } else {
                        UpdateStatus::UpToDate {
                            latest_version: release.version,
                        }
                    }
                }
                Err(message) => UpdateStatus::Failed { message },
            };
            Some(self.status.clone())
        } else {
            None
        }
    }
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .min(i64::MAX as u64) as i64
}

fn curl_command() -> Command {
    let mut command = Command::new("curl");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Avoid briefly opening a console window from the GUI application.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

fn fetch_latest_release() -> Result<ReleaseInfo, String> {
    // `curl` is present on current Windows, macOS, and common Linux installs.
    // `wget` is a fallback for minimal Linux environments. Keeping the updater
    // on system HTTP utilities avoids adding another TLS stack to the viewer.
    let body = match curl_command()
        .args([
            "--silent",
            "--show-error",
            "--location",
            "--connect-timeout",
            "5",
            "--max-time",
            "15",
            "--max-filesize",
            MAX_RELEASE_RESPONSE_BYTES_TEXT,
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--user-agent",
            concat!("Jpeg-Viewer/", env!("CARGO_PKG_VERSION")),
            "--header",
            "Accept: application/vnd.github+json",
            "--header",
            "X-GitHub-Api-Version: 2022-11-28",
            "--write-out",
            "\n__JPEG_VIEWER_HTTP_STATUS__:%{http_code}",
            LATEST_RELEASE_API,
        ])
        .output()
    {
        Ok(output) => release_body_from_curl(output)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => fetch_with_wget()?,
        Err(error) => return Err(format!("Could not start curl for the update check: {error}")),
    };

    parse_release_response(&body)
}

fn release_body_from_curl(output: std::process::Output) -> Result<Vec<u8>, String> {
    if !output.status.success() {
        let details = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(if details.is_empty() {
            format!("The HTTPS request failed ({})", output.status)
        } else {
            format!("The HTTPS request failed: {details}")
        });
    }

    let stdout = String::from_utf8(output.stdout)
        .map_err(|_| "GitHub returned a response that was not valid UTF-8.".to_owned())?;
    let (body, status) = stdout
        .rsplit_once(HTTP_STATUS_MARKER)
        .ok_or_else(|| "The update server response did not include an HTTP status.".to_owned())?;
    let status = status
        .trim()
        .parse::<u16>()
        .map_err(|_| "The update server returned an invalid HTTP status.".to_owned())?;

    if status != 200 {
        let message = serde_json::from_str::<serde_json::Value>(body)
            .ok()
            .and_then(|value| value.get("message")?.as_str().map(str::to_owned));
        return Err(match message {
            Some(message) => format!("GitHub returned HTTP {status}: {message}"),
            None => format!("GitHub returned HTTP {status} while checking for updates."),
        });
    }
    if body.len() > MAX_RELEASE_RESPONSE_BYTES {
        return Err("The GitHub release response exceeded the 1 MiB size limit.".to_owned());
    }

    Ok(body.as_bytes().to_vec())
}

fn fetch_with_wget() -> Result<Vec<u8>, String> {
    let output = wget_command()
        .args([
            "--quiet",
            "--timeout=15",
            "--tries=1",
            "--max-redirect=5",
            "--user-agent",
            concat!("Jpeg-Viewer/", env!("CARGO_PKG_VERSION")),
            "--header=Accept: application/vnd.github+json",
            "--header=X-GitHub-Api-Version: 2022-11-28",
            "--output-document=-",
            LATEST_RELEASE_API,
        ])
        .output()
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                "Could not check for updates because neither `curl` nor `wget` is installed or available on PATH. Open the official GitHub Releases page to check manually.".to_owned()
            } else {
                format!("Could not start wget for the update check: {error}")
            }
        })?;

    if !output.status.success() {
        let details = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(if details.is_empty() {
            format!("The HTTPS request through wget failed ({})", output.status)
        } else {
            format!("The HTTPS request through wget failed: {details}")
        });
    }

    if output.stdout.len() > MAX_RELEASE_RESPONSE_BYTES {
        return Err("The GitHub release response exceeded the 1 MiB size limit.".to_owned());
    }

    Ok(output.stdout)
}

fn wget_command() -> Command {
    let mut command = Command::new("wget");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

fn parse_release_response(body: &[u8]) -> Result<ReleaseInfo, String> {
    let value: serde_json::Value = serde_json::from_slice(body)
        .map_err(|error| format!("Could not parse the GitHub release response: {error}"))?;

    if value.get("draft").and_then(serde_json::Value::as_bool) == Some(true)
        || value
            .get("prerelease")
            .and_then(serde_json::Value::as_bool)
            == Some(true)
    {
        return Err("GitHub did not return a stable release.".to_owned());
    }

    let tag_name = value
        .get("tag_name")
        .and_then(serde_json::Value::as_str)
        .filter(|tag| !tag.trim().is_empty())
        .ok_or_else(|| "GitHub's release response did not contain a version tag.".to_owned())?
        .to_owned();
    let version_text = tag_name
        .strip_prefix('v')
        .or_else(|| tag_name.strip_prefix('V'))
        .unwrap_or(&tag_name);
    let version = Version::parse(version_text).map_err(|error| {
        format!("The GitHub release tag `{tag_name}` is not a valid stable version: {error}")
    })?;

    let url = value
        .get("html_url")
        .and_then(serde_json::Value::as_str)
        .filter(|url| url.starts_with(OFFICIAL_RELEASE_PREFIX))
        .ok_or_else(|| "GitHub returned an unexpected release URL.".to_owned())?
        .to_owned();

    let title = value
        .get("name")
        .and_then(serde_json::Value::as_str)
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(&tag_name)
        .to_owned();
    let published_at = value
        .get("published_at")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);

    Ok(ReleaseInfo {
        version,
        tag_name,
        title,
        url,
        published_at,
    })
}

#[cfg(test)]
mod tests {
    use super::parse_release_response;

    #[test]
    fn parses_a_tagged_stable_release() {
        let response = br#"{
            "tag_name": "v1.2.3",
            "name": "JPEG Viewer 1.2.3",
            "html_url": "https://github.com/MobinWeekend/Jpeg-Viewer/releases/tag/v1.2.3",
            "published_at": "2026-11-20T12:00:00Z",
            "draft": false,
            "prerelease": false
        }"#;

        let release = parse_release_response(response).expect("valid release should parse");
        assert_eq!(release.version.to_string(), "1.2.3");
        assert_eq!(release.tag_name, "v1.2.3");
        assert_eq!(release.title, "JPEG Viewer 1.2.3");
    }

    #[test]
    fn cached_release_reference_round_trips() {
        let response = br#"{
            "tag_name": "v1.2.3",
            "name": "JPEG Viewer 1.2.3",
            "html_url": "https://github.com/MobinWeekend/Jpeg-Viewer/releases/tag/v1.2.3",
            "draft": false,
            "prerelease": false
        }"#;
        let release = parse_release_response(response).expect("valid release should parse");
        let cached = release.to_settings_json();
        let restored = parse_release_response(cached.as_bytes())
            .expect("saved release reference should parse again");

        assert_eq!(restored.version, release.version);
        assert_eq!(restored.url, release.url);
    }

    #[test]
    fn rejects_non_github_release_urls() {
        let response = br#"{
            "tag_name": "v1.2.3",
            "html_url": "https://example.com/releases/v1.2.3",
            "draft": false,
            "prerelease": false
        }"#;

        let error = parse_release_response(response).expect_err("external URL must be rejected");
        assert!(error.contains("unexpected release URL"));
    }

    #[test]
    fn rejects_prereleases() {
        let response = br#"{
            "tag_name": "v1.2.3-rc.1",
            "html_url": "https://github.com/MobinWeekend/Jpeg-Viewer/releases/tag/v1.2.3-rc.1",
            "draft": false,
            "prerelease": true
        }"#;

        let error = parse_release_response(response).expect_err("prerelease must be rejected");
        assert!(error.contains("stable release"));
    }
}
