fn main() {
    // Re-run the build script when application assets change.
    println!("cargo:rerun-if-changed=assets/icons");
    println!("cargo:rerun-if-env-changed=CARGO_PKG_VERSION");

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
