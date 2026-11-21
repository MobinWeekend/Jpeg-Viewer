//! Monitor enumeration and default-display resolution.
//!
//! Keep this module independent of the UI framework so startup placement,
//! image-aware sizing, and the settings picker all use the same monitor list.

use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::process::{Command, Stdio};
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::thread;

const MONITOR_CACHE_TTL: Duration = Duration::from_secs(3);
/// Select the display that currently contains the largest part of the window.
pub const CURRENT_WINDOW_MONITOR: &str = "current";
#[cfg(any(target_os = "linux", target_os = "macos"))]
const COMMAND_TIMEOUT: Duration = Duration::from_millis(350);

#[derive(Debug, Clone, PartialEq)]
pub struct MonitorInfo {
    /// Stable-ish platform display identifier (device name/output name).
    pub id: String,
    pub name: String,
    /// Desktop-space origin and dimensions in logical points.
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub scale_factor: f32,
    pub is_primary: bool,
}

impl MonitorInfo {
    pub fn bounds(&self) -> eframe::egui::Rect {
        eframe::egui::Rect::from_min_size(
            eframe::egui::pos2(self.x, self.y),
            eframe::egui::vec2(self.width, self.height),
        )
    }

    /// Return the monitor's usable area after applying equal padding on every edge.
    /// Keep at least one logical point in each dimension, even for an excessive inset.
    pub fn inset_bounds(&self, padding: f32) -> eframe::egui::Rect {
        let padding = if padding.is_finite() {
            padding.max(0.0)
        } else {
            0.0
        };
        let horizontal = padding.min((self.width - 1.0).max(0.0) * 0.5);
        let vertical = padding.min((self.height - 1.0).max(0.0) * 0.5);
        eframe::egui::Rect::from_min_max(
            eframe::egui::pos2(self.x + horizontal, self.y + vertical),
            eframe::egui::pos2(
                self.x + self.width - horizontal,
                self.y + self.height - vertical,
            ),
        )
    }

    pub fn display_label(&self) -> String {
        let primary = if self.is_primary { " · Primary" } else { "" };
        let name = if self.name == self.id {
            self.name.clone()
        } else {
            format!("{} ({})", self.name, self.id)
        };
        format!(
            "{} · {} × {} pt{}",
            name,
            self.width.round() as i32,
            self.height.round() as i32,
            primary
        )
    }
}

#[derive(Default)]
struct MonitorCache {
    checked_at: Option<Instant>,
    monitors: Vec<MonitorInfo>,
}

/// Return a cached monitor list. The short refresh interval lets newly
/// connected/disconnected displays appear in Settings without spawning a
/// compositor query on every UI frame.
pub fn available_monitors() -> Vec<MonitorInfo> {
    static CACHE: OnceLock<Mutex<MonitorCache>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(MonitorCache::default()));
    let Ok(mut cache) = cache.lock() else {
        return enumerate_platform_monitors();
    };
    let should_refresh = cache
        .checked_at
        .is_none_or(|checked| checked.elapsed() >= MONITOR_CACHE_TTL);
    if should_refresh {
        cache.monitors = enumerate_platform_monitors();
        cache.checked_at = Some(Instant::now());
    }
    cache.monitors.clone()
}

/// Resolve the saved preference. If its display was disconnected, use the
/// primary display, then the first remaining display. Keeping the preference
/// unchanged means the selected display is used again if it is reconnected.
pub fn resolve_default_monitor<'a>(
    monitors: &'a [MonitorInfo],
    preferred_id: &str,
) -> Option<&'a MonitorInfo> {
    if preferred_id != "primary"
        && let Some(monitor) = monitors.iter().find(|monitor| monitor.id == preferred_id)
    {
        return Some(monitor);
    }
    monitors
        .iter()
        .find(|monitor| monitor.is_primary)
        .or_else(|| monitors.first())
}

/// Resolve the requested display using the active window rectangle when the
/// preference is `CURRENT_WINDOW_MONITOR`. The monitor with the greatest
/// intersection area wins; when no window geometry is available (or it overlaps
/// no enumerated monitor), use the primary display and then the first display.
pub fn resolve_default_monitor_at<'a>(
    monitors: &'a [MonitorInfo],
    preferred_id: &str,
    window_rect: Option<eframe::egui::Rect>,
) -> Option<&'a MonitorInfo> {
    if preferred_id == CURRENT_WINDOW_MONITOR {
        if let Some(window_rect) = window_rect {
            let best_overlap = monitors
                .iter()
                .map(|monitor| {
                    let overlap = window_rect.intersect(monitor.bounds());
                    let area = overlap.width().max(0.0) * overlap.height().max(0.0);
                    (monitor, area)
                })
                .filter(|(_, area)| *area > 0.0)
                .max_by(|(left, left_area), (right, right_area)| {
                    left_area
                        .total_cmp(right_area)
                        .then_with(|| left.is_primary.cmp(&right.is_primary))
                })
                .map(|(monitor, _)| monitor);
            if best_overlap.is_some() {
                return best_overlap;
            }
        }
        return monitors
            .iter()
            .find(|monitor| monitor.is_primary)
            .or_else(|| monitors.first());
    }

    resolve_default_monitor(monitors, preferred_id)
}

pub fn preferred_monitor_is_available(monitors: &[MonitorInfo], preferred_id: &str) -> bool {
    preferred_id == "primary"
        || preferred_id == CURRENT_WINDOW_MONITOR
        || monitors.iter().any(|monitor| monitor.id == preferred_id)
}

fn enumerate_platform_monitors() -> Vec<MonitorInfo> {
    #[cfg(target_os = "windows")]
    {
        return enumerate_windows_monitors();
    }
    #[cfg(target_os = "linux")]
    {
        return enumerate_linux_monitors();
    }
    #[cfg(target_os = "macos")]
    {
        return enumerate_macos_monitors();
    }
    #[allow(unreachable_code)]
    Vec::new()
}

#[cfg(target_os = "windows")]
fn enumerate_windows_monitors() -> Vec<MonitorInfo> {
    use std::ffi::c_void;
    use std::mem::size_of;
    use std::ptr;

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }

    #[repr(C)]
    struct MonitorInfoExW {
        cb_size: u32,
        rc_monitor: Rect,
        rc_work: Rect,
        dw_flags: u32,
        sz_device: [u16; 32],
    }

    type MonitorCallback = unsafe extern "system" fn(
        monitor: *mut c_void,
        hdc: *mut c_void,
        rect: *mut Rect,
        data: isize,
    ) -> i32;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn EnumDisplayMonitors(
            hdc: *mut c_void,
            clip_rect: *const Rect,
            callback: Option<MonitorCallback>,
            data: isize,
        ) -> i32;
        fn GetMonitorInfoW(monitor: *mut c_void, info: *mut MonitorInfoExW) -> i32;
    }

    #[link(name = "Shcore")]
    unsafe extern "system" {
        fn GetDpiForMonitor(
            monitor: *mut c_void,
            dpi_type: i32,
            dpi_x: *mut u32,
            dpi_y: *mut u32,
        ) -> i32;
    }

    unsafe extern "system" fn collect_monitor(
        monitor: *mut c_void,
        _hdc: *mut c_void,
        _rect: *mut Rect,
        data: isize,
    ) -> i32 {
        let monitors = unsafe { &mut *(data as *mut Vec<MonitorInfo>) };
        let mut info = MonitorInfoExW {
            cb_size: size_of::<MonitorInfoExW>() as u32,
            rc_monitor: Rect::default(),
            rc_work: Rect::default(),
            dw_flags: 0,
            sz_device: [0; 32],
        };
        if unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
            return 1;
        }

        let mut dpi_x = 96_u32;
        let mut dpi_y = 96_u32;
        let dpi_result = unsafe { GetDpiForMonitor(monitor, 0, &mut dpi_x, &mut dpi_y) };
        let scale = if dpi_result >= 0 && dpi_x > 0 {
            dpi_x as f32 / 96.0
        } else {
            1.0
        };
        let end = info
            .sz_device
            .iter()
            .position(|character| *character == 0)
            .unwrap_or(info.sz_device.len());
        let id = String::from_utf16_lossy(&info.sz_device[..end]);
        let width = (info.rc_monitor.right - info.rc_monitor.left) as f32 / scale;
        let height = (info.rc_monitor.bottom - info.rc_monitor.top) as f32 / scale;
        if width > 0.0 && height > 0.0 && !id.is_empty() {
            monitors.push(MonitorInfo {
                id: id.clone(),
                name: id,
                x: info.rc_monitor.left as f32 / scale,
                y: info.rc_monitor.top as f32 / scale,
                width,
                height,
                scale_factor: scale,
                is_primary: info.dw_flags & 1 != 0,
            });
        }
        1
    }

    let mut monitors: Vec<MonitorInfo> = Vec::new();
    unsafe {
        EnumDisplayMonitors(
            ptr::null_mut(),
            ptr::null(),
            Some(collect_monitor),
            &mut monitors as *mut Vec<MonitorInfo> as isize,
        );
    }
    monitors.sort_by(|left, right| {
        right
            .is_primary
            .cmp(&left.is_primary)
            .then_with(|| left.x.total_cmp(&right.x))
            .then_with(|| left.y.total_cmp(&right.y))
    });
    monitors
}

#[cfg(target_os = "linux")]
fn enumerate_linux_monitors() -> Vec<MonitorInfo> {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .to_ascii_lowercase();
    let session = std::env::var("XDG_SESSION_TYPE")
        .unwrap_or_default()
        .to_ascii_lowercase();

    // Use compositor-native Wayland tools first so the coordinates represent
    // the real desktop rather than XWayland's virtual root window.
    let is_cosmic = desktop.contains("cosmic");
    let is_kde = desktop.contains("kde") || desktop.contains("plasma");
    let mut commands: Vec<(&str, &[&str], fn(&str) -> Vec<MonitorInfo>)> = Vec::new();
    if is_cosmic {
        // COSMIC exposes the active output list through its own CLI. Its regular
        // text output includes enabled state, current mode, position, and scale.
        commands.push(("cosmic-randr", &["list"], parse_wlr_style_outputs));
    }
    if is_kde {
        commands.push(("kscreen-doctor", &["-o"], parse_kscreen_outputs));
    }
    if session == "wayland" {
        commands.push(("wlr-randr", &[], parse_wlr_style_outputs));
        if !is_cosmic {
            commands.push(("cosmic-randr", &["list"], parse_wlr_style_outputs));
        }
        if !is_kde {
            commands.push(("kscreen-doctor", &["-o"], parse_kscreen_outputs));
        }
    }
    commands.push(("xrandr", &["--query"], parse_xrandr_outputs));

    for (program, args, parser) in commands {
        if let Some(output) = run_command_with_timeout(program, args) {
            let monitors = parser(&output);
            if !monitors.is_empty() {
                return sort_monitors(monitors);
            }
        }
    }
    Vec::new()
}

#[cfg(target_os = "linux")]
fn parse_xrandr_outputs(output: &str) -> Vec<MonitorInfo> {
    let mut monitors = Vec::new();
    for line in output.lines() {
        let mut fields = line.split_whitespace();
        let Some(name) = fields.next() else {
            continue;
        };
        if fields.next() != Some("connected") {
            continue;
        }
        let remaining: Vec<&str> = fields.collect();
        let primary = remaining.contains(&"primary");
        let geometry = remaining
            .iter()
            .find_map(|field| parse_xrandr_geometry(field));
        let Some((width, height, x, y)) = geometry else {
            continue;
        };
        monitors.push(MonitorInfo {
            id: name.to_owned(),
            name: name.to_owned(),
            x,
            y,
            width,
            height,
            scale_factor: 1.0,
            is_primary: primary,
        });
    }
    monitors
}

#[cfg(target_os = "linux")]
fn parse_xrandr_geometry(value: &str) -> Option<(f32, f32, f32, f32)> {
    let value = value.trim_matches(|character: char| character == ',' || character == ';');
    let (width, rest) = value.split_once('x')?;
    let width = width.parse::<f32>().ok()?;
    let height_end = rest
        .char_indices()
        .find(|(_, character)| *character == '+' || *character == '-')
        .map(|(index, _)| index)
        .unwrap_or(rest.len());
    let height = rest[..height_end].parse::<f32>().ok()?;
    let position = &rest[height_end..];
    if position.is_empty() {
        return None;
    }
    let (x, y) = parse_signed_coordinate_pair(position)?;
    Some((width, height, x, y))
}

#[cfg(target_os = "linux")]
fn parse_wlr_style_outputs(output: &str) -> Vec<MonitorInfo> {
    #[derive(Default)]
    struct Pending {
        id: String,
        name: String,
        make: String,
        model: String,
        enabled: bool,
        primary: bool,
        x: Option<f32>,
        y: Option<f32>,
        width: Option<f32>,
        height: Option<f32>,
        scale: f32,
    }

    fn finish(pending: Pending, monitors: &mut Vec<MonitorInfo>) {
        if !pending.enabled {
            return;
        }
        let (Some(x), Some(y), Some(width), Some(height)) =
            (pending.x, pending.y, pending.width, pending.height)
        else {
            return;
        };
        let scale = if pending.scale.is_finite() && pending.scale > 0.0 {
            pending.scale
        } else {
            1.0
        };
        let name = match (pending.make.trim(), pending.model.trim()) {
            (make, model) if !make.is_empty() && !model.is_empty() => {
                format!("{make} {model}")
            }
            (make, _) if !make.is_empty() => make.to_owned(),
            (_, model) if !model.is_empty() => model.to_owned(),
            _ => pending.name,
        };
        monitors.push(MonitorInfo {
            id: pending.id,
            name,
            x,
            y,
            width: width / scale,
            height: height / scale,
            scale_factor: scale,
            is_primary: pending.primary,
        });
    }

    let mut monitors = Vec::new();
    let mut current: Option<Pending> = None;
    for raw_line in output.lines() {
        let line = strip_ansi(raw_line);
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let indented = line.chars().next().is_some_and(char::is_whitespace);
        if !indented {
            if let Some(previous) = current.take() {
                finish(previous, &mut monitors);
            }
            let id = trimmed
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .trim_matches('"')
                .to_owned();
            let quoted_name = trimmed
                .find('"')
                .and_then(|start| {
                    trimmed[start + 1..]
                        .find('"')
                        .map(|end| &trimmed[start + 1..start + 1 + end])
                })
                .map(str::trim)
                .filter(|name| !name.is_empty());
            if !id.is_empty() {
                current = Some(Pending {
                    id: id.clone(),
                    name: quoted_name.unwrap_or(&id).to_owned(),
                    enabled: !trimmed.contains("(disabled)"),
                    primary: trimmed.contains("(primary)"),
                    scale: 1.0,
                    ..Pending::default()
                });
            }
            continue;
        }
        let Some(pending) = current.as_mut() else {
            continue;
        };
        if let Some(value) = trimmed.strip_prefix("Enabled:") {
            pending.enabled = matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "yes" | "true" | "1"
            );
        } else if trimmed.contains("(disabled)") {
            pending.enabled = false;
        } else if trimmed.contains("(enabled)") {
            pending.enabled = true;
        } else if let Some(value) = trimmed.strip_prefix("Position:") {
            if let Some((x, y)) = parse_signed_coordinate_pair(value.trim()) {
                pending.x = Some(x);
                pending.y = Some(y);
            }
        } else if let Some(value) = trimmed.strip_prefix("Scale:") {
            pending.scale = parse_scale_factor(value).unwrap_or(1.0);
        } else if let Some(value) = trimmed.strip_prefix("Make:") {
            pending.make = value.trim().to_owned();
        } else if let Some(value) = trimmed.strip_prefix("Model:") {
            pending.model = value.trim().to_owned();
        } else {
            let lowercase = trimmed.to_ascii_lowercase();
            if let Some(value) = lowercase
                .strip_prefix("primary:")
                .or_else(|| lowercase.strip_prefix("xwayland primary:"))
            {
                pending.primary = matches!(value.trim(), "yes" | "true" | "1");
            }
        }
        if trimmed.contains("(current)") || trimmed.starts_with("Current mode:") {
            let token = trimmed
                .split_whitespace()
                .find(|token| token.contains('x'))
                .unwrap_or_default()
                .trim_end_matches(',');
            if let Some((width, height)) = parse_resolution(token) {
                pending.width = Some(width);
                pending.height = Some(height);
            }
        }
    }
    if let Some(previous) = current {
        finish(previous, &mut monitors);
    }
    monitors
}

#[cfg(target_os = "linux")]
fn parse_scale_factor(value: &str) -> Option<f32> {
    let raw = value.split_whitespace().next()?;
    let is_percentage = raw.ends_with('%');
    let numeric = raw.trim_end_matches('%').parse::<f32>().ok()?;
    let scale = if is_percentage {
        numeric / 100.0
    } else {
        numeric
    };
    (scale.is_finite() && scale > 0.0).then_some(scale)
}

#[cfg(target_os = "linux")]
fn parse_kscreen_outputs(output: &str) -> Vec<MonitorInfo> {
    let mut monitors = Vec::new();
    let mut current: Option<MonitorInfo> = None;
    let mut enabled = false;
    for raw_line in output.lines() {
        let line = strip_ansi(raw_line);
        let trimmed = line.trim();
        if let Some(header) = trimmed.strip_prefix("Output:") {
            if enabled {
                if let Some(monitor) = current.take() {
                    monitors.push(monitor);
                }
            }
            let fields: Vec<&str> = header.split_whitespace().collect();
            let name = fields.get(1).copied().unwrap_or("Display").to_owned();
            enabled = fields.iter().any(|field| *field == "enabled");
            let primary = fields
                .windows(2)
                .any(|pair| pair[0] == "priority" && pair[1] == "1");
            current = Some(MonitorInfo {
                id: name.clone(),
                name,
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 0.0,
                scale_factor: 1.0,
                is_primary: primary,
            });
            continue;
        }
        let Some(monitor) = current.as_mut() else {
            continue;
        };
        if let Some(value) = trimmed.strip_prefix("Geometry:") {
            let fields: Vec<&str> = value.split_whitespace().collect();
            if fields.len() >= 2
                && let Some((x, y)) = parse_signed_coordinate_pair(fields[0])
                && let Some((width, height)) = parse_resolution(fields[1])
            {
                monitor.x = x;
                monitor.y = y;
                monitor.width = width;
                monitor.height = height;
            }
        } else if let Some(value) = trimmed.strip_prefix("Scale:") {
            monitor.scale_factor = parse_scale_factor(value).unwrap_or(1.0);
        }
    }
    if enabled {
        if let Some(mut monitor) = current {
            if monitor.width > 0.0 && monitor.height > 0.0 {
                monitor.width /= monitor.scale_factor;
                monitor.height /= monitor.scale_factor;
                monitors.push(monitor);
            }
        }
    }
    monitors
}

#[cfg(target_os = "linux")]
fn parse_resolution(value: &str) -> Option<(f32, f32)> {
    let value = value
        .trim_matches(|character: char| !character.is_ascii_alphanumeric() && character != 'x')
        .split('@')
        .next()
        .unwrap_or_default()
        .trim_end_matches(',');
    let (width, height) = value.split_once('x')?;
    Some((width.parse().ok()?, height.parse().ok()?))
}

#[cfg(target_os = "linux")]
fn parse_signed_coordinate_pair(value: &str) -> Option<(f32, f32)> {
    let value = value.trim().trim_end_matches(',');
    if let Some((x, y)) = value.split_once(',') {
        return Some((x.trim().parse().ok()?, y.trim().parse().ok()?));
    }
    let split = value
        .char_indices()
        .skip(1)
        .find(|(_, character)| *character == '+' || *character == '-')
        .map(|(index, _)| index)?;
    let x = value[..split].parse::<f32>().ok()?;
    let y = value[split..].parse::<f32>().ok()?;
    Some((x, y))
}

#[cfg(target_os = "linux")]
fn sort_monitors(mut monitors: Vec<MonitorInfo>) -> Vec<MonitorInfo> {
    monitors.sort_by(|left, right| {
        right
            .is_primary
            .cmp(&left.is_primary)
            .then_with(|| left.x.total_cmp(&right.x))
            .then_with(|| left.y.total_cmp(&right.y))
    });
    monitors
}

#[cfg(target_os = "linux")]
fn strip_ansi(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(character) = chars.next() {
        if character == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            for next in chars.by_ref() {
                if ('@'..='~').contains(&next) {
                    break;
                }
            }
        } else {
            output.push(character);
        }
    }
    output
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn run_command_with_timeout(program: &str, args: &[&str]) -> Option<String> {
    let mut child = Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return None;
                }
                return child
                    .wait_with_output()
                    .ok()
                    .map(|output| String::from_utf8_lossy(&output.stdout).into_owned());
            }
            Ok(None) if started.elapsed() < COMMAND_TIMEOUT => {
                thread::sleep(Duration::from_millis(12));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn enumerate_macos_monitors() -> Vec<MonitorInfo> {
    // JXA uses the system AppKit screen list so this remains dependency-free.
    // NSScreen frames are already expressed in points, matching egui's geometry.
    let script = r#"
ObjC.import('AppKit');
var screens = $.NSScreen.screens;
var main = $.NSScreen.mainScreen;
var mainFrame = main.frame;
for (var i = 0; i < screens.count; i++) {
  var screen = screens.objectAtIndex(i);
  var frame = screen.frame;
  var number = screen.deviceDescription.objectForKey('NSScreenNumber');
  var id = String(number.stringValue);
  var name = ObjC.unwrap(screen.localizedName);
  var primary = screen.isEqual(main) ? '1' : '0';
  // Convert AppKit's bottom-left Y origin to the top-left desktop coordinates
  // used by the windowing APIs, anchored to the main display.
  var x = frame.origin.x;
  var y = mainFrame.origin.y + mainFrame.size.height - (frame.origin.y + frame.size.height);
  console.log([id, name, x, y, frame.size.width, frame.size.height, primary].join('\t'));
}
"#;
    let Some(output) = run_command_with_timeout("osascript", &["-l", "JavaScript", "-e", script])
    else {
        return Vec::new();
    };
    output
        .lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.trim().split('\t').collect();
            if fields.len() < 7 {
                return None;
            }
            let monitor = MonitorInfo {
                id: fields[0].to_owned(),
                name: fields[1].to_owned(),
                x: fields[2].parse().ok()?,
                y: fields[3].parse().ok()?,
                width: fields[4].parse().ok()?,
                height: fields[5].parse().ok()?,
                scale_factor: 1.0,
                is_primary: fields[6] == "1",
            };
            (monitor.width > 0.0 && monitor.height > 0.0).then_some(monitor)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_missing_preference_to_primary_then_first_available() {
        let monitors = vec![
            MonitorInfo {
                id: "DISPLAY2".into(),
                name: "DISPLAY2".into(),
                x: 1920.0,
                y: 0.0,
                width: 1920.0,
                height: 1080.0,
                scale_factor: 1.0,
                is_primary: false,
            },
            MonitorInfo {
                id: "DISPLAY1".into(),
                name: "DISPLAY1".into(),
                x: 0.0,
                y: 0.0,
                width: 1920.0,
                height: 1080.0,
                scale_factor: 1.0,
                is_primary: true,
            },
        ];
        assert_eq!(
            resolve_default_monitor(&monitors, "DISPLAY2").map(|monitor| monitor.id.as_str()),
            Some("DISPLAY2")
        );
        assert_eq!(
            resolve_default_monitor(&monitors, "disconnected").map(|monitor| monitor.id.as_str()),
            Some("DISPLAY1")
        );
        assert_eq!(
            resolve_default_monitor(&monitors, "primary").map(|monitor| monitor.id.as_str()),
            Some("DISPLAY1")
        );
        let no_primary = vec![MonitorInfo {
            id: "FIRST".into(),
            name: "FIRST".into(),
            x: 0.0,
            y: 0.0,
            width: 1280.0,
            height: 720.0,
            scale_factor: 1.0,
            is_primary: false,
        }];
        assert_eq!(
            resolve_default_monitor(&no_primary, "disconnected").map(|monitor| monitor.id.as_str()),
            Some("FIRST")
        );
    }

    #[test]
    fn current_window_preference_chooses_monitor_with_largest_overlap() {
        let monitors = vec![
            MonitorInfo {
                id: "LEFT".into(),
                name: "LEFT".into(),
                x: 0.0,
                y: 0.0,
                width: 1920.0,
                height: 1080.0,
                scale_factor: 1.0,
                is_primary: true,
            },
            MonitorInfo {
                id: "RIGHT".into(),
                name: "RIGHT".into(),
                x: 1920.0,
                y: 0.0,
                width: 1920.0,
                height: 1080.0,
                scale_factor: 1.0,
                is_primary: false,
            },
        ];
        let window = eframe::egui::Rect::from_min_size(
            eframe::egui::pos2(1500.0, 100.0),
            eframe::egui::vec2(1000.0, 800.0),
        );
        assert_eq!(
            resolve_default_monitor_at(&monitors, CURRENT_WINDOW_MONITOR, Some(window))
                .map(|monitor| monitor.id.as_str()),
            Some("RIGHT")
        );
        assert_eq!(
            resolve_default_monitor_at(&monitors, CURRENT_WINDOW_MONITOR, None)
                .map(|monitor| monitor.id.as_str()),
            Some("LEFT")
        );
    }

    #[test]
    fn monitor_padding_insets_each_edge() {
        let monitor = MonitorInfo {
            id: "DISPLAY".into(),
            name: "DISPLAY".into(),
            x: -1920.0,
            y: 0.0,
            width: 1920.0,
            height: 1080.0,
            scale_factor: 1.0,
            is_primary: true,
        };
        let inset = monitor.inset_bounds(100.0);
        assert_eq!(inset.min, eframe::egui::pos2(-1820.0, 100.0));
        assert_eq!(inset.max, eframe::egui::pos2(-100.0, 980.0));
        assert_eq!(inset.size(), eframe::egui::vec2(1720.0, 880.0));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn parses_xrandr_primary_and_negative_monitor_positions() {
        let output = r#"
Screen 0: minimum 8 x 8, current 3840 x 1080, maximum 32767 x 32767
DP-1 connected primary 1920x1080+0+0 (normal)
HDMI-1 connected 1920x1080-1920+0 (normal)
"#;
        let monitors = parse_xrandr_outputs(output);
        assert_eq!(monitors.len(), 2);
        assert!(monitors[0].is_primary);
        assert_eq!(monitors[1].x, -1920.0);
        assert_eq!(monitors[1].width, 1920.0);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn parses_wlr_randr_output_geometry_and_scale() {
        let output = r#"
DP-1 "Primary display" (enabled)
  Enabled: yes
  Current mode: 2560x1440 @ 60.000 Hz
  Position: 0,0
  Scale: 125%
  Xwayland primary: true
HDMI-A-1 "Secondary display" (enabled)
  Enabled: yes
  Current mode: 1920x1080 @ 60.000 Hz
  Position: 2048,0
  Scale: 1
DP-2 "Disconnected display" (disabled)
  Enabled: no
  Current mode: 1920x1080 @ 60.000 Hz
  Position: 0,0
  Scale: 1
"#;
        let monitors = parse_wlr_style_outputs(output);
        assert_eq!(monitors.len(), 2);
        assert_eq!(monitors[0].name, "Primary display");
        assert!(monitors[0].is_primary);
        assert_eq!(monitors[0].width, 2048.0);
        assert_eq!(monitors[0].scale_factor, 1.25);
        assert_eq!(monitors[1].name, "Secondary display");
        assert_eq!(monitors[1].x, 2048.0);
        assert_eq!(parse_scale_factor("100%"), Some(1.0));
        assert_eq!(parse_scale_factor("1.5"), Some(1.5));
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn parses_cosmic_randr_percent_scale_and_primary_monitor() {
        let output = r#"
DP-2 (enabled)
  Make: Dell Inc.
  Model: AW3225QF
  Position: 0,0
  Scale: 125%
  Xwayland primary: true
  Modes:
    3840x2160 @ 239.991 Hz (current) (preferred)
HDMI-A-1 (enabled)
  Make: Samsung
  Model: Display
  Position: 3072,0
  Scale: 100%
  Xwayland primary: false
  Modes:
    1920x1080 @ 60.000 Hz (current)
"#;
        let monitors = parse_wlr_style_outputs(output);
        assert_eq!(monitors.len(), 2);
        assert_eq!(monitors[0].id, "DP-2");
        assert_eq!(monitors[0].name, "Dell Inc. AW3225QF");
        assert!(monitors[0].is_primary);
        assert_eq!(monitors[0].width, 3072.0);
        assert_eq!(monitors[0].height, 1728.0);
        assert_eq!(monitors[0].scale_factor, 1.25);
        assert_eq!(monitors[1].x, 3072.0);
        assert!(!monitors[1].is_primary);
    }
}
