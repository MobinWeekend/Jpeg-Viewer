fn main() {
    // Re-run the build script when application assets change.
    println!("cargo:rerun-if-changed=assets/icons");

    #[cfg(target_os = "windows")]
    build_windows();

    #[cfg(target_os = "macos")]
    build_macos();

    #[cfg(target_os = "linux")]
    build_linux();
}

#[cfg(target_os = "windows")]
fn build_windows() {
    let mut res = winresource::WindowsResource::new();

    res.set_icon("assets/icons/icon.ico");

    res.compile().expect("Failed to compile Windows resources");
}

#[cfg(target_os = "macos")]
fn build_macos() {
    println!("cargo:rerun-if-changed=packaging/macos/Info.plist");
    println!("cargo:rerun-if-changed=packaging/macos/jpeg_viewer.icns");
}

#[cfg(target_os = "linux")]
fn build_linux() {
    // Linux icons are installed separately by the packaging system.
    println!("cargo:rerun-if-changed=assets/icons/icon.png");
}
