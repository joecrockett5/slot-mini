//! slot's in-game MENU helper: gestures, the menu over a frozen game, and
//! what each choice does to RetroArch. Runs against a fake game and screen;
//! the RetroArch control itself is tested against a fake RetroArch.

mod common;

use std::sync::{Arc, Mutex};

use slot_mini::ingame::{Game, Overlay, Screen};
use slot_mini_gfx::{Button, Canvas, Event};

#[derive(Clone, Default)]
struct Log(Arc<Mutex<Vec<String>>>);

impl Log {
    fn push(&self, s: impl Into<String>) {
        self.0.lock().unwrap().push(s.into());
    }
    fn take(&self) -> Vec<String> {
        std::mem::take(&mut *self.0.lock().unwrap())
    }
}

struct FakeGame(Log);

impl Game for FakeGame {
    fn running(&mut self) -> bool {
        true
    }
    fn pause(&mut self) {
        self.0.push("pause");
    }
    fn resume(&mut self) {
        self.0.push("resume");
    }
    fn send(&mut self, c: &str) {
        self.0.push(c);
    }
    fn quit(&mut self) {
        self.0.push("quit");
    }
}

/// Three framebuffer pages (RetroArch can use more than two); the driver
/// says page 1 is shown.
struct FakeScreen {
    pages: [Canvas; 3],
}

impl Screen for FakeScreen {
    fn pages(&mut self) -> u32 {
        3
    }
    fn displayed(&mut self) -> u32 {
        1
    }
    fn capture(&mut self, page: u32) -> Canvas {
        self.pages[page as usize].clone()
    }
    fn draw_on(&mut self, page: u32, frame: &Canvas) {
        self.pages[page as usize] = frame.clone();
    }
}

fn game_frame() -> Canvas {
    let decoder = png::Decoder::new(
        std::fs::File::open(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/game-frame.png"
        ))
        .unwrap(),
    );
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();
    let mut c = Canvas::new(info.width, info.height);
    let step = info.color_type.samples();
    for (px, p) in c.px.iter_mut().zip(buf.chunks_exact(step)) {
        *px = 0xFF00_0000 | (p[0] as u32) << 16 | (p[1] as u32) << 8 | p[2] as u32;
    }
    c
}

fn overlay(log: &Log, sysdir: &std::path::Path) -> Overlay<FakeGame, FakeScreen> {
    let screen = FakeScreen {
        pages: [Canvas::new(640, 480), game_frame(), Canvas::new(640, 480)],
    };
    Overlay::new(FakeGame(log.clone()), screen, sysdir.to_path_buf())
}

fn tap(o: &mut Overlay<FakeGame, FakeScreen>, b: Button, at: u64) {
    o.handle(Event::Press(b), at);
    o.handle(Event::Release(b), at + 40);
}

#[test]
fn tapping_menu_freezes_the_game_under_slots_menu_and_b_resumes() {
    let log = Log::default();
    let dir = tempfile::tempdir().unwrap();
    let mut o = overlay(&log, dir.path());
    tap(&mut o, Button::Menu, 0);
    assert!(o.is_open());
    assert_eq!(log.take(), vec!["pause"]);
    common::golden("ingame_menu", &o.screen.pages[1]);
    tap(&mut o, Button::Down, 100);
    tap(&mut o, Button::Down, 200);
    common::golden("ingame_menu_load", &o.screen.pages[1]);
    tap(&mut o, Button::B, 300);
    assert!(!o.is_open());
    assert_eq!(log.take(), vec!["resume"]);
    assert_eq!(
        o.screen.pages[1].px,
        game_frame().px,
        "the game's own picture is put back"
    );
    assert_eq!(
        o.screen.pages[0].px,
        Canvas::new(640, 480).px,
        "the hidden page is untouched"
    );
}

#[test]
fn each_row_does_what_it_says() {
    let dir = tempfile::tempdir().unwrap();
    let cases: [(usize, &[&str]); 5] = [
        (0, &["resume"]),
        (1, &["resume", "SAVE_STATE"]),
        (2, &["resume", "LOAD_STATE"]),
        (3, &["resume", "MENU_TOGGLE"]),
        (5, &["quit"]),
    ];
    for (row, want) in cases {
        let log = Log::default();
        let mut o = overlay(&log, dir.path());
        tap(&mut o, Button::Menu, 0);
        log.take();
        for i in 0..row {
            tap(&mut o, Button::Down, 100 + i as u64 * 100);
        }
        tap(&mut o, Button::A, 1000);
        assert_eq!(log.take(), want, "row {row}");
        assert!(!o.is_open());
    }
    // Game Switcher: Onion's flag for the runtime loop, then quit.
    let log = Log::default();
    let mut o = overlay(&log, dir.path());
    tap(&mut o, Button::Menu, 0);
    log.take();
    for i in 0..4 {
        tap(&mut o, Button::Down, 100 + i * 100);
    }
    tap(&mut o, Button::A, 1000);
    assert_eq!(log.take(), vec!["quit"]);
    assert!(dir.path().join(".runGameSwitcher").is_file());
}

#[test]
fn holding_menu_ejects_without_opening_the_menu() {
    let log = Log::default();
    let dir = tempfile::tempdir().unwrap();
    let mut o = overlay(&log, dir.path());
    o.handle(Event::Press(Button::Menu), 0);
    o.tick(500);
    o.tick(900);
    assert!(log.take().is_empty(), "decided on release, not while held");
    o.handle(Event::Release(Button::Menu), 900);
    assert_eq!(log.take(), vec!["quit"]);
    assert!(!o.is_open());
    o.handle(Event::Press(Button::Menu), 1000);
    o.handle(Event::Release(Button::Menu), 1100);
    assert!(log.take().is_empty(), "nothing more once quitting");
}

/// RetroArch's and Onion's MENU hotkeys: MENU held (even for a while) with
/// another button, in either order, neither opens the menu nor ejects.
#[test]
fn menu_hotkeys_never_open_the_menu_or_eject() {
    let log = Log::default();
    let dir = tempfile::tempdir().unwrap();
    let mut o = overlay(&log, dir.path());
    // MENU held a second and a half, then R2 (save state).
    o.handle(Event::Press(Button::Menu), 0);
    o.tick(1000);
    o.handle(Event::Press(Button::R2), 1500);
    o.handle(Event::Release(Button::R2), 1600);
    o.handle(Event::Release(Button::Menu), 1700);
    // L1 a moment before MENU (pressed "together").
    o.handle(Event::Press(Button::L1), 2000);
    o.handle(Event::Press(Button::Menu), 2010);
    o.handle(Event::Release(Button::L1), 2100);
    o.handle(Event::Release(Button::Menu), 2110);
    // MENU + POWER (screenshot).
    o.handle(Event::Press(Button::Menu), 3000);
    o.handle(Event::Press(Button::Power), 3050);
    o.handle(Event::Release(Button::Power), 3100);
    o.handle(Event::Release(Button::Menu), 3150);
    assert!(!o.is_open());
    assert!(log.take().is_empty(), "the game was left alone");
    // A plain tap still opens the menu.
    o.handle(Event::Press(Button::Menu), 4000);
    o.handle(Event::Release(Button::Menu), 4100);
    assert!(o.is_open());
}

#[test]
fn menu_shortcuts_with_other_buttons_are_left_alone() {
    let log = Log::default();
    let dir = tempfile::tempdir().unwrap();
    let mut o = overlay(&log, dir.path());
    o.handle(Event::Press(Button::Menu), 0);
    o.handle(Event::Press(Button::Up), 50);
    o.handle(Event::Release(Button::Up), 100);
    o.tick(1000);
    o.handle(Event::Release(Button::Menu), 1100);
    assert!(!o.is_open());
    assert!(log.take().is_empty());
}

/// A stand-in RetroArch: a real process to freeze and thaw, and a UDP
/// listener on RetroArch's command port answering GET_STATUS.
#[cfg(target_os = "linux")]
/// RetroArch's command port is fixed (55355): the tests that listen on it
/// or send to it take turns, or one hears the other's commands.
static RETROARCH_PORT: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn port_turn() -> std::sync::MutexGuard<'static, ()> {
    RETROARCH_PORT.lock().unwrap_or_else(|e| e.into_inner())
}

#[test]
fn retroarch_is_paused_frozen_and_thawed() {
    let _turn = port_turn();
    use slot_mini::ingame::RetroArch;
    let Ok(sock) = std::net::UdpSocket::bind("127.0.0.1:55355") else {
        eprintln!("port 55355 busy; skipping");
        return;
    };
    sock.set_read_timeout(Some(std::time::Duration::from_millis(50)))
        .unwrap();
    let heard = Log::default();
    let h = heard.clone();
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let s2 = stop.clone();
    let server = std::thread::spawn(move || {
        let mut paused = false;
        let mut buf = [0u8; 256];
        while !s2.load(std::sync::atomic::Ordering::SeqCst) {
            let Ok((n, from)) = sock.recv_from(&mut buf) else {
                continue;
            };
            let cmd = String::from_utf8_lossy(&buf[..n]).to_string();
            match cmd.as_str() {
                "GET_STATUS" => {
                    let state = if paused { "PAUSED" } else { "PLAYING" };
                    let _ = sock.send_to(
                        format!("GET_STATUS {state} mgba,Game,crc32=0").as_bytes(),
                        from,
                    );
                }
                "PAUSE_TOGGLE" => {
                    paused = !paused;
                    h.push(cmd);
                }
                _ => h.push(cmd),
            }
        }
    });
    let mut child = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .unwrap();
    let pid = child.id() as i32;
    let state = || {
        std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .unwrap()
            .split_whitespace()
            .nth(2)
            .unwrap()
            .to_string()
    };
    let mut ra = RetroArch::attach(pid);
    assert!(ra.running());
    ra.pause();
    std::thread::sleep(std::time::Duration::from_millis(50));
    assert_eq!(state(), "T", "frozen so it cannot draw over the menu");
    ra.resume();
    std::thread::sleep(std::time::Duration::from_millis(50));
    assert_ne!(state(), "T");
    ra.send("SAVE_STATE");
    ra.quit();
    let status = child.wait().unwrap();
    assert!(!status.success(), "SIGTERM ended it");
    std::thread::sleep(std::time::Duration::from_millis(100));
    stop.store(true, std::sync::atomic::Ordering::SeqCst);
    server.join().unwrap();
    assert_eq!(
        heard.take(),
        vec!["PAUSE_TOGGLE", "PAUSE_TOGGLE", "SAVE_STATE"]
    );
}

/// The real helper process end to end: `slot-mini --ingame` finds a process
/// named retroarch, takes MENU from Onion with its flag, draws the menu into
/// a file standing in for /dev/fb0 when MENU is tapped, freezes the game,
/// and cleans up (flag gone, game thawed) when the game ends.
#[cfg(target_os = "linux")]
#[test]
fn the_helper_process_end_to_end() {
    use std::time::{Duration, Instant};
    if cfg!(target_arch = "arm") {
        // Under qemu-user, /proc shows qemu, not the emulated programs.
        return;
    }
    // The helper sends to RetroArch's port.
    let _turn = port_turn();
    let rig = common::Rig::new();
    let dir = rig.dir.path();
    // A "retroarch": sleep, renamed, so /proc/<pid>/comm says retroarch.
    let ra = dir.join("retroarch");
    std::fs::copy("/bin/sleep", &ra).unwrap();
    let fb = dir.join("fb0");
    std::fs::write(&fb, vec![0u8; 640 * 960 * 4]).unwrap();
    // MENU pressed and released, as the device writes events.
    let ev = dir.join("event0");
    let size = std::mem::size_of::<libc::input_event>();
    let mut bytes = Vec::new();
    for v in [1i32, 0] {
        let mut r = vec![0u8; size - 8];
        r.extend_from_slice(&1u16.to_le_bytes());
        r.extend_from_slice(&1u16.to_le_bytes()); // KEY_ESC: MENU
        r.extend_from_slice(&v.to_le_bytes());
        bytes.extend(r);
    }
    std::fs::write(&ev, bytes).unwrap();

    let mut game = std::process::Command::new(&ra).arg("30").spawn().unwrap();
    let pid = game.id();
    let mut helper = std::process::Command::new(env!("CARGO_BIN_EXE_slot-mini"))
        .args([
            "--ingame",
            "--sd",
            rig.paths.sd.to_str().unwrap(),
            "--tmp",
            rig.paths.tmp.to_str().unwrap(),
        ])
        .env("SLOT_MINI_FB", &fb)
        .env("SLOT_MINI_INPUT", &ev)
        .spawn()
        .unwrap();
    let flag = rig.paths.tmp.join("disable_menu_button");
    let state = || {
        std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .map(|s| s.split_whitespace().nth(2).unwrap_or("").to_string())
            .unwrap_or_default()
    };
    let start = Instant::now();
    while state() != "T" && start.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(state(), "T", "the tap froze the game");
    assert_eq!(
        std::fs::read_to_string(&flag).unwrap().trim(),
        helper.id().to_string(),
        "Onion's keymon is told to leave MENU alone, by this helper"
    );
    // The menu is on the page the game shows (page 0 here): its scrim over
    // black is still black, but the lit RESUME row is bright.
    let page = std::fs::read(&fb).unwrap();
    let bright = page[..640 * 480 * 4]
        .chunks_exact(4)
        .filter(|p| p[0] > 200 && p[1] > 200 && p[2] > 200)
        .count();
    assert!(
        bright > 500,
        "menu text drawn into the framebuffer ({bright} bright pixels)"
    );

    // The game ends (killed while frozen): the helper cleans up and exits.
    game.kill().unwrap();
    game.wait().unwrap();
    let done = helper.wait().unwrap();
    assert!(done.success());
    assert!(!flag.exists(), "MENU is Onion's again");
}
