fn main() {
    // Re-run build script if the icon/assets change.
    println!("cargo:rerun-if-changed=assets");

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
    res.set_icon("assets/icon.ico");
    res.compile().expect("Failed to compile Windows resources");
}

#[cfg(target_os = "macos")]
fn build_macos() {
    // macOS application bundling is normally handled by the packaging
    // step rather than by rustc/build.rs itself.
    //
    // For example, an .icns icon and Info.plist can be added when
    // creating the .app bundle.
    println!("cargo:rerun-if-changed=assets/icon.icns");
    println!("cargo:rerun-if-changed=assets/Info.plist");
}

#[cfg(target_os = "linux")]
fn build_linux() {
    // Linux does not embed application icons into the executable
    // like Windows does.
    //
    // The icon should normally be installed alongside the application
    // and referenced by the desktop entry (.desktop file).
    println!("cargo:rerun-if-changed=assets/icon.png");
}
