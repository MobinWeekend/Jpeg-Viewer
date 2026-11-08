//! Best-effort system sleep inhibition while a slideshow is running.

#[derive(Default)]
pub struct PowerInhibitor {
    active: bool,
    #[cfg(not(windows))]
    child: Option<std::process::Child>,
}

impl PowerInhibitor {
    pub fn set_active(&mut self, active: bool) {
        if self.active == active {
            #[cfg(windows)]
            if active {
                Self::set_windows_execution_state(true);
            }
            return;
        }
        self.active = active;
        #[cfg(windows)]
        Self::set_windows_execution_state(active);
        #[cfg(target_os = "macos")]
        {
            if active {
                self.child = std::process::Command::new("caffeinate")
                    .args(["-dimsu"])
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn()
                    .ok();
            } else {
                self.stop_child();
            }
        }
        #[cfg(target_os = "linux")]
        {
            if active {
                self.child = std::process::Command::new("systemd-inhibit")
                    .args([
                        "--what=sleep",
                        "--mode=block",
                        "--why=JPEG Viewer slideshow",
                        "sleep",
                        "infinity",
                    ])
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn()
                    .ok();
            } else {
                self.stop_child();
            }
        }
        #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
        let _ = active;
    }

    #[cfg(windows)]
    fn set_windows_execution_state(active: bool) {
        use windows::Win32::System::Power::{
            ES_CONTINUOUS, ES_DISPLAY_REQUIRED, ES_SYSTEM_REQUIRED, SetThreadExecutionState,
        };
        unsafe {
            if active {
                let _ = SetThreadExecutionState(
                    ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED,
                );
            } else {
                let _ = SetThreadExecutionState(ES_CONTINUOUS);
            }
        }
    }

    #[cfg(not(windows))]
    fn stop_child(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for PowerInhibitor {
    fn drop(&mut self) {
        #[cfg(windows)]
        Self::set_windows_execution_state(false);
        #[cfg(not(windows))]
        self.stop_child();
    }
}
