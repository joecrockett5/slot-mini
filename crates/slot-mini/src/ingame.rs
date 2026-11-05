//! slot's in-game MENU, for games slot-mini launches.
//!
//! slot-mini leaves this helper running beside RetroArch (`slot-mini
//! --ingame`). It sets Onion's own `/tmp/disable_menu_button` flag so Onion's
//! key monitor leaves MENU alone (its MENU combinations keep working: keymon
//! tracks those separately), then:
//!
//! - tap MENU: the game freezes behind slot's in-game menu (Resume, Save
//!   State, Load State, Settings, Game Switcher, Exit Game), drawn straight
//!   onto the framebuffer page the game is showing;
//! - hold MENU: eject. RetroArch is asked to quit with SIGTERM, which writes
//!   its auto state on the way out, exactly as Onion's "exit to menu" does.
//!
//! The helper is already loaded and asleep on the button device, so the menu
//! appears within a frame or two. It exits, clearing the flag, when RetroArch
//! does. See docs/DESIGN-PARITY.md and docs/PLATFORM.md.

use std::path::{Path, PathBuf};

use slot_mini_gfx::{Button, Canvas, Draw, Event, Renderer, Sprite, Stage, TexId};
use slot_mini_ui::quick::{
    quick_pointer_face, quick_text_face, DIM_INK, GROUND_SCRIM, LEGEND_Y, MENU_INK, MENU_PAD,
    POINTER_BLEED, POINTER_GAP, QUICK_PITCH, TYPE_DROP,
};
use slot_mini_ui::{hint_face, Millis, UndoFace, FOOTER_Y};

use crate::paths::Paths;

/// Holding MENU this long ejects (keymon's in-game long press is 700 ms).
pub const HOLD_MS: Millis = 700;
/// Onion's flag that makes keymon ignore MENU (`keymon.c`).
pub const FLAG: &str = "disable_menu_button";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Gesture {
    Tap,
    Hold,
}

/// Tells a tap of MENU from a hold, and leaves MENU alone whenever it is
/// part of a combination (Onion's and RetroArch's MENU hotkeys: MENU + R2
/// to save, MENU + L to rewind, ...): any other button down when MENU goes
/// down, or pressed before it comes up, makes it a combination, whichever
/// came first. Both gestures are decided when MENU is released, so holding
/// MENU before pressing the rest of a hotkey never ejects the game.
#[derive(Default, Debug)]
pub struct MenuButton {
    down: Option<Millis>,
    combo: bool,
    /// Other buttons down now.
    others: Vec<Button>,
}

impl MenuButton {
    pub fn feed(&mut self, ev: Event, now: Millis) -> Option<Gesture> {
        match ev {
            Event::Press(Button::Menu) => {
                self.down = Some(now);
                self.combo = !self.others.is_empty();
                None
            }
            Event::Press(b) => {
                if !self.others.contains(&b) {
                    self.others.push(b);
                }
                if self.down.is_some() {
                    self.combo = true;
                }
                None
            }
            Event::Release(Button::Menu) => {
                let gesture = match self.down.take() {
                    Some(_) if self.combo => None,
                    Some(d) if now.saturating_sub(d) >= HOLD_MS => Some(Gesture::Hold),
                    Some(_) => Some(Gesture::Tap),
                    None => None,
                };
                self.combo = false;
                gesture
            }
            Event::Release(b) => {
                self.others.retain(|o| *o != b);
                None
            }
            Event::Quit => None,
        }
    }

    /// Which other buttons are down, from the device itself where it can
    /// say (so a release that went missing cannot make every MENU a
    /// combination).
    pub fn set_others(&mut self, down: Vec<Button>) {
        self.others = down.into_iter().filter(|b| *b != Button::Menu).collect();
    }

    pub fn held(&self) -> bool {
        self.down.is_some()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Row {
    Resume,
    SaveState,
    LoadState,
    Settings,
    GameSwitcher,
    ExitGame,
}

impl Row {
    pub const ALL: [Row; 6] = [
        Row::Resume,
        Row::SaveState,
        Row::LoadState,
        Row::Settings,
        Row::GameSwitcher,
        Row::ExitGame,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Row::Resume => "Resume",
            Row::SaveState => "Save State",
            Row::LoadState => "Load State",
            Row::Settings => "Settings",
            Row::GameSwitcher => "Game Switcher",
            Row::ExitGame => "Exit Game",
        }
    }
}

/// What the player chose.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Resume,
    Save,
    Load,
    /// RetroArch's own menu.
    Settings,
    /// Onion's Game Switcher.
    Switcher,
    Exit,
}

#[derive(Default, Debug)]
pub struct Menu {
    pub row: usize,
}

impl Menu {
    pub fn press(&mut self, b: Button) -> Option<Action> {
        let n = Row::ALL.len();
        match b {
            Button::Up => self.row = (self.row + n - 1) % n,
            Button::Down => self.row = (self.row + 1) % n,
            Button::B | Button::Menu => return Some(Action::Resume),
            Button::A => {
                return Some(match Row::ALL[self.row] {
                    Row::Resume => Action::Resume,
                    Row::SaveState => Action::Save,
                    Row::LoadState => Action::Load,
                    Row::Settings => Action::Settings,
                    Row::GameSwitcher => Action::Switcher,
                    Row::ExitGame => Action::Exit,
                })
            }
            _ => {}
        }
        None
    }
}

/// The running game.
pub trait Game {
    fn running(&mut self) -> bool;
    /// Freeze the game (and its picture) for the menu.
    fn pause(&mut self);
    /// Undo `pause`.
    fn resume(&mut self);
    /// A RetroArch network command (`SAVE_STATE`, `LOAD_STATE`, `MENU_TOGGLE`).
    fn send(&mut self, command: &str);
    /// Ask RetroArch to quit; it writes its auto state first.
    fn quit(&mut self);
}

/// Where the menu is drawn.
pub trait Screen {
    /// How many screen-sized pages the framebuffer memory holds, read fresh
    /// (RetroArch may use more than two and set them up after we start).
    fn pages(&mut self) -> u32;
    /// The framebuffer page on screen now.
    fn displayed(&mut self) -> u32;
    fn capture(&mut self, page: u32) -> Canvas;
    fn draw_on(&mut self, page: u32, frame: &Canvas);
}

#[cfg(target_os = "linux")]
impl Screen for slot_mini_gfx::fbdev::Fbdev {
    fn pages(&mut self) -> u32 {
        self.refresh();
        self.mapped_pages()
    }
    fn displayed(&mut self) -> u32 {
        self.displayed_page()
    }
    fn capture(&mut self, page: u32) -> Canvas {
        slot_mini_gfx::fbdev::Fbdev::capture(self, page)
    }
    fn draw_on(&mut self, page: u32, frame: &Canvas) {
        slot_mini_gfx::fbdev::Fbdev::draw_on(self, page, frame)
    }
}

type Face = (TexId, u32, u32);

/// The menu's text, rasterised once when the helper starts so opening the
/// menu costs one frame.
pub struct Faces {
    gfx: Renderer,
    rows: Vec<[Face; 2]>,
    pointer: Face,
    legends: [Face; 2],
    ground: Option<TexId>,
}

fn up(gfx: &mut Renderer, f: UndoFace) -> Face {
    (gfx.upload_rgba(f.w, f.h, &f.rgba), f.w, f.h)
}

impl Faces {
    pub fn new() -> Faces {
        let mut gfx = Renderer::new();
        let rows = Row::ALL
            .iter()
            .map(|r| {
                [
                    up(&mut gfx, quick_text_face(r.label(), DIM_INK)),
                    up(&mut gfx, quick_text_face(r.label(), MENU_INK)),
                ]
            })
            .collect();
        let pointer = up(&mut gfx, quick_pointer_face());
        let legends = [
            up(&mut gfx, hint_face("B", "BACK")),
            up(&mut gfx, hint_face("A", "SELECT")),
        ];
        Faces {
            gfx,
            rows,
            pointer,
            legends,
            ground: None,
        }
    }

    /// The paused game under slot's menu scrim, the rows centred as slot's
    /// in-game page draws them, the pointer beside the selected row.
    pub fn compose(&mut self, game: &Canvas, menu: &Menu) -> Canvas {
        let stage = Stage::MINI;
        let ground = Sprite {
            w: game.w,
            h: game.h,
            px: game.px.clone(),
        };
        let ground = match self.ground {
            Some(t) => {
                self.gfx.replace(t, ground);
                t
            }
            None => {
                let t = self.gfx.upload(ground);
                self.ground = Some(t);
                t
            }
        };
        let mut out = vec![
            Draw::Tex {
                x: 0.0,
                y: 0.0,
                w: stage.w,
                h: 480.0,
                tex: ground,
                alpha: 1.0,
            },
            Draw::Rect {
                x: 0.0,
                y: 0.0,
                w: stage.w,
                h: 480.0,
                colour: [0.0, 0.0, 0.0, GROUND_SCRIM],
            },
        ];
        let pad = MENU_PAD as f32;
        let main_w = self.rows.iter().map(|r| r[1].1).max().unwrap_or(0);
        let x = stage.centred(main_w as f32);
        let top = (LEGEND_Y - QUICK_PITCH * Row::ALL.len() as f32) / 2.0;
        let face = |(tex, w, h): Face, x: f32, y: f32| Draw::Tex {
            x: x.round(),
            y: y.round(),
            w: w as f32,
            h: h as f32,
            tex,
            alpha: 1.0,
        };
        for (i, faces) in self.rows.iter().enumerate() {
            let lit = i == menu.row;
            let y = top + QUICK_PITCH * i as f32 + TYPE_DROP;
            out.push(face(faces[lit as usize], x, y));
            if lit {
                let (_, pw, _) = self.pointer;
                out.push(face(
                    self.pointer,
                    x + pad - POINTER_GAP - pw as f32,
                    y - POINTER_BLEED as f32,
                ));
            }
        }
        let [back, select] = self.legends;
        out.push(face(back, 24.0, FOOTER_Y));
        out.push(face(select, stage.w - 24.0 - select.1 as f32, FOOTER_Y));
        let mut canvas = Canvas::new(game.w, game.h);
        self.gfx.render(&out, &mut canvas);
        canvas
    }
}

impl Default for Faces {
    fn default() -> Self {
        Faces::new()
    }
}

/// The menu on screen: what every page held before, which one the game was
/// showing, and a choice waiting for its button to be let go.
struct Open {
    menu: Menu,
    shown: usize,
    saved: Vec<Canvas>,
    held: Vec<Button>,
    chosen: Option<Action>,
}

/// The helper's state: MENU gestures, the menu, and what they do to the game.
pub struct Overlay<G: Game, S: Screen> {
    pub game: G,
    pub screen: S,
    faces: Faces,
    button: MenuButton,
    open: Option<Open>,
    sysdir: PathBuf,
    /// Set once the game has been asked to quit.
    pub quitting: bool,
}

impl<G: Game, S: Screen> Overlay<G, S> {
    pub fn new(game: G, screen: S, sysdir: PathBuf) -> Self {
        Overlay {
            game,
            screen,
            faces: Faces::new(),
            button: MenuButton::default(),
            open: None,
            sysdir,
            quitting: false,
        }
    }

    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    /// Whether to wake soon. Gestures are decided on release, so nothing
    /// waits on the clock.
    pub fn wants_tick(&self) -> bool {
        false
    }

    /// See `MenuButton::set_others`.
    pub fn set_other_buttons(&mut self, down: Vec<Button>) {
        self.button.set_others(down);
    }

    /// While the menu is up the buttons are the menu's alone (the helper
    /// grabs the input device), so nothing pressed in it reaches the game.
    pub fn wants_grab(&self) -> bool {
        self.open.is_some()
    }

    pub fn handle(&mut self, ev: Event, now: Millis) {
        if self.quitting {
            return;
        }
        if let Some(open) = self.open.as_mut() {
            match ev {
                Event::Press(b) => {
                    open.held.push(b);
                    if open.chosen.is_none() {
                        // POWER cannot reach Onion while the buttons are
                        // grabbed: give the game back and let it through.
                        let action = match b {
                            Button::Power => Some(Action::Resume),
                            _ => open.menu.press(b),
                        };
                        match action {
                            Some(a) => open.chosen = Some(a),
                            None => self.redraw(),
                        }
                    }
                }
                Event::Release(b) => {
                    open.held.retain(|h| *h != b);
                    // Act once everything is let go, so the release of the
                    // button that chose never lands in the game either.
                    if open.held.is_empty() {
                        if let Some(a) = open.chosen {
                            self.close(a);
                        }
                    }
                }
                Event::Quit => {}
            }
            return;
        }
        match self.button.feed(ev, now) {
            Some(Gesture::Tap) => self.open_menu(),
            Some(Gesture::Hold) => {
                // slot: hold MENU to eject.
                self.quitting = true;
                self.game.quit();
            }
            None => {}
        }
    }

    pub fn tick(&mut self, _now: Millis) {}

    fn open_menu(&mut self) {
        self.game.pause();
        let pages = self.screen.pages().max(1);
        let shown = (self.screen.displayed().min(pages - 1)) as usize;
        let saved = (0..pages).map(|p| self.screen.capture(p)).collect();
        self.open = Some(Open {
            menu: Menu::default(),
            shown,
            saved,
            held: Vec::new(),
            chosen: None,
        });
        self.redraw();
    }

    /// Draw the menu on every page, not only the one reported as shown: the
    /// game may page-flip through more buffers than the driver's offset
    /// suggests, and the menu must be on whichever one is on screen.
    fn redraw(&mut self) {
        let Some(open) = self.open.as_ref() else {
            return;
        };
        let frame = self.faces.compose(&open.saved[open.shown], &open.menu);
        for page in 0..open.saved.len() {
            self.screen.draw_on(page as u32, &frame);
        }
    }

    /// Put the game's own picture back and do what was chosen.
    fn close(&mut self, action: Action) {
        let Some(open) = self.open.take() else {
            return;
        };
        for (page, saved) in open.saved.iter().enumerate() {
            self.screen.draw_on(page as u32, saved);
        }
        match action {
            Action::Resume => self.game.resume(),
            Action::Save => {
                self.game.resume();
                self.game.send("SAVE_STATE");
            }
            Action::Load => {
                self.game.resume();
                self.game.send("LOAD_STATE");
            }
            Action::Settings => {
                self.game.resume();
                self.game.send("MENU_TOGGLE");
            }
            Action::Switcher => {
                // Onion 4.3's Game Switcher action: set the flag the runtime
                // loop checks after the game, and quit the game.
                let _ = std::fs::write(self.sysdir.join(".runGameSwitcher"), b"");
                self.quitting = true;
                self.game.quit();
            }
            Action::Exit => {
                self.quitting = true;
                self.game.quit();
            }
        }
    }
}

// ---- the flag and the helper process ------------------------------------

fn flag_path(paths: &Paths) -> PathBuf {
    paths.tmp.join(FLAG)
}

fn read_pid(p: &Path) -> Option<i32> {
    std::fs::read_to_string(p).ok()?.trim().parse().ok()
}

fn alive(pid: i32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

/// Called when slot-mini starts: no game is running, so a helper left from
/// the last one (or its flag) must go, or MENU would stay dead in Onion.
pub fn clear_leftovers(paths: &Paths) {
    let flag = flag_path(paths);
    if let Some(pid) = read_pid(&flag) {
        if pid != std::process::id() as i32 && alive(pid) && is_helper(pid) {
            // SAFETY: signalling a process we started.
            unsafe { libc::kill(pid, libc::SIGTERM) };
        }
        let _ = std::fs::remove_file(&flag);
    }
}

fn is_helper(pid: i32) -> bool {
    std::fs::read(format!("/proc/{pid}/cmdline"))
        .map(|c| c.split(|b| *b == 0).any(|a| a == b"--ingame"))
        .unwrap_or(false)
}

/// Start the helper beside the game about to run: a separate session, so
/// it outlives slot-mini and the boot shim.
pub fn spawn(paths: &Paths) {
    use std::os::unix::process::CommandExt;
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let mut cmd = std::process::Command::new(exe);
    cmd.args(["--ingame", "--sd"])
        .arg(&paths.sd)
        .arg("--tmp")
        .arg(&paths.tmp)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    // SAFETY: setsid in the child before exec only detaches it.
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    if let Err(e) = cmd.spawn() {
        eprintln!("slot-mini: in-game menu: {e}");
    }
}

/// The game RetroArch, paused with its network commands and frozen with
/// SIGSTOP so it cannot draw over the menu.
#[cfg(target_os = "linux")]
pub struct RetroArch {
    pub pid: i32,
    sock: Option<std::net::UdpSocket>,
    toggled: bool,
    stopped: bool,
}

#[cfg(target_os = "linux")]
static FROZEN: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

#[cfg(target_os = "linux")]
impl RetroArch {
    /// RetroArch's process, as Onion's keymon finds it (`retroarch` or `ra32`).
    pub fn find() -> Option<i32> {
        let me = std::process::id() as i32;
        std::fs::read_dir("/proc").ok()?.flatten().find_map(|e| {
            let pid: i32 = e.file_name().to_str()?.parse().ok()?;
            let comm = std::fs::read_to_string(format!("/proc/{pid}/comm")).ok()?;
            let comm = comm.trim();
            (pid != me && (comm == "retroarch" || comm == "ra32")).then_some(pid)
        })
    }

    pub fn attach(pid: i32) -> RetroArch {
        let sock = std::net::UdpSocket::bind("127.0.0.1:0").ok();
        if let Some(s) = &sock {
            let _ = s.set_read_timeout(Some(std::time::Duration::from_millis(120)));
        }
        RetroArch {
            pid,
            sock,
            toggled: false,
            stopped: false,
        }
    }

    fn command(&self, cmd: &str) {
        if let Some(s) = &self.sock {
            let _ = s.send_to(cmd.as_bytes(), "127.0.0.1:55355");
        }
    }

    /// `GET_STATUS` answers `GET_STATUS PLAYING ...` or `GET_STATUS PAUSED ...`.
    fn status(&self) -> Option<String> {
        let s = self.sock.as_ref()?;
        let mut buf = [0u8; 512];
        while s.recv(&mut buf).is_ok() {} // drop stale replies
        let _ = s.set_read_timeout(Some(std::time::Duration::from_millis(120)));
        s.send_to(b"GET_STATUS", "127.0.0.1:55355").ok()?;
        let n = s.recv(&mut buf).ok()?;
        Some(String::from_utf8_lossy(&buf[..n]).into_owned())
    }

    fn signal(&self, sig: i32) {
        // SAFETY: signalling RetroArch, which we found by name.
        unsafe { libc::kill(self.pid, sig) };
    }
}

#[cfg(target_os = "linux")]
impl Game for RetroArch {
    fn running(&mut self) -> bool {
        match std::fs::read_to_string(format!("/proc/{}/stat", self.pid)) {
            Ok(stat) => stat.split_whitespace().nth(2).is_none_or(|s| s != "Z"),
            Err(_) => false,
        }
    }

    fn pause(&mut self) {
        // Pause properly first (RetroArch stops its audio), then freeze it so
        // it cannot redraw over the menu, as Onion does for its screenshots.
        if self.status().is_some_and(|s| s.contains("PLAYING")) {
            self.command("PAUSE_TOGGLE");
            self.toggled = true;
            for _ in 0..4 {
                if self.status().is_none_or(|s| s.contains("PAUSED")) {
                    break;
                }
            }
        }
        self.signal(libc::SIGSTOP);
        self.stopped = true;
        FROZEN.store(self.pid, std::sync::atomic::Ordering::SeqCst);
    }

    fn resume(&mut self) {
        if self.stopped {
            self.signal(libc::SIGCONT);
            self.stopped = false;
            FROZEN.store(0, std::sync::atomic::Ordering::SeqCst);
        }
        if self.toggled {
            self.command("PAUSE_TOGGLE");
            self.toggled = false;
        }
    }

    fn send(&mut self, command: &str) {
        self.command(command);
    }

    fn quit(&mut self) {
        if self.stopped {
            self.signal(libc::SIGCONT);
            self.stopped = false;
            FROZEN.store(0, std::sync::atomic::Ordering::SeqCst);
        }
        self.signal(libc::SIGTERM);
    }
}

#[cfg(target_os = "linux")]
static STOP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg(target_os = "linux")]
extern "C" fn on_signal(_: libc::c_int) {
    STOP.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// The helper's main loop (`slot-mini --ingame`). Returns when the game ends.
#[cfg(target_os = "linux")]
pub fn run(paths: &Paths, fb: &Path, input: &Path) {
    use std::sync::atomic::Ordering;
    use std::time::{Duration, Instant};

    let flag = flag_path(paths);
    let _ = std::fs::write(&flag, std::process::id().to_string());
    let cleanup = |flag: &Path| {
        // A frozen game must never be left behind.
        let pid = FROZEN.swap(0, Ordering::SeqCst);
        if pid > 0 {
            // SAFETY: resuming the RetroArch we froze.
            unsafe { libc::kill(pid, libc::SIGCONT) };
        }
        if read_pid(flag) == Some(std::process::id() as i32) {
            let _ = std::fs::remove_file(flag);
        }
    };
    {
        let flag = flag.clone();
        std::panic::set_hook(Box::new(move |info| {
            let pid = FROZEN.load(Ordering::SeqCst);
            if pid > 0 {
                // SAFETY: as above.
                unsafe { libc::kill(pid, libc::SIGCONT) };
            }
            let _ = std::fs::remove_file(&flag);
            eprintln!("slot-mini: in-game menu: {info}");
        }));
    }
    // SAFETY: the handler only stores to an atomic.
    unsafe {
        libc::signal(libc::SIGTERM, on_signal as *const () as libc::sighandler_t);
        libc::signal(libc::SIGINT, on_signal as *const () as libc::sighandler_t);
    }
    // The runtime starts the game after slot-mini exits; wait for it.
    let started = Instant::now();
    let pid = loop {
        if let Some(pid) = RetroArch::find() {
            break Some(pid);
        }
        if STOP.load(Ordering::SeqCst) || started.elapsed() > Duration::from_secs(30) {
            break None;
        }
        std::thread::sleep(Duration::from_millis(200));
    };
    let (Some(pid), Ok(screen), Ok(mut buttons)) = (
        pid,
        slot_mini_gfx::fbdev::Fbdev::open_any(fb),
        slot_mini_gfx::evdev::Evdev::open(input),
    ) else {
        return cleanup(&flag);
    };
    let mut overlay = Overlay::new(RetroArch::attach(pid), screen, paths.sysdir());
    let clock = Instant::now();
    while !STOP.load(Ordering::SeqCst) && overlay.game.running() {
        let wait = if overlay.wants_tick() || overlay.is_open() {
            20
        } else {
            250
        };
        let events = buttons.wait(wait);
        if !events.is_empty() {
            // What is down now, from the kernel (it already includes this
            // batch); the batch is then applied on top, in order.
            if let Some(down) = buttons.down_buttons() {
                overlay.set_other_buttons(down);
            }
        }
        for ev in events {
            overlay.handle(ev, clock.elapsed().as_millis() as Millis);
        }
        overlay.tick(clock.elapsed().as_millis() as Millis);
        buttons.grab(overlay.wants_grab());
    }
    buttons.grab(false);
    if overlay.is_open() {
        overlay.game.resume();
    }
    cleanup(&flag);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tap_hold_and_combinations() {
        let mut b = MenuButton::default();
        let press = |b: &mut MenuButton, k, t| b.feed(Event::Press(k), t);
        let release = |b: &mut MenuButton, k, t| b.feed(Event::Release(k), t);
        assert_eq!(press(&mut b, Button::Menu, 0), None);
        assert_eq!(release(&mut b, Button::Menu, 350), Some(Gesture::Tap));

        // A hold is told on release, once.
        press(&mut b, Button::Menu, 1000);
        assert_eq!(release(&mut b, Button::Menu, 1700), Some(Gesture::Hold));

        // MENU + R2 (save state) is neither, held short or long.
        press(&mut b, Button::Menu, 3000);
        press(&mut b, Button::R2, 3050);
        release(&mut b, Button::R2, 3100);
        assert_eq!(release(&mut b, Button::Menu, 3150), None);
        press(&mut b, Button::Menu, 4000);
        press(&mut b, Button::R2, 5500);
        assert_eq!(release(&mut b, Button::Menu, 5600), None, "no eject");
        release(&mut b, Button::R2, 5700);

        // The other button first (pressed together, a little early).
        press(&mut b, Button::L1, 6000);
        press(&mut b, Button::Menu, 6010);
        release(&mut b, Button::L1, 6100);
        assert_eq!(release(&mut b, Button::Menu, 6120), None);

        // Volume keys count too.
        press(&mut b, Button::Menu, 7000);
        press(&mut b, Button::VolumeUp, 7050);
        assert_eq!(release(&mut b, Button::Menu, 7100), None);
        release(&mut b, Button::VolumeUp, 7150);

        // And afterwards a plain tap is a tap again.
        press(&mut b, Button::Menu, 8000);
        assert_eq!(release(&mut b, Button::Menu, 8100), Some(Gesture::Tap));

        // A release that went missing: the device's own key state wins.
        press(&mut b, Button::X, 9000);
        b.set_others(vec![Button::Menu]);
        press(&mut b, Button::Menu, 9100);
        assert_eq!(release(&mut b, Button::Menu, 9200), Some(Gesture::Tap));
    }

    #[test]
    fn menu_moves_and_chooses() {
        let mut m = Menu::default();
        assert_eq!(m.press(Button::Up), None);
        assert_eq!(Row::ALL[m.row], Row::ExitGame, "wraps");
        assert_eq!(m.press(Button::A), Some(Action::Exit));
        m.press(Button::Down);
        m.press(Button::Down);
        assert_eq!(m.press(Button::A), Some(Action::Save));
        assert_eq!(m.press(Button::B), Some(Action::Resume));
        assert_eq!(m.press(Button::Menu), Some(Action::Resume));
    }
}
