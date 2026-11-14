//! The PC app without its window: the shared app on a library folder,
//! RetroArch run around it, and the requests only a window can carry out.

use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use slot_mini::paths::Pc;
use slot_mini::sound::Sound;
use slot_mini::status::Status;
use slot_mini::{App, Exit, Options, Paths};
use slot_mini_gfx::{Button, Canvas, Event, Sprite, SCREEN_H, SCREEN_W};
use slot_mini_store::{Cart, Core};

use crate::config::Config;
use crate::input::{Device, Key, Stick};
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
        PcApp {
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
        }
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

    /// F11 or Alt+Enter.
    pub fn toggle_fullscreen(&mut self) {
        self.config.fullscreen = !self.config.fullscreen;
        self.save_config();
        self.requests
            .push(Request::Fullscreen(self.config.fullscreen));
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
        self.app.tick(now);
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
