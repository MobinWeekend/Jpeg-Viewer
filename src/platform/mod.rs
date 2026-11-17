//! Operating-system integration that is independent of the visual theme.

pub mod instance;
pub mod power;

pub mod thumbnails;

#[cfg(target_os = "linux")]
pub mod linux_thumbnails;

// Temporary egui 0.33 / winit 0.30 Wayland pointer-release workaround.
#[cfg(target_os = "linux")]
pub mod wayland_pointer_recovery;
