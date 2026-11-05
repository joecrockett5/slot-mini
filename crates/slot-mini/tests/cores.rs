//! slot's core picker on a GBA cart: START opens the cart, left and right
//! move the chip between mGBA and gpSP, A keeps it in Onion's per-game core
//! file and carries the game's save to the new core, B leaves everything as
//! it was.

mod common;

use common::{golden, Rig};
use slot_mini::cores;
use slot_mini_gfx::Button;
use slot_mini_store::Core;

const GAME: &str = "Golden Sun (USA, Europe)";

fn cfg(rig: &Rig) -> std::path::PathBuf {
    rig.paths
        .sd
        .join("Roms/GBA/.game_config")
        .join(format!("{GAME}.cfg"))
}

#[test]
fn start_opens_the_cart_and_a_keeps_gpsp_in_onions_file() {
    let rig = Rig::new();
    let mut d = rig.app();
    d.go_to(GAME);
    d.tap(Button::Start);
    let scene = d.app.core.as_ref().expect("the picker opens");
    assert_eq!(scene.picker.seat(), Core::Mgba, "mGBA by default");
    // Faces are built, then the cart opens (slot's 420 ms).
    d.run(800);
    assert!(d.app.core.as_ref().unwrap().shown());
    golden("core_picker", &d.frame);

    // Other buttons do nothing while it is open.
    for b in [Button::L1, Button::Y, Button::Select, Button::Start] {
        d.tap(b);
    }
    assert!(d.app.core.is_some());
    assert_eq!(d.selected(), GAME);

    // Right: the chip hops to gpSP.
    d.tap(Button::Right);
    d.run(48);
    golden("core_picker_hop", &d.frame);
    d.run(300);
    assert_eq!(d.app.core.as_ref().unwrap().picker.seat(), Core::Gpsp);

    // A keeps it and the cart closes.
    d.tap(Button::A);
    d.run(400);
    assert!(d.app.core.is_none());
    assert_eq!(
        std::fs::read_to_string(cfg(&rig)).unwrap(),
        "core = \"gpsp_libretro\"\n"
    );
    assert_eq!(d.app.notice(), Some("Now runs on gpSP"), "no save to carry");
    // Hold A's fresh start runs gpSP too.
    let cart = d.app.current().unwrap().1.clone();
    assert_eq!(
        slot_mini::launch::core_for(&rig.paths, &cart).as_deref(),
        Some("gpsp_libretro")
    );

    // Back to mGBA, the default: Onion's file goes, as "Restore default
    // core" removes it.
    d.tap(Button::Start);
    d.run(800);
    assert_eq!(d.app.core.as_ref().unwrap().picker.seat(), Core::Gpsp);
    d.tap(Button::Left);
    d.run(300);
    d.tap(Button::A);
    d.run(400);
    assert!(!cfg(&rig).exists());
    assert!(!cfg(&rig).parent().unwrap().exists());
    assert_eq!(cores::chosen(&rig.paths, &cart), Core::Mgba);
}

/// Switch with the picker: START, a direction, A.
fn switch(d: &mut common::Driver, to: Button) {
    d.tap(Button::Start);
    d.run(800);
    d.tap(to);
    d.run(300);
    d.tap(Button::A);
    d.run(400);
    assert!(d.app.core.is_none());
}

#[test]
fn the_save_goes_with_the_game_both_ways() {
    let rig = Rig::new();
    let mut d = rig.app();
    d.go_to(GAME);
    let cart = d.app.current().unwrap().1.clone();
    let save = |c| cores::save_path(&rig.paths, &cart, c).unwrap();
    let state = |c| cores::auto_state_path(&rig.paths, &cart, c).unwrap();
    let put = |p: &std::path::Path, b: &[u8]| {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, b).unwrap();
    };
    // Played on mGBA; an old gpSP session lies around.
    put(&save(Core::Mgba), b"mgba progress");
    put(&state(Core::Gpsp), b"old gpsp session");
    switch(&mut d, Button::Right);
    assert_eq!(d.app.notice(), Some("Save carried over to gpSP"));
    assert_eq!(std::fs::read(save(Core::Gpsp)).unwrap(), b"mgba progress");
    assert!(!state(Core::Gpsp).exists(), "the old session is set aside");

    // Played on gpSP, then back to mGBA: the newer save comes back and the
    // older mGBA one is kept as a backup.
    put(&save(Core::Gpsp), b"gpsp progress");
    put(&state(Core::Mgba), b"mgba session");
    switch(&mut d, Button::Left);
    assert_eq!(d.app.notice(), Some("Save carried over to mGBA"));
    assert_eq!(std::fs::read(save(Core::Mgba)).unwrap(), b"gpsp progress");
    assert!(!state(Core::Mgba).exists());
    let kept: Vec<_> = std::fs::read_dir(save(Core::Mgba).parent().unwrap())
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().contains(".srm.bak_"))
        .collect();
    assert_eq!(kept.len(), 1);
    assert_eq!(std::fs::read(kept[0].path()).unwrap(), b"mgba progress");
    assert!(!cfg(&rig).exists(), "back on the default core");
}

#[test]
fn b_leaves_the_core_as_it_was() {
    let rig = Rig::new();
    let mut d = rig.app();
    d.go_to(GAME);
    d.tap(Button::Start);
    d.run(800);
    d.tap(Button::Right);
    d.run(300);
    d.tap(Button::B);
    d.run(400);
    assert!(d.app.core.is_none());
    assert!(!cfg(&rig).exists());
    // The shelf works again.
    d.tap(Button::Right);
    assert_ne!(d.selected(), GAME);
}

#[test]
fn game_boy_carts_and_missing_cores_do_not_open_it() {
    let rig = Rig::new();
    let mut d = rig.app();
    // GBA, GBC, GB: L1 wraps round to Game Boy.
    d.tap(Button::L1);
    d.go_to("Tetris (World)");
    d.tap(Button::Start);
    assert!(d.app.core.is_none());

    let rig = Rig::new();
    std::fs::remove_file(
        rig.paths
            .sd
            .join("RetroArch/.retroarch/cores/gpsp_libretro.so"),
    )
    .unwrap();
    let mut d = rig.app();
    d.go_to(GAME);
    d.tap(Button::Start);
    assert!(d.app.core.is_none());
    assert_eq!(d.app.notice(), Some("gpSP is not installed"));
}
