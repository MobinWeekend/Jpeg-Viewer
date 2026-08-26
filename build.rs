fn main() {
    #[cfg(target_os = "windows")]
    build_windows();

    #[cfg(target_os = "macos")]
    build_macos();

    #[cfg(target_os = "linux")]
    build_linux();
}

#[cfg(target_os = "windows")]
fn build_windows() {
    println!("cargo:rerun-if-changed=assets/icons/icon.ico");

    let mut res = winresource::WindowsResource::new();

    res.set_icon("assets/icons/icon.ico");

    res.compile().expect("Failed to compile Windows resources");
}

#[cfg(target_os = "macos")]
fn build_macos() {
    println!("cargo:rerun-if-changed=packaging/macos/jpeg_viewer.icns");
    println!("cargo:rerun-if-changed=packaging/macos/Info.plist");
}

#[cfg(target_os = "linux")]
fn build_linux() {
    println!("cargo:rerun-if-changed=packaging/icons/com.jpegviewer.JpegViewer.png");
}
