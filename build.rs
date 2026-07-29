fn main() {
    #[cfg(target_os = "windows")]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.png");
        res.compile().unwrap();
    }
    
    #[cfg(target_os = "macos")]
    {
        // macOS specific build steps
        // Create Info.plist, set bundle name, etc.
    }
    
    #[cfg(target_os = "linux")]
    {
        // Linux specific build steps
    }
}