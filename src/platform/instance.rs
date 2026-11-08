//! Optional per-user single-instance coordination with local file-open forwarding.
//!
//! A process-wide OS lock chooses the primary process. The primary listens only on
//! loopback TCP and receives length-prefixed UTF-8 paths from later launches. The
//! transport is intentionally local-only; no network interface is exposed.

use std::fs::{self, File};
#[cfg(not(windows))]
use std::fs::OpenOptions;
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, Mutex};
use eframe::egui;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const TOKEN_LEN: usize = 32;

pub enum LaunchOutcome {
    Primary(InstanceServer),
    Forwarded,
}

pub struct InstanceServer {
    receiver: Receiver<Vec<PathBuf>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    info_path: PathBuf,
    repaint_context: Arc<Mutex<Option<egui::Context>>>,
    _lock: InstanceLock,
}

impl InstanceServer {
    /// Attach the UI context so IPC traffic wakes an otherwise idle application.
    pub fn set_repaint_context(&self, ctx: egui::Context) {
        if let Ok(mut context) = self.repaint_context.lock() {
            *context = Some(ctx);
        }
    }

    pub fn drain_requests(&self) -> Vec<Vec<PathBuf>> {
        let mut requests = Vec::new();
        loop {
            match self.receiver.try_recv() {
                Ok(paths) => requests.push(paths),
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        }
        requests
    }
}

impl Drop for InstanceServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let _ = fs::remove_file(&self.info_path);
        // `_lock` is released automatically after Drop returns.
    }
}

enum LockState {
    Acquired(InstanceLock),
    AlreadyRunning,
}

#[cfg(unix)]
struct InstanceLock {
    _file: File,
}

#[cfg(windows)]
struct InstanceLock {
    handle: *mut std::ffi::c_void,
}

#[cfg(not(any(unix, windows)))]
struct InstanceLock {
    path: PathBuf,
}

#[cfg(unix)]
fn acquire_lock(path: &Path, _mutex_name: &str) -> io::Result<LockState> {
    use std::os::fd::AsRawFd;
    use std::os::raw::c_int;

    unsafe extern "C" {
        fn flock(fd: c_int, operation: c_int) -> c_int;
    }
    const LOCK_EX: c_int = 2;
    const LOCK_NB: c_int = 4;

    let file = OpenOptions::new().create(true).read(true).write(true).open(path)?;
    let result = unsafe { flock(file.as_raw_fd(), LOCK_EX | LOCK_NB) };
    if result == 0 {
        Ok(LockState::Acquired(InstanceLock { _file: file }))
    } else {
        let error = io::Error::last_os_error();
        if matches!(error.raw_os_error(), Some(11) | Some(35)) || error.kind() == io::ErrorKind::WouldBlock {
            Ok(LockState::AlreadyRunning)
        } else {
            Err(error)
        }
    }
}

#[cfg(windows)]
fn acquire_lock(_path: &Path, mutex_name: &str) -> io::Result<LockState> {
    use std::ffi::c_void;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateMutexW(
            attributes: *mut c_void,
            initial_owner: i32,
            name: *const u16,
        ) -> *mut c_void;
        fn GetLastError() -> u32;
        fn CloseHandle(handle: *mut c_void) -> i32;
    }
    const ERROR_ALREADY_EXISTS: u32 = 183;

    let wide: Vec<u16> = mutex_name.encode_utf16().chain(std::iter::once(0)).collect();
    let handle = unsafe { CreateMutexW(std::ptr::null_mut(), 1, wide.as_ptr()) };
    if handle.is_null() {
        return Err(io::Error::last_os_error());
    }
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        unsafe { CloseHandle(handle); }
        Ok(LockState::AlreadyRunning)
    } else {
        Ok(LockState::Acquired(InstanceLock { handle }))
    }
}

#[cfg(windows)]
impl Drop for InstanceLock {
    fn drop(&mut self) {
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn ReleaseMutex(handle: *mut std::ffi::c_void) -> i32;
            fn CloseHandle(handle: *mut std::ffi::c_void) -> i32;
        }
        unsafe {
            let _ = ReleaseMutex(self.handle);
            let _ = CloseHandle(self.handle);
        }
    }
}

#[cfg(not(any(unix, windows)))]
fn acquire_lock(path: &Path, _mutex_name: &str) -> io::Result<LockState> {
    match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(_) => Ok(LockState::Acquired(InstanceLock { path: path.to_path_buf() })),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(LockState::AlreadyRunning),
        Err(error) => Err(error),
    }
}

#[cfg(not(any(unix, windows)))]
impl Drop for InstanceLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Enable single-instance behavior for this launch. A forwarded process exits after
/// the primary acknowledges the request; a primary receives later path batches.
pub fn launch_or_forward(settings_path: &Path, paths: Vec<PathBuf>) -> io::Result<LaunchOutcome> {
    let parent = settings_path.parent().filter(|path| !path.as_os_str().is_empty()).unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let lock_path = parent.join("jpeg-viewer.instance.lock");
    let info_path = parent.join("jpeg-viewer.instance.info");
    let hash = stable_path_hash(settings_path);
    let mutex_name = format!("Local\\com.jpegviewer.JpegViewer.{hash:016x}");

    match acquire_lock(&lock_path, &mutex_name)? {
        LockState::AlreadyRunning => {
            forward_to_primary(&info_path, paths)?;
            Ok(LaunchOutcome::Forwarded)
        }
        LockState::Acquired(lock) => {
            let listener = TcpListener::bind(("127.0.0.1", 0))?;
            listener.set_nonblocking(true)?;
            let address = listener.local_addr()?;
            let token = format!("{:032x}", rand::random::<u128>());
            write_instance_info(&info_path, address, &token, std::process::id())?;

            let (sender, receiver) = mpsc::channel();
            let stop = Arc::new(AtomicBool::new(false));
            let thread_stop = Arc::clone(&stop);
            let thread_token = token.clone();
            let repaint_context = Arc::new(Mutex::new(None::<egui::Context>));
            let thread_repaint_context = Arc::clone(&repaint_context);
            let worker = thread::Builder::new()
                .name("jpeg-viewer-instance-ipc".into())
                .spawn(move || {
                    while !thread_stop.load(Ordering::Relaxed) {
                        match listener.accept() {
                            Ok((stream, _)) => {
                                if let Some(paths) = read_request(stream, &thread_token) {
                                    if sender.send(paths).is_err() {
                                        break;
                                    }
                                    // Wake the UI thread even when the viewer is idle.
                                    if let Ok(context) = thread_repaint_context.lock() {
                                        if let Some(context) = context.as_ref() {
                                            context.request_repaint();
                                        }
                                    }
                                }
                            }
                            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                                thread::sleep(Duration::from_millis(20));
                            }
                            Err(_) => thread::sleep(Duration::from_millis(50)),
                        }
                    }
                })?;

            Ok(LaunchOutcome::Primary(InstanceServer {
                receiver,
                stop,
                thread: Some(worker),
                info_path,
                repaint_context,
                _lock: lock,
            }))
        }
    }
}

fn stable_path_hash(path: &Path) -> u64 {
    // FNV-1a: stable across launches and avoids a dependency on DefaultHasher's seed.
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in path.to_string_lossy().as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn write_instance_info(path: &Path, address: SocketAddr, token: &str, process_id: u32) -> io::Result<()> {
    let temp = path.with_extension("info.tmp");
    let mut file = File::create(&temp)?;
    writeln!(file, "{}", address.port())?;
    writeln!(file, "{token}")?;
    writeln!(file, "{process_id}")?;
    file.sync_all()?;
    // Windows does not replace an existing destination with `rename`. A previous
    // process may have left its endpoint file behind after a crash, even though
    // the OS lock itself was released automatically.
    if path.exists() {
        fs::remove_file(path)?;
    }
    fs::rename(temp, path)
}

fn read_instance_info(path: &Path) -> io::Result<(u16, String, u32)> {
    let text = fs::read_to_string(path)?;
    let mut lines = text.lines();
    let port = lines.next().and_then(|value| value.parse::<u16>().ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid instance port"))?;
    let token = lines.next().unwrap_or_default().to_owned();
    if token.len() != TOKEN_LEN || !token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid instance token"));
    }
    let process_id = lines.next().and_then(|value| value.parse::<u32>().ok()).unwrap_or(0);
    Ok((port, token, process_id))
}

#[cfg(windows)]
fn allow_primary_to_set_foreground(process_id: u32) {
    if process_id == 0 {
        return;
    }
    #[link(name = "user32")]
    unsafe extern "system" {
        fn AllowSetForegroundWindow(process_id: u32) -> i32;
    }
    // The helper process was just launched by the foreground file manager. Let
    // the already-running primary process activate its window in response.
    unsafe {
        let _ = AllowSetForegroundWindow(process_id);
    }
}

#[cfg(not(windows))]
fn allow_primary_to_set_foreground(_process_id: u32) {}

fn forward_to_primary(info_path: &Path, paths: Vec<PathBuf>) -> io::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut last_error = None;
    while Instant::now() < deadline {
        match read_instance_info(info_path) {
            Ok((port, token, process_id)) => {
                match TcpStream::connect_timeout(&SocketAddr::from(([127, 0, 0, 1], port)), Duration::from_millis(250)) {
                    Ok(mut stream) => {
                        allow_primary_to_set_foreground(process_id);
                        stream.set_write_timeout(Some(Duration::from_secs(2)))?;
                        stream.set_read_timeout(Some(Duration::from_secs(2)))?;
                        stream.write_all(token.as_bytes())?;
                        stream.write_all(&(paths.len() as u32).to_le_bytes())?;
                        for path in &paths {
                            let bytes = path.to_string_lossy().as_bytes().to_vec();
                            let len = u32::try_from(bytes.len()).map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path is too long"))?;
                            stream.write_all(&len.to_le_bytes())?;
                            stream.write_all(&bytes)?;
                        }
                        stream.flush()?;
                        let mut ack = [0_u8; 1];
                        stream.read_exact(&mut ack)?;
                        if ack[0] == 1 { return Ok(()); }
                        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "primary rejected the request"));
                    }
                    Err(error) => last_error = Some(error),
                }
            }
            Err(error) => last_error = Some(error),
        }
        thread::sleep(Duration::from_millis(50));
    }
    Err(last_error.unwrap_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "primary instance did not become ready")))
}

fn read_request(mut stream: TcpStream, expected_token: &str) -> Option<Vec<PathBuf>> {
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
    stream.set_write_timeout(Some(Duration::from_secs(2))).ok()?;
    let mut token = [0_u8; TOKEN_LEN];
    stream.read_exact(&mut token).ok()?;
    if token.as_slice() != expected_token.as_bytes() { let _ = stream.write_all(&[0]); return None; }
    let mut count = [0_u8; 4];
    stream.read_exact(&mut count).ok()?;
    let count = u32::from_le_bytes(count).min(4096) as usize;
    let mut paths = Vec::with_capacity(count);
    for _ in 0..count {
        let mut len = [0_u8; 4];
        stream.read_exact(&mut len).ok()?;
        let len = u32::from_le_bytes(len) as usize;
        if len > 1024 * 1024 { let _ = stream.write_all(&[0]); return None; }
        let mut bytes = vec![0_u8; len];
        stream.read_exact(&mut bytes).ok()?;
        let value = String::from_utf8(bytes).ok()?;
        paths.push(PathBuf::from(value));
    }
    stream.write_all(&[1]).ok()?;
    Some(paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_launch_forwards_selected_paths_to_primary() {
        let unique = format!("jpeg-viewer-instance-test-{:032x}", rand::random::<u128>());
        let directory = std::env::temp_dir().join(unique);
        fs::create_dir_all(&directory).unwrap();
        let settings_path = directory.join("settings.ini");

        let primary = launch_or_forward(&settings_path, Vec::new()).unwrap();
        let server = match primary {
            LaunchOutcome::Primary(server) => server,
            LaunchOutcome::Forwarded => panic!("first process unexpectedly forwarded"),
        };
        let expected = vec![
            directory.join("first image.jpg"),
            directory.join("second image.png"),
        ];

        let second = launch_or_forward(&settings_path, expected.clone()).unwrap();
        assert!(matches!(second, LaunchOutcome::Forwarded));

        let deadline = Instant::now() + Duration::from_secs(1);
        let received = loop {
            let requests = server.drain_requests();
            if let Some(paths) = requests.into_iter().next() {
                break paths;
            }
            assert!(Instant::now() < deadline, "primary did not receive forwarded paths");
            thread::sleep(Duration::from_millis(10));
        };
        assert_eq!(received, expected);

        drop(server);
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn separate_file_manager_launches_forward_every_selected_path() {
        let unique = format!("jpeg-viewer-instance-batch-test-{:032x}", rand::random::<u128>());
        let directory = std::env::temp_dir().join(unique);
        fs::create_dir_all(&directory).unwrap();
        let settings_path = directory.join("settings.ini");

        let primary = launch_or_forward(&settings_path, Vec::new()).unwrap();
        let server = match primary {
            LaunchOutcome::Primary(server) => server,
            LaunchOutcome::Forwarded => panic!("first process unexpectedly forwarded"),
        };

        // Windows Explorer can launch one helper process per selected file when
        // the association uses the Document selection model. Exercise that shape.
        let expected: Vec<PathBuf> = (1..=4)
            .map(|index| directory.join(format!("selected-{index}.jpg")))
            .collect();
        for path in &expected {
            let result = launch_or_forward(&settings_path, vec![path.clone()]).unwrap();
            assert!(matches!(result, LaunchOutcome::Forwarded));
        }

        let deadline = Instant::now() + Duration::from_secs(2);
        let mut received = Vec::new();
        while received.len() < expected.len() {
            for paths in server.drain_requests() {
                received.extend(paths);
            }
            assert!(Instant::now() < deadline, "primary did not receive every forwarded path");
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(received, expected);

        drop(server);
        let _ = fs::remove_dir_all(directory);
    }

}
