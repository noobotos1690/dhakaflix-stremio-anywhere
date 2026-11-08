fn main() {
    slint_build::compile("ui/main-window.slint").expect("Slint UI failed to compile");

    #[cfg(windows)]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/app.ico");
        res.set("FileDescription", "DhakaFlix Relay");
        res.set("ProductName", "DhakaFlix Relay");
        res.set("CompanyName", "DhakaFlix");
        res.set("FileVersion", "2.0.0.0");
        res.set("ProductVersion", "2.0.0");
        let _ = res.compile();
    }
}
