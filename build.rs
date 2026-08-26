fn main() {
    // Windows executable icon.
    println!("cargo:rerun-if-changed=assets/icon.ico");

    #[cfg(target_os = "windows")]
    build_windows();
}

#[cfg(target_os = "windows")]
fn build_windows() {
    let mut res = winresource::WindowsResource::new();

    res.set_icon("assets/icon.ico");

    res.compile().expect("Failed to compile Windows resources");
}
