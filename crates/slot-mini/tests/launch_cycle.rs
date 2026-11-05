//! The whole launch cycle on the desktop: slot-mini runs inside a stand-in
//! for Onion's runtime loop (tests/fake-runtime.sh), launches a game, and
//! comes back onto the same cart.

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

use common::Rig;

const BIN: &str = env!("CARGO_BIN_EXE_slot-mini");

/// How to start the binary: cross-built test runs go through the same
/// runner cargo uses (qemu for armv7), which shell scripts cannot know.
fn runner() -> String {
    match cfg!(target_arch = "arm") {
        true => {
            std::env::var("CARGO_TARGET_ARMV7_UNKNOWN_LINUX_MUSLEABIHF_RUNNER").unwrap_or_default()
        }
        false => String::new(),
    }
}

fn runtime() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fake-runtime.sh")
}

/// Replace the GBA emulator with one that records what it was asked to play.
fn fake_emulator(rig: &Rig) {
    let script = format!(
        "#!/bin/sh\necho \"$1\" >> \"{}/played.log\"\n",
        rig.paths.tmp.display()
    );
    let p = rig.paths.sd.join("Emu/GBA/launch.sh");
    std::fs::write(&p, script).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// A MainUI stand-in: slot-mini as the launcher, with scripted input per
/// start (`input.1`, `input.2`, ...), a screenshot of its last frame each
/// time, and a simulated clock so runs are repeatable.
fn launcher(rig: &Rig, mode: &str) -> PathBuf {
    let tmp = rig.paths.tmp.display();
    let script = format!(
        r#"#!/bin/sh
n=$(cat "{tmp}/starts" 2>/dev/null || echo 0); n=$((n + 1)); echo $n > "{tmp}/starts"
input=$(cat "{tmp}/input.$n" 2>/dev/null)
exec {runner} "{BIN}" --sd "{sd}" --tmp "{tmp}" --mode {mode} --backend headless --sim --frames 250 --shot "{tmp}/shot.$n.png" --input "$input"
"#,
        sd = rig.paths.sd.display(),
        runner = runner(),
    );
    let p = rig.dir.path().join(format!("{mode}.sh"));
    std::fs::write(&p, script).unwrap();
    p
}

fn run(rig: &Rig, turns: u32, mainui: &[&str]) {
    let st = Command::new("sh")
        .arg(runtime())
        .arg(&rig.paths.sd)
        .arg(&rig.paths.tmp)
        .arg(turns.to_string())
        .args(mainui)
        .status()
        .unwrap();
    assert!(st.success());
}

fn read(p: impl AsRef<Path>) -> String {
    std::fs::read_to_string(p).unwrap_or_default()
}

fn input(rig: &Rig, n: u32, script: &str) {
    std::fs::write(rig.paths.tmp.join(format!("input.{n}")), script).unwrap();
}

#[test]
fn tap_a_launches_through_the_runtime_and_returns_to_the_cart() {
    let rig = Rig::new();
    fake_emulator(&rig);
    let mainui = launcher(&rig, "launcher");
    // Right twice to Golden Sun, then tap A.
    input(
        &rig,
        1,
        "200:press:RIGHT,240:release:RIGHT,500:press:RIGHT,540:release:RIGHT,1200:press:A,1260:release:A",
    );
    run(&rig, 2, &["sh", mainui.to_str().unwrap()]);

    let log = read(rig.paths.tmp.join("runtime.log"));
    let cmd = "LD_PRELOAD=/mnt/SDCARD/miyoo/app/../lib/libpadsp.so \"/mnt/SDCARD/Emu/GBA/launch.sh\" \"/mnt/SDCARD/Emu/GBA/../../Roms/GBA/Golden Sun (USA, Europe).gba\"";
    assert!(log.contains(&format!("turn 1: run {cmd}")), "{log}");
    assert!(
        !log.contains("appendconfig"),
        "tap A keeps the auto state: {log}"
    );
    assert_eq!(
        read(rig.paths.tmp.join("played.log")).trim(),
        format!(
            "{}/Emu/GBA/../../Roms/GBA/Golden Sun (USA, Europe).gba",
            rig.paths.sd.display()
        )
    );

    // Recents: the game first, in the record format Onion's Game Switcher reads.
    let recents = read(rig.paths.sd.join("Roms/recentlist.json"));
    let first: serde_json::Value = serde_json::from_str(recents.lines().next().unwrap()).unwrap();
    assert_eq!(first["label"], "Golden Sun (USA, Europe)");
    assert_eq!(first["launch"], "/mnt/SDCARD/Emu/GBA/launch.sh");
    assert_eq!(
        first["rompath"],
        "/mnt/SDCARD/Emu/GBA/../../Roms/GBA/Golden Sun (USA, Europe).gba"
    );
    assert_eq!(first["type"], 5);

    // The loop started the launcher again, and it came back on the same cart.
    assert!(log.contains("turn 2: MainUI"), "{log}");
    assert!(!rig.paths.sysdir().join("cmd_to_run.sh").exists());
    let state = read(rig.paths.data().join("state.txt"));
    assert!(
        state.contains("cart = gba/Golden Sun (USA, Europe)"),
        "{state}"
    );
    assert!(
        state.contains("launched = off"),
        "the eject plays once: {state}"
    );
    let shot = rig.paths.tmp.join("shot.2.png");
    let frame = load(&shot);
    common::golden("relaunch_same_cart", &frame);
}

#[test]
fn hold_a_starts_fresh_for_one_run() {
    // Stable Onion (4.3.1-1): Hold A writes Game List Options' "Reset game"
    // command, RetroArch with Onion's reset.cfg appended.
    let rig = Rig::new();
    let retroarch = rig.paths.sd.join("RetroArch/retroarch");
    std::fs::write(
        &retroarch,
        format!(
            "#!/bin/sh\necho \"$*\" >> \"{}/retroarch.log\"\n",
            rig.paths.tmp.display()
        ),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&retroarch, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mainui = launcher(&rig, "launcher");
    input(&rig, 1, "200:press:A,1000:release:A");
    run(&rig, 1, &["sh", mainui.to_str().unwrap()]);
    let log = read(rig.paths.tmp.join("runtime.log"));
    assert!(
        log.contains("turn 1: run LD_PRELOAD=/mnt/SDCARD/miyoo/lib/libpadsp.so ./retroarch -v --appendconfig \"/mnt/SDCARD/.tmp_update/reset.cfg\" -L \"/mnt/SDCARD/RetroArch/.retroarch/cores/mgba_libretro.so\" \"/mnt/SDCARD/Roms/GBA/2 Games in 1 (Europe).gba\""),
        "{log}"
    );
    let ran = read(rig.paths.tmp.join("retroarch.log"));
    let sd = rig.paths.sd.display();
    assert_eq!(
        ran.trim(),
        format!("-v --appendconfig {sd}/.tmp_update/reset.cfg -L {sd}/RetroArch/.retroarch/cores/mgba_libretro.so {sd}/Roms/GBA/2 Games in 1 (Europe).gba")
    );
    assert_eq!(
        read(rig.paths.sysdir().join("reset.cfg")),
        "savestate_auto_load = \"false\"\nconfig_save_on_exit = \"false\"\n"
    );
    assert!(!rig.paths.tmp.join("reset_game").exists(), "no stray flag");
}

#[test]
fn hold_a_falls_back_to_the_dev_runtime_flag() {
    // Onion's development branch picks the GBA core at run time; there the
    // runtime's /tmp/reset_game flag does the job.
    let rig = Rig::new();
    fake_emulator(&rig);
    let script = rig.paths.sd.join("Emu/GBA/launch.sh");
    let text =
        read(&script) + "# ./retroarch -v -L .retroarch/cores/${default_core}_libretro.so \"$1\"\n";
    std::fs::write(&script, text).unwrap();
    let mainui = launcher(&rig, "launcher");
    input(&rig, 1, "200:press:A,1000:release:A");
    run(&rig, 1, &["sh", mainui.to_str().unwrap()]);
    let log = read(rig.paths.tmp.join("runtime.log"));
    assert!(log.contains("turn 1: appendconfig /tmp/reset.cfg"), "{log}");
    assert!(
        !rig.paths.tmp.join("reset_game").exists(),
        "the flag is used up"
    );
    assert!(read(rig.paths.tmp.join("played.log")).contains("2 Games in 1"));
}

#[test]
fn menu_in_launcher_mode_hands_back_to_onion() {
    let rig = Rig::new();
    let mainui = launcher(&rig, "launcher");
    // MENU opens About; "Exit to Onion" is the last row (UP wraps to it).
    input(
        &rig,
        1,
        "300:press:MENU,340:release:MENU,500:press:UP,540:release:UP,900:press:A,940:release:A",
    );
    run(&rig, 1, &["sh", mainui.to_str().unwrap()]);
    let log = read(rig.paths.tmp.join("runtime.log"));
    assert!(log.contains("turn 1: MainUI exit 10"), "{log}");
    assert!(!log.contains("run "), "nothing was launched: {log}");
}

#[test]
fn as_an_app_the_game_is_queued_with_a_quick_switch() {
    let rig = Rig::new();
    fake_emulator(&rig);
    let app = launcher(&rig, "app");
    // Stock MainUI stand-in: the user opens slot-mini from Apps, which makes
    // MainUI write the app's command and exit. Second time round it does
    // nothing (the user stays in MainUI).
    let app_dir = rig.paths.app();
    std::fs::create_dir_all(&app_dir).unwrap();
    std::fs::write(
        app_dir.join("launch.sh"),
        format!("#!/bin/sh\nexec sh \"{}\"\n", app.display()),
    )
    .unwrap();
    let stock = rig.dir.path().join("stock-mainui.sh");
    std::fs::write(
        &stock,
        format!(
            "#!/bin/sh\n[ -f \"{tmp}/opened\" ] && exit 0\ntouch \"{tmp}/opened\"\necho 'cd /mnt/SDCARD/App/slot-mini; sh ./launch.sh' > \"{tmp}/cmd_to_run.sh\"\n",
            tmp = rig.paths.tmp.display()
        ),
    )
    .unwrap();
    input(&rig, 1, "300:press:A,360:release:A");
    run(&rig, 3, &["sh", stock.to_str().unwrap()]);
    let log = read(rig.paths.tmp.join("runtime.log"));
    assert!(
        log.contains("turn 1: run cd /mnt/SDCARD/App/slot-mini"),
        "{log}"
    );
    assert!(
        log.contains("turn 2: run LD_PRELOAD=/mnt/SDCARD/miyoo/app/../lib/libpadsp.so \"/mnt/SDCARD/Emu/GBA/launch.sh\""),
        "the quick switch kept the game command for the next turn: {log}"
    );
    assert!(read(rig.paths.tmp.join("played.log")).contains("2 Games in 1"));
    assert!(
        log.contains("turn 3: MainUI"),
        "after the game Onion is back in MainUI: {log}"
    );
    let state = read(rig.paths.data().join("state.txt"));
    assert!(
        state.contains("cart = gba/2 Games in 1 (Europe)"),
        "{state}"
    );
}

#[test]
fn a_name_onion_cannot_run_is_refused_on_screen() {
    let rig = Rig::new();
    std::fs::write(
        rig.paths.sd.join("Roms/GBA/0 Zelda: Minish Cap.gba"),
        [0u8; 512],
    )
    .unwrap();
    let mut d = rig.app();
    assert_eq!(d.selected(), "0 Zelda: Minish Cap");
    d.tap(slot_mini_gfx::Button::A);
    d.run(800);
    assert!(d.app.exit.is_none());
    assert_eq!(d.app.notice(), Some("Rename this file to launch it"));
    common::golden("refused_name", &d.frame);
}

fn load(p: &Path) -> slot_mini_gfx::Canvas {
    let decoder = png::Decoder::new(std::fs::File::open(p).unwrap());
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();
    let mut c = slot_mini_gfx::Canvas::new(info.width, info.height);
    for (px, p) in c.px.iter_mut().zip(buf.chunks_exact(4)) {
        *px = 0xFF00_0000 | (p[0] as u32) << 16 | (p[1] as u32) << 8 | p[2] as u32;
    }
    c
}

#[test]
fn insert_and_eject_frames() {
    let rig = Rig::new();
    let mut d = rig.app();
    d.go_to("Golden Sun (USA, Europe)");
    d.run(2500);
    d.press(slot_mini_gfx::Button::A);
    d.run(16);
    d.release(slot_mini_gfx::Button::A);
    d.run(200);
    common::golden("insert", &d.frame);
    d.run(600);
    match &d.app.exit {
        Some(slot_mini::Exit::Launch { cart, fresh }) => {
            assert_eq!(cart.stem, "Golden Sun (USA, Europe)");
            assert!(!fresh);
        }
        other => panic!("expected a launch, got {other:?}"),
    }

    // Back from the game as the boot launcher: the cart comes out of the slot.
    slot_mini::launch::hand_off(
        &rig.paths,
        slot_mini::launch::Mode::Launcher,
        "GBA",
        &d.app.current().unwrap().1.clone(),
        false,
    )
    .unwrap();
    let mut d = rig.app();
    assert!(matches!(d.app.phase, slot_mini::Phase::Ejecting { .. }));
    d.run(200);
    common::golden("eject", &d.frame);
    d.run(500);
    assert_eq!(d.app.phase, slot_mini::Phase::Shelf);
    assert_eq!(d.selected(), "Golden Sun (USA, Europe)");
}

/// The eject starts with the first frame shown, however long starting up
/// took (on the device it took longer than the whole animation).
#[test]
fn the_eject_starts_on_the_first_frame_shown() {
    let rig = Rig::new();
    let cart = slot_mini_store::scan_platform(&rig.paths.sd, slot_mini_store::Platform::Gba)
        .unwrap()
        .into_iter()
        .find(|c| c.stem == "Golden Sun (USA, Europe)")
        .unwrap();
    slot_mini::launch::hand_off(
        &rig.paths,
        slot_mini::launch::Mode::Launcher,
        "GBA",
        &cart,
        false,
    )
    .unwrap();
    let opts = slot_mini::Options {
        sound: None,
        face_wait: None,
        seed: 1,
    };
    let status = slot_mini::status::FixedStatus {
        battery: None,
        wifi: false,
        clock: (0, 0),
        charging: false,
    };
    let mut app = slot_mini::App::new(rig.paths.clone(), Box::new(status), opts, 0);
    let mut frame = slot_mini_gfx::Canvas::new(640, 480);
    // Two seconds pass before the first frame.
    app.tick(2000);
    app.render(&mut frame);
    assert!(
        matches!(app.phase, slot_mini::Phase::Ejecting { .. }),
        "still ejecting on the first frame"
    );
    app.tick(2200);
    app.render(&mut frame);
    assert!(matches!(app.phase, slot_mini::Phase::Ejecting { .. }));
    app.tick(2500);
    assert_eq!(app.phase, slot_mini::Phase::Shelf);
}
