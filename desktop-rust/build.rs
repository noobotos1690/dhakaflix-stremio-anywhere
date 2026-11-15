fn main() {
    println!("cargo:rerun-if-changed=ui");
    println!("cargo:rerun-if-changed=assets");

    slint_build::compile("ui/main-window.slint").expect("Slint compilation failed");

    #[cfg(windows)]
    {
        // The icon is optional so a source checkout without generated artwork still builds.
        if std::path::Path::new("assets/app.ico").exists() {
            let mut res = winres::WindowsResource::new();
            res.set_icon("assets/app.ico");
            res.set("ProductName", "DhakaFlix Relay");
            res.set("FileDescription", "DhakaFlix Relay");
            res.set("CompanyName", "DhakaFlix");
            res.set("LegalCopyright", "DhakaFlix Relay contributors");
            if let Err(error) = res.compile() {
                println!("cargo:warning=could not embed the application icon: {error}");
            }
        } else {
            println!("cargo:warning=assets/app.ico is missing; building without an embedded icon");
        }
    }
}
