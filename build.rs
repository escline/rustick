fn main() {
    // Declare the custom cfg so rustc's check-cfg doesn't warn about it
    println!("cargo::rustc-check-cfg=cfg(has_icon_png)");

    // Re-run this script whenever the icon files change
    println!("cargo:rerun-if-changed=assets/icon.png");
    println!("cargo:rerun-if-changed=assets/icon.ico");

    // Set a cfg flag so main.rs can conditionally compile the icon embed
    if std::path::Path::new("assets/icon.png").exists() {
        println!("cargo:rustc-cfg=has_icon_png");
    }

    // Embed the .ico into the .exe for Windows Explorer / taskbar previews
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        if let Err(e) = res.compile() {
            eprintln!("winres: {e} (place assets/icon.ico to embed an exe icon)");
        }
    }
}
