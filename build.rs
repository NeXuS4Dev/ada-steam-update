//! Build script: renders the procedural app icon into a multi-size .ico and
//! embeds it (plus version metadata) into the Windows executable.

#[allow(dead_code)]
mod icon_art {
    include!("src/icon_art.rs");
}

fn main() {
    println!("cargo:rerun-if-changed=src/icon_art.rs");
    println!("cargo:rerun-if-changed=build.rs");

    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let ico_path = out.join("app.ico");
    write_ico(&ico_path);

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        #[cfg(windows)]
        {
            let mut res = winresource::WindowsResource::new();
            res.set_icon(ico_path.to_str().unwrap())
                .set("ProductName", "Ada Steam Update")
                .set("FileDescription", "Ada Steam Update")
                .set("LegalCopyright", "MIT License");
            if let Err(e) = res.compile() {
                println!("cargo:warning=could not embed Windows resources: {e}");
            }
        }
    }
}

fn write_ico(path: &std::path::Path) {
    use image::codecs::ico::{IcoEncoder, IcoFrame};
    let frames: Vec<IcoFrame> = [256u32, 64, 48, 32, 24, 16]
        .iter()
        .map(|&s| {
            let rgba = icon_art::render_icon(s);
            IcoFrame::as_png(&rgba, s, s, image::ExtendedColorType::Rgba8).unwrap()
        })
        .collect();
    let file = std::fs::File::create(path).unwrap();
    IcoEncoder::new(file).encode_images(&frames).unwrap();
}
