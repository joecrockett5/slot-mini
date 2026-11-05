use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use slot_mini::launch::{self, Mode};
use slot_mini::status::OnionStatus;
use slot_mini::{App, Exit, Options, Paths};
use slot_mini_gfx::{Backend, Button, Canvas, Event, Headless, SCREEN_H, SCREEN_W};

/// Exit code that tells the boot shim to start Onion's own MainUI.
const EXIT_TO_ONION: i32 = 10;

struct Args {
    sd: PathBuf,
    tmp: PathBuf,
    mode: Mode,
    backend: String,
    shot: Option<PathBuf>,
    frames: Option<u64>,
    /// Scripted input for the headless backend: `ms:press|release:BUTTON,...`
    input: Vec<(u64, Event)>,
    /// Advance a simulated clock 16 ms per frame instead of real time, and
    /// show a fixed battery and clock (for tests and screenshots).
    sim: bool,
}

fn button(name: &str) -> Option<Button> {
    Some(match name.to_ascii_uppercase().as_str() {
        "UP" => Button::Up,
        "DOWN" => Button::Down,
        "LEFT" => Button::Left,
        "RIGHT" => Button::Right,
        "A" => Button::A,
        "B" => Button::B,
        "X" => Button::X,
        "Y" => Button::Y,
        "L1" => Button::L1,
        "R1" => Button::R1,
        "L2" => Button::L2,
        "R2" => Button::R2,
        "SELECT" => Button::Select,
        "START" => Button::Start,
        "MENU" => Button::Menu,
        _ => return None,
    })
}

fn parse_input(s: &str) -> Vec<(u64, Event)> {
    s.split(',')
        .filter(|e| !e.trim().is_empty())
        .filter_map(|e| {
            let mut it = e.trim().split(':');
            let at = it.next()?.parse().ok()?;
            let kind = it.next()?;
            let b = button(it.next()?)?;
            let ev = match kind {
                "press" => Event::Press(b),
                "release" => Event::Release(b),
                _ => return None,
            };
            Some((at, ev))
        })
        .collect()
}

const USAGE: &str = "usage: slot-mini [--sd DIR] [--tmp DIR] [--mode app|launcher]
                 [--backend headless|desktop|device] [--frames N] [--shot out.png]
                 [--sim] [--input 'MS:press|release:BUTTON,...']
       slot-mini --make-fake-sd DIR
       slot-mini [--sd DIR] --install-launcher | --restore-launcher";

fn args() -> Args {
    let mut a = Args {
        sd: PathBuf::from("/mnt/SDCARD"),
        tmp: PathBuf::from("/tmp"),
        mode: Mode::App,
        backend: default_backend().into(),
        shot: None,
        frames: None,
        input: Vec::new(),
        sim: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(k) = it.next() {
        let mut v = || it.next().unwrap_or_default();
        match k.as_str() {
            "--sd" => a.sd = v().into(),
            "--tmp" => a.tmp = v().into(),
            "--mode" => {
                a.mode = match v().as_str() {
                    "launcher" => Mode::Launcher,
                    _ => Mode::App,
                }
            }
            "--backend" => a.backend = v(),
            "--shot" => a.shot = Some(v().into()),
            "--frames" => a.frames = v().parse().ok(),
            "--input" => a.input = parse_input(&v()),
            "--sim" => a.sim = true,
            "--make-fake-sd" => {
                let dir = PathBuf::from(v());
                match slot_mini::fixture::build(&dir, true) {
                    Ok(()) => std::process::exit(0),
                    Err(e) => {
                        eprintln!("slot-mini: {}: {e}", dir.display());
                        std::process::exit(1);
                    }
                }
            }
            "--install-launcher" | "--restore-launcher" => {
                let paths = Paths::new(a.sd.clone(), a.tmp.clone());
                let done = match k.as_str() {
                    "--install-launcher" => slot_mini::switch::install(&paths),
                    _ => slot_mini::switch::restore(&paths),
                };
                match done {
                    Ok(()) => std::process::exit(0),
                    Err(e) => {
                        eprintln!("slot-mini: {e}");
                        std::process::exit(1);
                    }
                }
            }
            "--version" => {
                println!("slot-mini {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            _ => {
                eprintln!("{USAGE}");
                std::process::exit(2);
            }
        }
    }
    a
}

fn default_backend() -> &'static str {
    if cfg!(feature = "desktop") {
        "desktop"
    } else if cfg!(all(target_os = "linux", target_arch = "arm")) {
        "device"
    } else {
        "headless"
    }
}

fn backend(name: &str) -> Box<dyn Backend> {
    match name {
        #[cfg(feature = "desktop")]
        "desktop" => {
            match slot_mini_gfx::backend::desktop::Desktop::new("slot-mini", SCREEN_W, SCREEN_H) {
                Ok(d) => Box::new(d),
                Err(e) => {
                    eprintln!("slot-mini: desktop window: {e}");
                    std::process::exit(1);
                }
            }
        }
        #[cfg(target_os = "linux")]
        "device" => {
            let fb = std::env::var("SLOT_MINI_FB").unwrap_or_else(|_| "/dev/fb0".into());
            let ev =
                std::env::var("SLOT_MINI_INPUT").unwrap_or_else(|_| "/dev/input/event0".into());
            match slot_mini_gfx::backend::Device::open(fb.as_ref(), ev.as_ref()) {
                Ok(d) => Box::new(d),
                Err(e) => {
                    // The boot shim falls back to Onion's MainUI on this.
                    eprintln!("slot-mini: {fb}: {e}");
                    std::process::exit(1);
                }
            }
        }
        _ => Box::new(Headless::new(SCREEN_W, SCREEN_H)),
    }
}

static STOP: AtomicBool = AtomicBool::new(false);

extern "C" fn on_signal(_: libc::c_int) {
    STOP.store(true, Ordering::Release);
}

fn main() {
    let a = args();
    let started = Instant::now();
    // SAFETY: the handler only stores to an atomic.
    unsafe {
        libc::signal(libc::SIGTERM, on_signal as *const () as libc::sighandler_t);
        libc::signal(libc::SIGINT, on_signal as *const () as libc::sighandler_t);
    }
    let paths = Paths::new(a.sd, a.tmp);
    let status: Box<dyn slot_mini::status::Status> = match a.sim {
        true => Box::new(slot_mini::status::FixedStatus {
            battery: Some(76),
            wifi: false,
            clock: (12, 0),
        }),
        false => Box::new(OnionStatus::new(&paths)),
    };
    let seed = match a.sim {
        true => 7,
        false => std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as u64),
    };
    let opts = Options {
        sound: None,
        seed,
        face_wait: match a.sim {
            true => None,
            false => Options::default().face_wait,
        },
    };
    let mut app = App::new(paths.clone(), status, opts, 0);
    let mut out = backend(&a.backend);
    let mut frame = Canvas::new(SCREEN_W, SCREEN_H);
    let mut script = a.input.clone();
    script.sort_by_key(|(t, _)| *t);
    let mut script = script.into_iter().peekable();
    let mut n = 0u64;
    let frame_time = Duration::from_micros(16_667);
    loop {
        let t = Instant::now();
        let now = match a.sim {
            true => n * 16,
            false => started.elapsed().as_millis() as u64,
        };
        let mut events = out.poll();
        while script.peek().is_some_and(|(at, _)| *at <= now) {
            events.push(script.next().unwrap().1);
        }
        if STOP.load(Ordering::Acquire) {
            events.push(Event::Quit);
        }
        for ev in events {
            app.handle(ev);
        }
        app.tick(now);
        app.render(&mut frame);
        out.present(&frame);
        if n == 0 {
            eprintln!(
                "slot-mini: first frame at {} ms",
                started.elapsed().as_millis()
            );
        }
        n += 1;
        if app.exit.is_some() || a.frames.is_some_and(|f| n >= f) {
            break;
        }
        // The desktop window paces itself; everything else sleeps to 60 fps.
        if !a.sim && a.backend != "desktop" {
            if let Some(left) = frame_time.checked_sub(t.elapsed()) {
                std::thread::sleep(left);
            }
        }
    }
    if let Some(p) = &a.shot {
        if let Err(e) = slot_mini_gfx::save_png(&frame, p) {
            eprintln!("slot-mini: {}: {e}", p.display());
        }
    }
    if std::env::var_os("SLOT_MINI_REPORT_RSS").is_some() {
        let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
        for line in status
            .lines()
            .filter(|l| l.starts_with("VmHWM") || l.starts_with("VmRSS"))
        {
            eprintln!("slot-mini: {line}");
        }
        eprintln!("slot-mini: textures {:.1} MB", app.gfx.bytes() as f64 / 1e6);
    }
    let code = match app.exit.take() {
        Some(Exit::Launch { cart, fresh }) => {
            match launch::hand_off(&paths, a.mode, &app.shelf_key(), &cart, fresh) {
                Ok(cmd) => {
                    eprintln!("slot-mini: launching: {}", cmd.trim_end());
                    0
                }
                Err(e) => {
                    eprintln!("slot-mini: launch failed: {e}");
                    1
                }
            }
        }
        Some(Exit::ToOnion) if a.mode == Mode::Launcher => {
            save(&app, &paths);
            EXIT_TO_ONION
        }
        _ => {
            save(&app, &paths);
            0
        }
    };
    std::process::exit(code);
}

fn save(app: &App, paths: &Paths) {
    if let Err(e) = app.state().write(paths) {
        eprintln!("slot-mini: state: {e}");
    }
}
