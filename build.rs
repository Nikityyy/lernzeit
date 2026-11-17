fn main() {
    println!("cargo:rerun-if-changed=installer/lernzeit.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("installer/lernzeit.ico")
            .set("ProductName", "Lernzeit")
            .set("FileDescription", "Lernzeit")
            .set("OriginalFilename", "Lernzeit.exe")
            .compile()
            .expect("Could not compile the Windows icon and version information");
    }
}
