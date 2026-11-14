//! Gives the Windows exe its icon, its version details (what Explorer's
//! Properties shows) and its manifest. Nothing happens for other targets.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-changed=assets/app.manifest");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon("assets/icon.ico")
        .set_manifest_file("assets/app.manifest")
        .set("ProductName", "slot-mini")
        .set("FileDescription", "slot-mini")
        .set("OriginalFilename", "slot-mini-pc.exe")
        .set(
            "LegalCopyright",
            "GPL-3.0-or-later. slot by Brandon Kowalski.",
        );
    if let Err(e) = res.compile() {
        // Releases are built on Windows, where this must work. A cross
        // build from elsewhere (a check on Linux) may lack the tools.
        if cfg!(windows) {
            panic!("slot-mini-pc: resources: {e}");
        }
        println!("cargo:warning=slot-mini-pc: no icon or version resource: {e}");
    }
}
