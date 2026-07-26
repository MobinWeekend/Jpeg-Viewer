use winresource::WindowsResource;

fn main() {
    #[cfg(target_os = "windows")]
    {
        WindowsResource::new()
            .set_icon("assets/icon.ico")
            .compile()
            .unwrap();
    }
}