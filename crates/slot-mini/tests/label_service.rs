//! slot's label service client against a local server: the index's ETag
//! cache, the studio's label printer, and fetching labels for carts without
//! one. Local label files always win.

mod common;
#[path = "common/server.rs"]
mod server;

use std::collections::HashMap;

use common::{golden, Rig};
use server::Server;
use slot_mini::art::jobs;
use slot_mini::status::FixedStatus;
use slot_mini_gfx::Button;

const REAL: &[u8] = include_bytes!("../../../assets/fetch_example_real.png");
const LOGO: &[u8] = include_bytes!("../../../assets/fetch_example_logo.png");

/// An index with entries for Metroid Fusion and for Emerald (which already
/// has a local label in the fixture).
fn serve(rig: &Rig) -> Server {
    let crc = |name: &str| jobs::rom_crc(&rig.paths.sd.join("Roms/GBA").join(name)).unwrap();
    let entry = r#"{"support-texture": "labels/real.png", "wheel": "logos/logo.png", "look": {"sure": true, "ground": "20304a"}}"#;
    let index = format!(
        "{{\"{:08x}\": {entry}, \"{:08x}\": {entry}}}",
        crc("Metroid Fusion (USA).gba"),
        crc("Pokemon - Emerald Version (USA, Europe).gba"),
    );
    let mut files = HashMap::new();
    files.insert("index.json".to_string(), index.into_bytes());
    files.insert("labels/real.png".to_string(), REAL.to_vec());
    files.insert("logos/logo.png".to_string(), LOGO.to_vec());
    Server::start(files)
}

fn enable(rig: &Rig, server: &Server) {
    std::fs::create_dir_all(rig.paths.data()).unwrap();
    std::fs::write(
        rig.paths.data().join("settings.txt"),
        format!("label_service = on\nart_host = {}\n", server.base),
    )
    .unwrap();
}

fn online() -> FixedStatus {
    FixedStatus {
        battery: Some(76),
        wifi: true,
        clock: (12, 0),
    }
}

#[test]
fn the_index_is_cached_with_its_etag() {
    let rig = Rig::new();
    let server = serve(&rig);
    let base = jobs::base_url(&server.base);
    let cart = slot_mini_store::scan_platform(&rig.paths.sd, slot_mini_store::Platform::Gba)
        .unwrap()
        .into_iter()
        .find(|c| c.stem == "Metroid Fusion (USA)")
        .unwrap();
    let prints = jobs::studio_prints(&rig.paths.sd, &base, &cart).unwrap();
    assert!(prints.real.is_some() && prints.logo.is_some());
    let again = jobs::studio_prints(&rig.paths.sd, &base, &cart).unwrap();
    assert!(again.real.is_some());
    let asked = server.asked();
    assert_eq!(
        asked.iter().filter(|a| a.starts_with("index.json")).count(),
        2
    );
    assert!(
        asked.contains(&"index.json (if-none-match)".to_string()),
        "{asked:?}"
    );
    // WebP is tried first, as slot does, then the PNG the index names.
    assert!(asked.contains(&"labels/real.webp".to_string()));
    assert!(rig
        .paths
        .sd
        .join("App/slot-mini/cache/art/index.json")
        .is_file());
}

#[test]
fn missing_labels_are_fetched_and_local_files_win() {
    let rig = Rig::new();
    let server = serve(&rig);
    enable(&rig, &server);
    let emerald = rig
        .paths
        .sd
        .join("Labels/GBA/Pokemon - Emerald Version (USA, Europe).png");
    let before = std::fs::read(&emerald).unwrap();
    let mut d = rig.app_with(online());
    let metroid = rig.paths.sd.join("Labels/GBA/Metroid Fusion (USA).png");
    for _ in 0..200 {
        if metroid.exists() {
            break;
        }
        d.run(50);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        metroid.is_file(),
        "the service's label was written for the cart without one"
    );
    assert_eq!(
        std::fs::read(&emerald).unwrap(),
        before,
        "a local label is never replaced"
    );
    assert!(
        !server.asked().iter().any(|a| a.contains("Emerald")),
        "carts with a local label are not looked up"
    );
    d.run(200);
    d.go_to("Metroid Fusion (USA)");
    d.run(1500);
    assert!(d.app.current().unwrap().1.label.is_some());
    golden("shelf_fetched_label", &d.frame);
}

#[test]
fn with_the_service_off_nothing_is_fetched() {
    let rig = Rig::new();
    let server = serve(&rig);
    let mut d = rig.app_with(online());
    d.run(500);
    assert!(server.asked().is_empty());
    assert!(!rig
        .paths
        .sd
        .join("Labels/GBA/Metroid Fusion (USA).png")
        .exists());
}

#[test]
fn the_label_printer_prints_and_applies() {
    let rig = Rig::new();
    let server = serve(&rig);
    enable(&rig, &server);
    // Keep the background fetch from labelling Metroid first.
    std::fs::write(
        rig.paths.data().join("settings.txt"),
        format!("label_service = on\nart_host = {}\n", server.base),
    )
    .unwrap();
    let mut d = rig.app_with(FixedStatus {
        wifi: false,
        ..online()
    });
    d.go_to("Metroid Fusion (USA)");
    d.tap(Button::Select);
    d.tap(Button::A);
    assert!(matches!(
        d.app.studio.as_ref().unwrap().printer,
        slot_mini::studio_ui::Printer::NoWifi
    ));
    d.run(50);
    golden("studio_printer_no_wifi", &d.frame);
    d.tap(Button::B);
    d.tap(Button::B);

    let mut d = rig.app_with(online());
    // The background fetch may land Metroid's label; wait for it so the
    // printer offers both prints either way.
    d.run(300);
    d.go_to("Metroid Fusion (USA)");
    d.tap(Button::Select);
    d.tap(Button::A);
    for _ in 0..200 {
        if matches!(
            d.app.studio.as_ref().unwrap().printer,
            slot_mini::studio_ui::Printer::Ready
        ) {
            break;
        }
        d.run(20);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    d.run(50);
    golden("studio_printer", &d.frame);
    // Pick the logo label and apply it.
    d.tap(Button::Down);
    d.tap(Button::A);
    d.tap(Button::B);
    d.tap(Button::B);
    let label = rig.paths.sd.join("Labels/GBA/Metroid Fusion (USA).png");
    let (_, w, h) = slot_mini_ui::decode_png(&label).unwrap();
    assert_eq!((w, h), (1280, 661), "a label at Cart Studio's GBA size");
}
