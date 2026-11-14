//! Sync around a launch, with the real binary inside the stand-in for
//! Onion's runtime loop (tests/fake-runtime.sh) and brief 1's hub served
//! over localhost by this process: the pull before the game starts, a PC
//! that is gone, and the helper's push once the game has ended.

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use common::desk::{age, save, write, Desk};
use common::Rig;
use slot_mini::sync;
use slot_mini_store::Platform;

const BIN: &str = env!("CARGO_BIN_EXE_slot-mini");
const EMERALD: &str = "Pokemon - Emerald Version (USA, Europe)";

/// The helper finds RetroArch by name, anywhere on the machine: one test
/// at a time runs something called that.
static RETROARCH: Mutex<()> = Mutex::new(());

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

fn executable(p: &Path, text: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(p, text).unwrap();
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// slot-mini with Wi-Fi up and the test's PC to be found, starting on
/// Emerald; `input.<n>` scripts each start.
fn launcher(rig: &Rig, desk: &Desk, mode: &str, helper: bool) -> PathBuf {
    let tmp = rig.paths.tmp.display();
    let discover = desk.options().discover[0];
    let seam = match helper {
        true => "SLOT_MINI_TEST_HELPER=1 ",
        false => "",
    };
    let script = format!(
        r#"#!/bin/sh
n=$(cat "{tmp}/starts" 2>/dev/null || echo 0); n=$((n + 1)); echo $n > "{tmp}/starts"
input=$(cat "{tmp}/input.$n" 2>/dev/null)
SLOT_MINI_SIM_WIFI=1 SLOT_MINI_SYNC_DISCOVER={discover} {seam}exec {runner} "{BIN}" --sd "{sd}" --tmp "{tmp}" --mode {mode} --backend headless --sim --frames 1500 --input "$input"
"#,
        sd = rig.paths.sd.display(),
        runner = runner(),
    );
    let p = rig.dir.path().join(format!("{mode}.sh"));
    std::fs::write(&p, script).unwrap();
    std::fs::create_dir_all(rig.paths.data()).unwrap();
    std::fs::write(
        rig.paths.data().join("state.txt"),
        format!("cart = gba/{EMERALD}\n"),
    )
    .unwrap();
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

/// What the game would find: a copy of its save and the names in its
/// states folder, taken the moment the emulator starts.
fn witness(rig: &Rig, mine: &Path, states: &Path) -> String {
    format!(
        "cp \"{}\" \"{tmp}/seen.srm\"\nls \"{}\" > \"{tmp}/seen.states\"\n",
        mine.display(),
        states.display(),
        tmp = rig.paths.tmp.display()
    )
}

/// The card and the PC agree, then the game is played on the PC: the
/// card's save and auto state are older.
fn played_on_the_pc(rig: &Rig, desk: &Desk) -> (PathBuf, PathBuf) {
    let device = sync::layout(&rig.paths.sd);
    let mine = device.save_file(Platform::Gba, EMERALD, "srm", None);
    write(&mine, &save(1));
    age(&mine, 9000);
    let state = device.state_file(Platform::Gba, EMERALD, None);
    write(&state, b"where the Miyoo left off");
    age(&state, 9000);
    desk.pair_and_seed(rig);
    let theirs = desk.layout().save_file(Platform::Gba, EMERALD, "srm", None);
    write(&theirs, &save(2));
    (mine, state)
}

fn assert_set_aside(rig: &Rig, state: &Path) {
    let seen = read(rig.paths.tmp.join("seen.states"));
    let name = state.file_name().unwrap().to_string_lossy().into_owned();
    assert!(
        !seen.lines().any(|l| l == name),
        "the auto state was still there when the game started: {seen}"
    );
    assert!(
        seen.lines().any(|l| l.starts_with(EMERALD)),
        "it is kept beside, not deleted: {seen}"
    );
}

#[test]
fn tap_a_on_gpsp_starts_the_game_on_the_pcs_newer_save() {
    let _one = RETROARCH.lock().unwrap_or_else(|e| e.into_inner());
    let rig = Rig::new();
    let desk = Desk::new(&rig);
    // Onion 4.3.1-1's GBA script, gpSP, standing in as the emulator.
    let script = rig.paths.sd.join("Emu/GBA/launch.sh");
    executable(
        &script,
        "#!/bin/sh\n# ./retroarch -v -L .retroarch/cores/gpsp_libretro.so \"$1\"\n",
    );
    let (mine, state) = played_on_the_pc(&rig, &desk);
    assert!(mine.to_string_lossy().contains("gpSP"), "{mine:?}");
    let text = read(&script)
        + &witness(&rig, &mine, state.parent().unwrap())
        + &format!(
            "echo \"$1\" >> \"{}/played.log\"\n",
            rig.paths.tmp.display()
        );
    executable(&script, &text);
    let mainui = launcher(&rig, &desk, "launcher", false);
    input(&rig, 1, "200:press:A,260:release:A");
    run(&rig, 1, &["sh", mainui.to_str().unwrap()]);

    assert!(
        read(rig.paths.tmp.join("played.log")).contains(EMERALD),
        "{}",
        read(rig.paths.tmp.join("runtime.log"))
    );
    assert_eq!(
        std::fs::read(rig.paths.tmp.join("seen.srm")).unwrap(),
        save(2),
        "the game started on the PC's save"
    );
    assert_set_aside(&rig, &state);
    assert!(!read(rig.paths.tmp.join("runtime.log")).contains("appendconfig"));
}

#[test]
fn hold_a_on_mgba_starts_fresh_on_the_pcs_newer_save() {
    let _one = RETROARCH.lock().unwrap_or_else(|e| e.into_inner());
    let rig = Rig::new();
    let desk = Desk::new(&rig);
    let (mine, state) = played_on_the_pc(&rig, &desk);
    assert!(mine.to_string_lossy().contains("mGBA"), "{mine:?}");
    // Stable Onion: Hold A runs RetroArch itself with reset.cfg appended.
    executable(
        &rig.paths.sd.join("RetroArch/retroarch"),
        &format!(
            "#!/bin/sh\n{}echo \"$*\" >> \"{}/retroarch.log\"\n",
            witness(&rig, &mine, state.parent().unwrap()),
            rig.paths.tmp.display()
        ),
    );
    let mainui = launcher(&rig, &desk, "launcher", false);
    input(&rig, 1, "200:press:A,1000:release:A");
    run(&rig, 1, &["sh", mainui.to_str().unwrap()]);

    let ran = read(rig.paths.tmp.join("retroarch.log"));
    assert!(ran.contains("--appendconfig"), "{ran}");
    assert!(ran.contains(EMERALD), "{ran}");
    assert_eq!(
        std::fs::read(rig.paths.tmp.join("seen.srm")).unwrap(),
        save(2)
    );
    assert_set_aside(&rig, &state);
}

#[test]
fn the_dev_builds_launch_takes_the_newer_save_too() {
    let _one = RETROARCH.lock().unwrap_or_else(|e| e.into_inner());
    let rig = Rig::new();
    let desk = Desk::new(&rig);
    // Onion's development branch picks the GBA core at run time.
    let script = rig.paths.sd.join("Emu/GBA/launch.sh");
    let dev =
        "#!/bin/sh\n# ./retroarch -v -L .retroarch/cores/${default_core}_libretro.so \"$1\"\n";
    executable(&script, dev);
    let (mine, state) = played_on_the_pc(&rig, &desk);
    let text = dev.to_string()
        + &witness(&rig, &mine, state.parent().unwrap())
        + &format!(
            "echo \"$1\" >> \"{}/played.log\"\n",
            rig.paths.tmp.display()
        );
    executable(&script, &text);
    let mainui = launcher(&rig, &desk, "launcher", false);
    // Hold A: the runtime's reset flag.
    input(&rig, 1, "200:press:A,1000:release:A");
    run(&rig, 1, &["sh", mainui.to_str().unwrap()]);

    let log = read(rig.paths.tmp.join("runtime.log"));
    assert!(log.contains("appendconfig /tmp/reset.cfg"), "{log}");
    assert!(read(rig.paths.tmp.join("played.log")).contains(EMERALD));
    assert_eq!(
        std::fs::read(rig.paths.tmp.join("seen.srm")).unwrap(),
        save(2)
    );
    assert_set_aside(&rig, &state);
}

#[test]
fn a_pc_that_is_gone_does_not_hold_the_launch() {
    let _one = RETROARCH.lock().unwrap_or_else(|e| e.into_inner());
    let rig = Rig::new();
    let mut desk = Desk::new(&rig);
    let script = rig.paths.sd.join("Emu/GBA/launch.sh");
    let (mine, state) = played_on_the_pc(&rig, &desk);
    let text = format!(
        "#!/bin/sh\n{}echo \"$1\" >> \"{}/played.log\"\n",
        witness(&rig, &mine, state.parent().unwrap()),
        rig.paths.tmp.display()
    );
    executable(&script, &text);
    let mainui = launcher(&rig, &desk, "launcher", false);
    desk.unplug();
    input(&rig, 1, "200:press:A,260:release:A");
    let start = Instant::now();
    run(&rig, 1, &["sh", mainui.to_str().unwrap()]);
    let took = start.elapsed();

    assert!(read(rig.paths.tmp.join("played.log")).contains(EMERALD));
    // Starting up, the cart going in, and no more than the cap on top.
    let allowed = sync::LAUNCH_CAP + Duration::from_secs(4);
    assert!(took < allowed, "the launch took {took:?}");
    // The game starts on what the card has, untouched.
    assert_eq!(
        std::fs::read(rig.paths.tmp.join("seen.srm")).unwrap(),
        save(1)
    );
    let seen = read(rig.paths.tmp.join("seen.states"));
    let name = state.file_name().unwrap().to_string_lossy().into_owned();
    assert!(seen.lines().any(|l| l == name), "{seen}");
}

/// Stock MainUI: the user opens slot-mini from Apps once, then stays put.
fn stock_mainui(rig: &Rig, app: &Path) -> PathBuf {
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
    stock
}

/// An emulator the helper can find: a process named `retroarch` that
/// writes the game's save, runs for a second and ends.
fn playing_emulator(rig: &Rig, mine: &Path, last_words: &str) {
    let tmp = rig.paths.tmp.display();
    std::fs::copy("/bin/sleep", rig.paths.tmp.join("retroarch")).unwrap();
    write(&rig.paths.tmp.join("played.srm"), &save(7));
    executable(
        &rig.paths.sd.join("Emu/GBA/launch.sh"),
        &format!(
            "#!/bin/sh\necho \"$1\" >> \"{tmp}/played.log\"\ncp \"{tmp}/played.srm\" \"{}\"\n\"{tmp}/retroarch\" 1\n{last_words}\n",
            mine.display()
        ),
    );
}

fn wait_for(what: &str, secs: u64, mut done: impl FnMut() -> bool) {
    let start = Instant::now();
    while !done() {
        assert!(
            start.elapsed() < Duration::from_secs(secs),
            "waited too long for {what}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// The helper runs slot-mini's own binary again, which a cross-built test
/// under qemu cannot start by itself.
#[test]
#[cfg_attr(
    target_arch = "arm",
    ignore = "the helper is started without the runner"
)]
fn as_an_app_the_helper_pushes_the_save_once_the_game_has_ended() {
    let _one = RETROARCH.lock().unwrap_or_else(|e| e.into_inner());
    let rig = Rig::new();
    let desk = Desk::new(&rig);
    let (mine, _) = played_on_the_pc(&rig, &desk);
    let theirs = desk.layout().save_file(Platform::Gba, EMERALD, "srm", None);
    playing_emulator(&rig, &mine, "");
    let app = launcher(&rig, &desk, "app", true);
    let stock = stock_mainui(&rig, &app);
    input(&rig, 1, "200:press:A,260:release:A");
    run(&rig, 3, &["sh", stock.to_str().unwrap()]);

    let log = read(rig.paths.tmp.join("runtime.log"));
    assert!(log.contains("turn 3: MainUI"), "{log}");
    assert!(read(rig.paths.tmp.join("played.log")).contains(EMERALD));
    // Onion's menu is back; beside it the save reaches the PC.
    wait_for("the push", 15, || {
        std::fs::read(&theirs).is_ok_and(|b| b == save(7))
    });
    // And the helper leaves nothing behind.
    wait_for("the helper to go", 10, || {
        !rig.paths.tmp.join("slot-mini-push.pid").exists()
    });
    assert_eq!(std::fs::read(&mine).unwrap(), save(7));
}

#[test]
#[cfg_attr(
    target_arch = "arm",
    ignore = "the helper is started without the runner"
)]
fn nothing_is_pushed_while_the_miyoo_powers_off() {
    let _one = RETROARCH.lock().unwrap_or_else(|e| e.into_inner());
    let rig = Rig::new();
    let desk = Desk::new(&rig);
    let (mine, _) = played_on_the_pc(&rig, &desk);
    let theirs = desk.layout().save_file(Platform::Gba, EMERALD, "srm", None);
    // Onion's power-off leaves this flag as the game is closed.
    let off = format!("touch \"{}/.offOrder\"", rig.paths.tmp.display());
    playing_emulator(&rig, &mine, &off);
    let app = launcher(&rig, &desk, "app", true);
    let stock = stock_mainui(&rig, &app);
    input(&rig, 1, "200:press:A,260:release:A");
    run(&rig, 3, &["sh", stock.to_str().unwrap()]);

    assert!(read(rig.paths.tmp.join("played.log")).contains(EMERALD));
    // The helper notices the game has gone within its half second.
    std::thread::sleep(Duration::from_millis(2500));
    assert_eq!(std::fs::read(&theirs).unwrap(), save(2), "nothing was sent");
    assert!(!rig.paths.tmp.join("slot-mini-push.pid").exists());
    assert_eq!(std::fs::read(&mine).unwrap(), save(7));
}
