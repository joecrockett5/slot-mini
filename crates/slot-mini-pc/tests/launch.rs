//! A game from the shelf to RetroArch and back, with a stand-in RetroArch
//! (`fake-retroarch`) that notes how it was run and writes a save.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use slot_mini::status::FixedStatus;
use slot_mini::Phase;
use slot_mini_gfx::Button;
use slot_mini_pc::input::Device;
use slot_mini_pc::pcapp::{PcOptions, Played};
use slot_mini_pc::{PcApp, Request};
use slot_mini_store::Core;

const FRAME_MS: u64 = 16;

struct Rig {
    dir: tempfile::TempDir,
    pc: PcApp,
    now: u64,
}

fn status() -> Box<FixedStatus> {
    Box::new(FixedStatus {
        battery: None,
        wifi: true,
        clock: (12, 0),
        charging: false,
    })
}

impl Rig {
    /// A library with the fixture's games and a RetroArch folder holding
    /// the stand-in and its cores. `setup` is the stand-in's `fake.txt`.
    fn new(setup: &str) -> Rig {
        let dir = tempfile::tempdir().unwrap();
        let library = dir.path().join("library");
        slot_mini::fixture::build(&library, true).unwrap();
        let data = library.join("App/slot-mini-pc/data");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(data.join("settings.txt"), "online_labels = off\n").unwrap();
        let ra = dir.path().join("RetroArch");
        std::fs::create_dir_all(ra.join("cores")).unwrap();
        let exe = ra.join(slot_mini_pc::retroarch::EXE);
        std::fs::copy(env!("CARGO_BIN_EXE_fake-retroarch"), &exe).unwrap();
        for core in ["mgba_libretro", "gambatte_libretro"] {
            let name = format!("{core}{}", std::env::consts::DLL_SUFFIX);
            std::fs::write(ra.join("cores").join(name), b"not a real core").unwrap();
        }
        let log = dir.path().join("log.txt");
        std::fs::write(
            ra.join("fake.txt"),
            format!("log = {}\n{setup}", log.display()),
        )
        .unwrap();
        let opts = PcOptions {
            config_dir: dir.path().join("config"),
            places: vec![exe],
            status: status(),
            sound: None,
            face_wait: None,
            seed: 7,
            releases_url: "http://127.0.0.1:9/releases".into(),
            buildbot: "http://127.0.0.1:9".into(),
        };
        let mut pc = PcApp::new(&library, opts, 0);
        pc.tick(0);
        pc.render();
        Rig { dir, pc, now: 0 }
    }

    fn log(&self) -> String {
        std::fs::read_to_string(self.dir.path().join("log.txt")).unwrap_or_default()
    }

    fn library(&self) -> PathBuf {
        self.dir.path().join("library")
    }

    fn frame(&mut self) {
        self.now += FRAME_MS;
        self.pc.tick(self.now);
        self.pc.render();
    }

    fn tap(&mut self, b: Button) {
        self.pc.button(b, true, Device::Keyboard);
        self.frame();
        self.pc.button(b, false, Device::Keyboard);
        self.frame();
    }

    fn wait_until(&mut self, what: &str, done: impl Fn(&Rig) -> bool) {
        let start = Instant::now();
        while !done(self) {
            assert!(
                start.elapsed() < Duration::from_secs(60),
                "gave up waiting for {what}"
            );
            std::thread::sleep(Duration::from_millis(2));
            self.frame();
        }
    }

    fn take(&mut self) -> Vec<Request> {
        std::mem::take(&mut self.pc.requests)
    }
}

fn at_rest(r: &Rig) -> bool {
    matches!(r.pc.app.phase, Phase::Shelf)
}

fn arg_after(log: &str, flag: &str) -> PathBuf {
    let args: Vec<&str> = log.lines().next().unwrap()["args: ".len()..]
        .split('\t')
        .collect();
    let i = args.iter().position(|a| *a == flag).unwrap();
    PathBuf::from(args[i + 1])
}

#[test]
fn a_game_runs_in_retroarch_and_comes_back_to_its_cart() {
    let mut rig = Rig::new("ms = 150\n");
    let played: Rc<RefCell<Vec<Played>>> = Rc::default();
    let seen = played.clone();
    rig.pc
        .on_game_exit(move |p| seen.borrow_mut().push(p.clone()));
    rig.tap(Button::Right);
    let cart = rig.pc.app.current().unwrap().1.clone();

    rig.tap(Button::A);
    rig.wait_until("the game to start", |r| r.pc.in_game());
    assert_eq!(rig.take(), [Request::Hide]);
    assert!(played.borrow().is_empty());

    // While it runs the app stands still: no second game, no input.
    rig.tap(Button::A);
    rig.tap(Button::Right);
    assert!(!rig.pc.render());

    rig.wait_until("the game to end", |r| !r.pc.in_game());
    assert_eq!(rig.take(), [Request::Show]);
    assert!(matches!(rig.pc.app.phase, Phase::Ejecting { .. }));
    rig.wait_until("the cart to come back out", at_rest);
    assert_eq!(rig.pc.app.current().unwrap().1.stem, cart.stem);

    // It was run as the brief says, on the files the library holds.
    let log = rig.log();
    let core = arg_after(&log, "-L");
    assert_eq!(
        core.parent().unwrap(),
        rig.dir.path().join("RetroArch/cores")
    );
    assert!(core
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("mgba_libretro"));
    assert!(log.contains(cart.rom.to_str().unwrap()), "{log}");
    let saves = rig.library().join("Saves/CurrentProfile/saves");
    let states = rig.library().join("Saves/CurrentProfile/states");
    // RetroArch's config takes forward slashes on every system.
    let cfg_path = |p: &Path| p.display().to_string().replace('\\', "/");
    assert!(
        log.contains(&format!("savefile_directory = \"{}\"", cfg_path(&saves))),
        "{log}"
    );
    assert!(
        log.contains(&format!("savestate_directory = \"{}\"", cfg_path(&states))),
        "{log}"
    );
    assert!(log.contains("sort_savefiles_enable = \"true\""), "{log}");
    assert!(log.contains("savestate_auto_load = \"true\""), "{log}");

    // The hook ran once, after the save was on disk.
    let played = played.borrow();
    assert_eq!(played.len(), 1);
    assert_eq!(played[0].cart.stem, cart.stem);
    assert_eq!(played[0].core, Core::Mgba);
    assert!(played[0].ok);
    let save = played[0].save.clone().unwrap();
    assert_eq!(save, saves.join("mGBA").join(format!("{}.srm", cart.stem)));
    assert_eq!(
        std::fs::read(&save).unwrap(),
        b"saved by the fake RetroArch"
    );

    // And the game is the most recent.
    let recents = std::fs::read_to_string(rig.library().join("App/slot-mini-pc/data/recents.txt"))
        .unwrap_or_default();
    assert!(
        recents.lines().next().unwrap_or("").contains(&cart.stem),
        "{recents}"
    );
}

#[test]
fn a_retroarch_that_fails_still_brings_the_shelf_back() {
    let mut rig = Rig::new("ms = 20\nexit = 3\n");
    let played: Rc<RefCell<Vec<Played>>> = Rc::default();
    let seen = played.clone();
    rig.pc
        .on_game_exit(move |p| seen.borrow_mut().push(p.clone()));
    rig.tap(Button::A);
    rig.wait_until("the game to start", |r| r.pc.in_game());
    rig.wait_until("the game to end", |r| !r.pc.in_game());
    assert_eq!(rig.take(), [Request::Hide, Request::Show]);
    assert!(!played.borrow()[0].ok);
    rig.wait_until("the cart to come back out", at_rest);
}

#[test]
fn without_retroarch_the_cart_stays_out_and_the_shelf_says_why() {
    let mut rig = Rig::new("");
    std::fs::remove_file(
        rig.dir
            .path()
            .join("RetroArch")
            .join(slot_mini_pc::retroarch::EXE),
    )
    .unwrap();
    rig.pc.focused();
    assert!(rig.pc.retroarch.is_none());
    rig.tap(Button::A);
    assert!(at_rest(&rig));
    assert_eq!(rig.pc.app.notice(), Some("RetroArch was not found"));
    assert!(!rig.pc.in_game());
    assert!(rig.take().is_empty());
}

#[test]
fn a_game_whose_core_is_missing_says_which() {
    let mut rig = Rig::new("");
    let core = format!("mgba_libretro{}", std::env::consts::DLL_SUFFIX);
    std::fs::remove_file(rig.dir.path().join("RetroArch/cores").join(core)).unwrap();
    rig.tap(Button::A);
    assert!(at_rest(&rig));
    assert_eq!(rig.pc.app.notice(), Some("mGBA is not installed"));
}

#[test]
fn a_new_library_gets_its_folders() {
    let dir = tempfile::tempdir().unwrap();
    let library = dir.path().join("new");
    slot_mini_pc::pcapp::make_library(&library);
    for sub in ["Roms/GBA", "Roms/GBC", "Roms/GB", "Labels", "Wallpapers"] {
        assert!(Path::new(&library.join(sub)).is_dir(), "{sub}");
    }
}
