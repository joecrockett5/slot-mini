//! The PC app without its window: the shared app on a library folder,
//! RetroArch run around it, and the requests only a window can carry out.

use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use slot_mini::about::{Host, HostRow};
use slot_mini::legend::Glyphs;
use slot_mini::paths::Pc;
use slot_mini::sound::Sound;
use slot_mini::status::Status;
use slot_mini::{App, Exit, Options, Paths, Pointer};
use slot_mini_gfx::{Button, Canvas, Event, Sprite, SCREEN_H, SCREEN_W};
use slot_mini_store::{Cart, Core};

use crate::config::Config;
use crate::input::{Device, Key, Stick};
use crate::net::{self, PcRelease};
use crate::retroarch::{self, RetroArch, Run};

/// Something only the window (or the desktop) can do, asked of `main`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    /// A game is starting: get out of its way.
    Hide,
    /// The game ended: come back to the front.
    Show,
    Fullscreen(bool),
    Quit,
    /// Ask for the library folder; answer with `PcApp::chose_library`.
    PickLibrary,
    /// Ask for `retroarch.exe`; answer with `PcApp::chose_retroarch`.
    PickRetroArch,
    /// Ask for an Onion SD card to import; answer with `PcApp::chose_card`.
    PickCard,
    /// Open a page in the browser.
    OpenUrl(String),
    /// The library changed: start the app again on the new one.
    Reopen,
}

/// MENU's update row.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum Update {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Available(PcRelease),
    Failed(String),
}

/// MENU's cores row: nothing downloads until the player has said so
/// twice.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum CoresJob {
    #[default]
    Idle,
    Asked,
    Fetching,
    Failed(String),
}

/// A game that has just ended, for whoever listens (brief 3's sync).
#[derive(Clone, Debug)]
pub struct Played {
    pub cart: Cart,
    /// The core it ran on: the PC's choice for this game.
    pub core: Core,
    /// Where RetroArch kept its save (the file may not exist).
    pub save: Option<PathBuf>,
    /// Whether RetroArch started and ended normally.
    pub ok: bool,
}

struct Game {
    run: Run,
    done: Receiver<Option<ExitStatus>>,
}

/// What the app is built with; tests replace the parts that touch the PC.
pub struct PcOptions {
    /// Where `config.txt` lives.
    pub config_dir: PathBuf,
    /// Where to look for RetroArch when none is remembered.
    pub places: Vec<PathBuf>,
    pub status: Box<dyn Status>,
    pub sound: Option<Box<dyn Sound>>,
    pub face_wait: Option<Duration>,
    pub seed: u64,
    /// GitHub's list of releases (`net::releases_url`).
    pub releases_url: String,
    /// libretro's buildbot folder for this system's cores.
    pub buildbot: String,
}

pub struct PcApp {
    pub config: Config,
    config_dir: PathBuf,
    places: Vec<PathBuf>,
    pub app: App,
    pub retroarch: Option<RetroArch>,
    /// The 640x480 frame, as the Miyoo's screen would show it.
    pub frame: Canvas,
    game: Option<Game>,
    /// What `main` should do next, oldest first.
    pub requests: Vec<Request>,
    hooks: Vec<Box<dyn FnMut(&Played)>>,
    /// What the player last used.
    pub device: Device,
    stick: Stick,
    l2_down: bool,
    now: u64,
    pub update: Update,
    update_rx: Option<Receiver<Result<Option<PcRelease>, String>>>,
    releases_url: String,
    pub cores_job: CoresJob,
    cores_rx: Option<Receiver<Result<(), String>>>,
    buildbot: String,
}

impl PcApp {
    /// Open `library` (created if it is not there yet).
    pub fn new(library: &Path, opts: PcOptions, now: u64) -> PcApp {
        slot_mini_store::set_app_dir(crate::APP_DIR);
        let config = Config::read(&opts.config_dir);
        make_library(library);
        let retroarch = retroarch::find(config.retroarch.as_deref(), &opts.places);
        let paths = Paths::pc(
            library,
            std::env::temp_dir().join("slot-mini-pc"),
            Pc {
                cores: retroarch.as_ref().map(|r| r.cores.clone()),
            },
        );
        let app = App::new(
            paths,
            opts.status,
            Options {
                sound: Some(
                    opts.sound
                        .unwrap_or_else(|| Box::new(slot_mini::sound::Silent)),
                ),
                face_wait: opts.face_wait,
                seed: opts.seed,
            },
            now,
        );
        let mut pc = PcApp {
            config,
            config_dir: opts.config_dir,
            places: opts.places,
            app,
            retroarch,
            frame: Canvas::new(SCREEN_W, SCREEN_H),
            game: None,
            requests: Vec::new(),
            hooks: Vec::new(),
            device: Device::default(),
            stick: Stick::default(),
            l2_down: false,
            now,
            update: Update::Idle,
            update_rx: None,
            releases_url: opts.releases_url,
            cores_job: CoresJob::Idle,
            cores_rx: None,
            buildbot: opts.buildbot,
        };
        pc.app.set_glyphs(glyphs(pc.device));
        pc.refresh_host();
        pc
    }

    pub fn library(&self) -> &Path {
        &self.app.paths.sd
    }

    /// Call `hook` after every game, once RetroArch has gone and its save
    /// is on disk (brief 3 syncs here).
    pub fn on_game_exit(&mut self, hook: impl FnMut(&Played) + 'static) {
        self.hooks.push(Box::new(hook));
    }

    /// Whether a game is running. The app stands still until it ends, so
    /// there is never a second one.
    pub fn in_game(&self) -> bool {
        self.game.is_some()
    }

    fn save_config(&self) {
        if let Err(e) = self.config.write(&self.config_dir) {
            eprintln!("slot-mini-pc: config: {e}");
        }
    }

    // ---- input ---------------------------------------------------------

    pub fn button(&mut self, b: Button, down: bool, device: Device) {
        self.device = device;
        self.app.set_glyphs(glyphs(device));
        if self.in_game() {
            return;
        }
        if b == Button::L2 {
            self.l2_down = down;
        }
        self.app.handle(match down {
            true => Event::Press(b),
            false => Event::Release(b),
        });
    }

    pub fn key(&mut self, key: Key, down: bool) {
        match key {
            Key::Button(b) => self.button(b, down, Device::Keyboard),
            Key::Letter(next) if down => self.letter(next),
            Key::Letter(_) => {}
        }
    }

    /// Jump to the letter before or after: Up / Down on the shelf, and
    /// with L2 held in the grid.
    fn letter(&mut self, next: bool) {
        let b = if next { Button::Down } else { Button::Up };
        let chord = self.app.grid_on() && !self.l2_down;
        if chord {
            self.button(Button::L2, true, Device::Keyboard);
        }
        self.button(b, true, Device::Keyboard);
        self.button(b, false, Device::Keyboard);
        if chord {
            self.button(Button::L2, false, Device::Keyboard);
        }
    }

    /// The left stick moved (up is positive `y`).
    pub fn stick(&mut self, x: f32, y: f32) {
        for (b, down) in self.stick.moved(x, y) {
            self.button(b, down, Device::Controller);
        }
    }

    /// The mouse, in the frame's pixels.
    pub fn pointer(&mut self, p: Pointer) {
        if !self.in_game() {
            self.app.pointer(p);
        }
    }

    /// F11 or Alt+Enter.
    pub fn toggle_fullscreen(&mut self) {
        self.config.fullscreen = !self.config.fullscreen;
        self.save_config();
        self.requests
            .push(Request::Fullscreen(self.config.fullscreen));
        self.refresh_host();
    }

    /// The window came to the front: the library or RetroArch may have
    /// changed while it was away.
    pub fn focused(&mut self) {
        if self.in_game() {
            return;
        }
        let found = retroarch::find(self.config.retroarch.as_deref(), &self.places);
        if found != self.retroarch {
            self.set_retroarch(found);
        }
        self.app.rescan();
    }

    pub fn set_retroarch(&mut self, ra: Option<RetroArch>) {
        self.app.set_cores(ra.as_ref().map(|r| r.cores.clone()));
        self.retroarch = ra;
        self.refresh_host();
    }

    // ---- MENU's rows ---------------------------------------------------

    /// The cores RetroArch has no file for.
    pub fn missing_cores(&self) -> Vec<Core> {
        Core::ALL
            .into_iter()
            .filter(|c| !slot_mini::cores::installed(&self.app.paths, c.libretro()))
            .collect()
    }

    /// Tell MENU what its PC rows show now.
    fn refresh_host(&mut self) {
        let (update, update_note) = match &self.update {
            Update::Idle => ("Check for updates".to_string(), None),
            Update::Checking => ("Checking for updates...".to_string(), None),
            Update::UpToDate => (
                "Up to date".to_string(),
                Some(format!(
                    "slot-mini PC {} is the latest release.",
                    crate::VERSION
                )),
            ),
            Update::Available(r) => (
                format!("Get slot-mini PC {}", r.version),
                Some("Opens its page. Unzip the download over this copy.".to_string()),
            ),
            Update::Failed(e) => ("Check for updates".to_string(), Some(e.clone())),
        };
        let names = self
            .missing_cores()
            .iter()
            .map(|c| c.text())
            .collect::<Vec<_>>()
            .join(", ");
        let get = Some("Get missing cores".to_string());
        let (cores, cores_note) = match &self.cores_job {
            _ if self.retroarch.is_none() => (None, None),
            CoresJob::Fetching => (
                Some("Downloading cores...".to_string()),
                Some("From libretro's buildbot into RetroArch's cores folder.".to_string()),
            ),
            _ if names.is_empty() => (None, None),
            CoresJob::Idle => (get, Some(format!("Not installed: {names}."))),
            CoresJob::Asked => (
                get,
                Some(format!(
                    "Again to download {names} from libretro's buildbot."
                )),
            ),
            CoresJob::Failed(e) => (get, Some(e.clone())),
        };
        self.app.set_host(Host {
            version: crate::VERSION.to_string(),
            fullscreen: self.config.fullscreen,
            xbox: self.config.xbox_layout,
            retroarch: self.retroarch.as_ref().map(|r| r.exe.display().to_string()),
            library: self.library().display().to_string(),
            update,
            update_note,
            cores,
            cores_note,
        });
    }

    /// A PC row on MENU was chosen.
    fn host_action(&mut self, row: HostRow) {
        if row != HostRow::Cores && self.cores_job == CoresJob::Asked {
            self.cores_job = CoresJob::Idle;
        }
        match row {
            HostRow::Import => self.requests.push(Request::PickCard),
            HostRow::Quit => self.requests.push(Request::Quit),
            HostRow::Fullscreen => self.toggle_fullscreen(),
            HostRow::Controller => {
                self.config.xbox_layout = !self.config.xbox_layout;
                self.save_config();
            }
            HostRow::RetroArch => self.requests.push(Request::PickRetroArch),
            HostRow::Library => self.requests.push(Request::PickLibrary),
            HostRow::Update => match &self.update {
                Update::Available(r) => self.requests.push(Request::OpenUrl(r.page.clone())),
                Update::Checking => {}
                _ => self.check_for_update(),
            },
            HostRow::Cores => match self.cores_job {
                CoresJob::Idle | CoresJob::Failed(_) => self.cores_job = CoresJob::Asked,
                CoresJob::Asked => self.fetch_cores(),
                CoresJob::Fetching => {}
            },
            HostRow::Folders | HostRow::Sync | HostRow::SyncSoon => {}
        }
        self.refresh_host();
    }

    fn check_for_update(&mut self) {
        let (tx, rx) = mpsc::channel();
        let url = self.releases_url.clone();
        let started = std::thread::Builder::new()
            .name("slot-mini-pc-update".into())
            .spawn(move || {
                let _ = tx.send(net::check(&url, crate::VERSION));
            });
        match started {
            Ok(_) => {
                self.update = Update::Checking;
                self.update_rx = Some(rx);
            }
            Err(e) => self.update = Update::Failed(format!("Could not check: {e}")),
        }
    }

    fn fetch_cores(&mut self) {
        let Some(ra) = self.retroarch.clone() else {
            return;
        };
        let missing = self.missing_cores();
        let base = self.buildbot.clone();
        let (tx, rx) = mpsc::channel();
        let started = std::thread::Builder::new()
            .name("slot-mini-pc-cores".into())
            .spawn(move || {
                let result = missing
                    .into_iter()
                    .try_for_each(|core| net::fetch_core(&base, core, &ra.cores));
                let _ = tx.send(result);
            });
        match started {
            Ok(_) => {
                self.cores_job = CoresJob::Fetching;
                self.cores_rx = Some(rx);
            }
            Err(e) => self.cores_job = CoresJob::Failed(format!("Could not start: {e}")),
        }
    }

    /// Take what the worker threads have finished.
    fn poll_workers(&mut self) {
        let mut changed = false;
        if let Some(result) = self.update_rx.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.update_rx = None;
            self.update = match result {
                Ok(Some(release)) => Update::Available(release),
                Ok(None) => Update::UpToDate,
                Err(e) => Update::Failed(e),
            };
            changed = true;
        }
        if let Some(result) = self.cores_rx.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.cores_rx = None;
            self.cores_job = match result {
                Ok(()) => CoresJob::Idle,
                Err(e) => CoresJob::Failed(e),
            };
            self.app.cores_changed();
            changed = true;
        }
        if changed {
            self.refresh_host();
        }
    }

    /// The folder dialog's answer: the library to use from now on.
    pub fn chose_library(&mut self, folder: Option<PathBuf>) {
        let Some(folder) = folder else {
            return;
        };
        if folder == self.library() {
            return;
        }
        self.config.library = Some(folder);
        self.save_config();
        self.requests.push(Request::Reopen);
    }

    /// The folder dialog's answer: the Onion card to import from.
    pub fn chose_card(&mut self, card: Option<PathBuf>) {
        let Some(_card) = card else {
            return;
        };
    }

    /// The file dialog's answer: where RetroArch is.
    pub fn chose_retroarch(&mut self, exe: Option<PathBuf>) {
        let Some(exe) = exe else {
            return;
        };
        if !exe.is_file() {
            return self
                .app
                .set_about_note(Some("That file is not there.".into()));
        }
        self.config.retroarch = Some(exe.clone());
        self.save_config();
        self.set_retroarch(Some(retroarch::at(&exe)));
    }

    /// The window is closing: remember where the shelf was.
    pub fn close(&mut self, window: Option<([i32; 4], bool)>) {
        if let Some((rect, maximized)) = window {
            self.config.maximized = maximized;
            if !maximized && !self.config.fullscreen {
                self.config.window = Some(rect);
            }
        }
        self.save_config();
        if let Err(e) = self.app.state().write(&self.app.paths) {
            eprintln!("slot-mini-pc: state: {e}");
        }
        self.app.close_sound();
    }

    // ---- frames --------------------------------------------------------

    /// Advance to `now` (milliseconds since start).
    pub fn tick(&mut self, now: u64) {
        self.now = now;
        if let Some(game) = &self.game {
            match game.done.try_recv() {
                Ok(status) => self.game_over(status),
                Err(mpsc::TryRecvError::Disconnected) => self.game_over(None),
                Err(mpsc::TryRecvError::Empty) => return,
            }
        }
        self.poll_workers();
        self.app.tick(now);
        if let Some(row) = self.app.take_host_action() {
            self.host_action(row);
        }
        match self.app.exit.take() {
            Some(Exit::Launch { cart, fresh }) => self.launch(cart, fresh),
            Some(_) => self.requests.push(Request::Quit),
            None => {}
        }
    }

    /// Draw the frame. Returns whether it changed.
    pub fn render(&mut self) -> bool {
        if self.in_game() {
            return false;
        }
        self.app.render(&mut self.frame)
    }

    /// The wallpaper, for the window's borders.
    pub fn wallpaper(&self) -> Option<(&Sprite, u64)> {
        self.app.wallpaper()
    }

    // ---- games ---------------------------------------------------------

    fn launch(&mut self, cart: Cart, fresh: bool) {
        let Some(ra) = self.retroarch.clone() else {
            self.app.set_notice("RetroArch was not found".into());
            return self.app.game_ended();
        };
        let run = retroarch::plan(&self.app.paths, &ra, &cart, fresh);
        let cfg = self.config_dir.join("launch.cfg");
        let spawned = retroarch::command(&ra, &run, self.library(), &cfg, self.config.fullscreen)
            .and_then(|mut cmd| cmd.spawn());
        let mut child = match spawned {
            Ok(child) => child,
            Err(e) => {
                eprintln!("slot-mini-pc: {}: {e}", ra.exe.display());
                self.app.set_notice("RetroArch could not start".into());
                return self.app.game_ended();
            }
        };
        self.app.game_started(&cart);
        // Waiting blocks, so a thread does it; `tick` only looks.
        let (tx, done) = mpsc::channel();
        let waiter = std::thread::Builder::new()
            .name("slot-mini-pc-game".into())
            .spawn(move || {
                let _ = tx.send(child.wait().ok());
            });
        if let Err(e) = waiter {
            eprintln!("slot-mini-pc: the game's waiter did not start: {e}");
        }
        self.game = Some(Game { run, done });
        self.requests.push(Request::Hide);
    }

    fn game_over(&mut self, status: Option<ExitStatus>) {
        let Some(game) = self.game.take() else {
            return;
        };
        let ok = status.is_some_and(|s| s.success());
        if !ok {
            eprintln!("slot-mini-pc: RetroArch ended with {status:?}");
        }
        self.requests.push(Request::Show);
        self.app.game_ended();
        let played = Played {
            cart: game.run.cart,
            core: game.run.core,
            save: game.run.save,
            ok,
        };
        for hook in &mut self.hooks {
            hook(&played);
        }
    }
}

fn glyphs(device: Device) -> Glyphs {
    match device {
        Device::Keyboard => Glyphs::Keyboard,
        Device::Controller => Glyphs::Pad,
    }
}

/// A library's folders, laid out like an Onion card.
pub fn make_library(library: &Path) {
    for dir in [
        "Roms/GBA",
        "Roms/GBC",
        "Roms/GB",
        "Labels",
        "Wallpapers",
        "Saves/CurrentProfile/saves",
        "Saves/CurrentProfile/states",
        "App/slot-mini-pc/data",
        "App/slot-mini-pc/cache",
    ] {
        if let Err(e) = std::fs::create_dir_all(library.join(dir)) {
            eprintln!("slot-mini-pc: {}: {e}", library.join(dir).display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::tests::{serve, zip_of, RELEASES};
    use slot_mini::status::FixedStatus;
    use std::collections::HashMap;

    struct Rig {
        dir: tempfile::TempDir,
        pc: PcApp,
        now: u64,
    }

    /// A library, and a RetroArch with only `cores` in its cores folder.
    /// `base` stands in for GitHub and the buildbot.
    fn rig(cores: &[Core], retroarch: bool, base: &str) -> Rig {
        let dir = tempfile::tempdir().unwrap();
        let library = dir.path().join("library");
        slot_mini::fixture::build(&library, true).unwrap();
        let exe = dir.path().join("ra").join(retroarch::EXE);
        std::fs::create_dir_all(exe.parent().unwrap().join("cores")).unwrap();
        std::fs::write(&exe, b"").unwrap();
        for core in cores {
            let file = retroarch::core_file(&exe.parent().unwrap().join("cores"), *core);
            std::fs::write(file, b"a core").unwrap();
        }
        let opts = PcOptions {
            config_dir: dir.path().join("config"),
            places: if retroarch { vec![exe] } else { Vec::new() },
            status: Box::new(FixedStatus {
                battery: None,
                wifi: true,
                clock: (12, 0),
                charging: false,
            }),
            sound: None,
            face_wait: None,
            seed: 7,
            releases_url: format!("{base}/releases"),
            buildbot: base.to_string(),
        };
        let mut pc = PcApp::new(&library, opts, 0);
        pc.tick(0);
        Rig { dir, pc, now: 0 }
    }

    impl Rig {
        fn tap(&mut self, b: Button) {
            for down in [true, false] {
                self.pc.button(b, down, Device::Controller);
                self.now += 16;
                self.pc.tick(self.now);
            }
        }

        fn taps(&mut self, buttons: &[Button]) {
            for b in buttons {
                self.tap(*b);
            }
        }

        fn wait(&mut self, what: &str, done: impl Fn(&PcApp) -> bool) {
            let start = std::time::Instant::now();
            while !done(&self.pc) {
                assert!(
                    start.elapsed() < Duration::from_secs(60),
                    "gave up on {what}"
                );
                std::thread::sleep(Duration::from_millis(5));
                self.now += 16;
                self.pc.tick(self.now);
            }
        }

        fn config(&self) -> Config {
            Config::read(&self.dir.path().join("config"))
        }
    }

    const ALL: [Core; 3] = Core::ALL;
    const NOWHERE: &str = "http://127.0.0.1:9";
    use Button::{Down, Menu, A};

    #[test]
    fn the_switches_change_the_config_and_the_window() {
        let mut r = rig(&ALL, true, NOWHERE);
        // MENU, Settings, down to Fullscreen.
        r.taps(&[Menu, A, Down, Down, Down, A]);
        assert_eq!(r.pc.requests, [Request::Fullscreen(true)]);
        assert!(r.config().fullscreen && r.pc.app.host().fullscreen);
        r.taps(&[Down, Button::Right]);
        assert!(r.config().xbox_layout && r.pc.app.host().xbox);
        r.tap(Button::Left);
        assert!(!r.config().xbox_layout && !r.pc.app.host().xbox);
        // Back out, and Quit.
        r.taps(&[Button::B, Button::Up, A]);
        assert_eq!(r.pc.requests.last(), Some(&Request::Quit));
    }

    #[test]
    fn the_update_row_checks_pc_releases_and_opens_the_newer_ones_page() {
        let base = serve(HashMap::from([(
            "/releases".to_string(),
            RELEASES.as_bytes().to_vec(),
        )]));
        let mut r = rig(&ALL, true, &base);
        assert_eq!(r.pc.app.host().update, "Check for updates");
        r.taps(&[Menu, Down, Down, A]);
        r.wait("the check", |pc| pc.update != Update::Checking);
        assert_eq!(r.pc.app.host().update, "Get slot-mini PC 0.10.1");
        assert!(r.pc.requests.is_empty());
        r.tap(A);
        assert_eq!(
            r.pc.requests,
            [Request::OpenUrl("https://example.test/pc-v0.10.1".into())]
        );
    }

    #[test]
    fn a_failed_check_says_so_and_can_be_tried_again() {
        let mut r = rig(&ALL, true, NOWHERE);
        r.taps(&[Menu, Down, Down, A]);
        r.wait("the check", |pc| pc.update != Update::Checking);
        assert_eq!(r.pc.update, Update::Failed("No connection.".into()));
        assert_eq!(r.pc.app.host().update, "Check for updates");
        assert_eq!(
            r.pc.app.host().update_note.as_deref(),
            Some("No connection.")
        );
        r.tap(A);
        assert_eq!(r.pc.update, Update::Checking);
    }

    #[test]
    fn missing_cores_are_downloaded_only_after_asking_twice() {
        let base = serve(HashMap::from([
            (
                "/gpsp_libretro.dll.zip".to_string(),
                zip_of("gpsp_libretro.dll", b"gpsp"),
            ),
            (
                "/gambatte_libretro.dll.zip".to_string(),
                zip_of("gambatte_libretro.dll", b"gambatte"),
            ),
        ]));
        let mut r = rig(&[Core::Mgba], true, &base);
        assert_eq!(r.pc.missing_cores(), [Core::Gpsp, Core::Gambatte]);
        let host = r.pc.app.host().clone();
        assert_eq!(host.cores.as_deref(), Some("Get missing cores"));
        assert_eq!(
            host.cores_note.as_deref(),
            Some("Not installed: gpSP, Gambatte.")
        );
        // MENU, Settings, Folders, the cores row.
        r.taps(&[Menu, A, Down, Down, Down, Down, Down, A, Down, Down, A]);
        assert_eq!(r.pc.cores_job, CoresJob::Asked);
        assert_eq!(r.pc.missing_cores().len(), 2, "nothing fetched yet");
        r.tap(A);
        assert_eq!(r.pc.cores_job, CoresJob::Fetching);
        r.wait("the cores", |pc| pc.cores_job != CoresJob::Fetching);
        assert_eq!(r.pc.cores_job, CoresJob::Idle);
        assert!(r.pc.missing_cores().is_empty());
        assert_eq!(r.pc.app.host().cores, None);
        let cores = r.dir.path().join("ra/cores");
        assert_eq!(
            std::fs::read(retroarch::core_file(&cores, Core::Gpsp)).unwrap(),
            b"gpsp"
        );
    }

    #[test]
    fn a_core_that_cannot_be_fetched_is_reported() {
        let mut r = rig(&[Core::Mgba, Core::Gambatte], true, NOWHERE);
        r.taps(&[Menu, A, Down, Down, Down, Down, Down, A, Down, Down, A, A]);
        r.wait("the cores", |pc| pc.cores_job != CoresJob::Fetching);
        assert_eq!(
            r.pc.cores_job,
            CoresJob::Failed("gpSP could not be downloaded.".into())
        );
        assert_eq!(r.pc.missing_cores(), [Core::Gpsp]);
    }

    #[test]
    fn retroarch_and_the_library_are_chosen_from_menu() {
        let mut r = rig(&ALL, false, NOWHERE);
        assert_eq!(r.pc.retroarch, None);
        assert_eq!(r.pc.app.host().retroarch, None);
        assert_eq!(
            r.pc.app.host().cores,
            None,
            "no cores row without RetroArch"
        );
        r.taps(&[Menu, A, Down, Down, Down, Down, Down, A, A]);
        assert_eq!(r.pc.requests, [Request::PickRetroArch]);
        // Cancelled, then a file that is not there, then the real one.
        r.pc.chose_retroarch(None);
        r.pc.chose_retroarch(Some(r.dir.path().join("nowhere.exe")));
        assert_eq!(r.pc.retroarch, None);
        let exe = r.dir.path().join("ra").join(retroarch::EXE);
        r.pc.chose_retroarch(Some(exe.clone()));
        assert_eq!(r.pc.retroarch.as_ref().unwrap().exe, exe);
        assert_eq!(r.config().retroarch, Some(exe.clone()));
        assert_eq!(r.pc.app.host().retroarch, Some(exe.display().to_string()));
        assert!(r.pc.missing_cores().is_empty());

        r.pc.requests.clear();
        r.taps(&[Down, A]);
        assert_eq!(r.pc.requests, [Request::PickLibrary]);
        r.pc.requests.clear();
        r.pc.chose_library(None);
        r.pc.chose_library(Some(r.pc.library().to_path_buf()));
        assert!(r.pc.requests.is_empty(), "the same folder changes nothing");
        let other = r.dir.path().join("other");
        r.pc.chose_library(Some(other.clone()));
        assert_eq!(r.pc.requests, [Request::Reopen]);
        assert_eq!(r.config().library, Some(other));
    }

    #[test]
    fn the_legend_follows_the_device_in_hand() {
        let mut r = rig(&ALL, true, NOWHERE);
        r.pc.key(Key::Button(Menu), true);
        r.pc.key(Key::Button(Menu), false);
        r.pc.tick(16);
        assert_eq!(r.pc.app.about.as_ref().unwrap().glyphs, Glyphs::Keyboard);
        r.pc.stick(0.0, -1.0);
        assert_eq!(r.pc.app.about.as_ref().unwrap().glyphs, Glyphs::Pad);
    }
}
