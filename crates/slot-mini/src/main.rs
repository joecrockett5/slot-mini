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
    /// Run as the in-game MENU helper beside RetroArch.
    ingame: bool,
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
        ingame: false,
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
            "--ingame" => a.ingame = true,
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

/// How long the screen must be unchanged (and no button held) before the
/// loop starts sleeping on the button device.
const IDLE_AFTER: Duration = Duration::from_millis(500);
/// How long an idle loop sleeps on the button device. Anything that changes
/// without a press (the battery, a face arriving) shows within this; a press
/// wakes it at once.
const IDLE_WAIT_MS: u32 = 100;
/// After this long unchanged, wake once a second instead: the clock only
/// changes by the minute and the battery is read every two seconds.
const DEEP_IDLE_AFTER: Duration = Duration::from_secs(5);
const DEEP_IDLE_WAIT_MS: u32 = 1000;

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
    #[cfg(target_os = "linux")]
    if a.ingame {
        let fb = std::env::var("SLOT_MINI_FB").unwrap_or_else(|_| "/dev/fb0".into());
        let ev = std::env::var("SLOT_MINI_INPUT").unwrap_or_else(|_| "/dev/input/event0".into());
        slot_mini::ingame::run(&paths, fb.as_ref(), ev.as_ref());
        std::process::exit(0);
    }
    // No game is running while slot-mini is: a helper left from the last one
    // (or its flag) would keep MENU away from Onion.
    slot_mini::ingame::clear_leftovers(&paths);
    // Let the CPU slow down while the shelf idles (as Onion's runtime does
    // after a game): a governor left at performance or userspace keeps it
    // at full clock.
    if a.backend == "device" && !a.sim {
        let cpufreq = std::path::Path::new(slot_mini::cpufreq::CPUFREQ);
        if let Some(was) = slot_mini::cpufreq::settle(cpufreq) {
            eprintln!("slot-mini: CPU governor {was} -> ondemand");
        }
        if let Some(cpu) = slot_mini::cpufreq::describe(cpufreq) {
            eprintln!("slot-mini: CPU {cpu}");
        }
    }
    if let Some(v) = slot_mini::update::after_restart(&paths) {
        eprintln!("slot-mini: now running {v}");
    }
    let status: Box<dyn slot_mini::status::Status> = match a.sim {
        true => Box::new(slot_mini::status::FixedStatus {
            battery: Some(76),
            wifi: false,
            clock: (12, 0),
            charging: false,
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
    // On the device an unchanged screen is neither drawn nor shown, and once
    // it has been still for a while (no button held) the loop sleeps on the
    // button device instead of running at 60 fps: an idle shelf otherwise
    // keeps the CPU busy and the handheld warm.
    let pace = a.backend == "device" && !a.sim;
    let mut quiet_since = Instant::now();
    let mut held = 0usize;
    loop {
        let t = Instant::now();
        let quiet = quiet_since.elapsed();
        // Held buttons from the device itself where it can say: a counted
        // press whose release went missing must not keep the loop at 60 fps.
        let idle = pace && quiet >= IDLE_AFTER && !out.held().unwrap_or(held > 0);
        let mut events = match idle {
            true if quiet >= DEEP_IDLE_AFTER => out.wait(DEEP_IDLE_WAIT_MS),
            true => out.wait(IDLE_WAIT_MS),
            false => out.poll(),
        };
        let now = match a.sim {
            true => n * 16,
            false => started.elapsed().as_millis() as u64,
        };
        while script.peek().is_some_and(|(at, _)| *at <= now) {
            events.push(script.next().unwrap().1);
        }
        if STOP.load(Ordering::Acquire) {
            events.push(Event::Quit);
        }
        if !events.is_empty() {
            quiet_since = Instant::now();
        }
        for ev in events {
            match ev {
                Event::Press(_) => held += 1,
                Event::Release(_) => held = held.saturating_sub(1),
                Event::Quit => {}
            }
            app.handle(ev);
        }
        app.tick(now);
        let changed = app.render(&mut frame);
        if changed || !pace || n == 0 {
            out.present(&frame);
            quiet_since = Instant::now();
        }
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
        // The desktop window paces itself; everything else sleeps to 60 fps
        // (an idle device already waited for input above).
        if !a.sim && a.backend != "desktop" && !idle {
            if let Some(left) = frame_time.checked_sub(t.elapsed()) {
                std::thread::sleep(left);
            }
        }
    }
    // Leave the screen on the framebuffer page Onion's programs expect.
    out.close(&frame);
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
                    // slot's in-game MENU, on the device only.
                    if app.settings.ingame_menu && a.backend == "device" && !a.sim {
                        slot_mini::ingame::spawn(&paths);
                    }
                    0
                }
                Err(e) => {
                    eprintln!("slot-mini: launch failed: {e}");
                    1
                }
            }
        }
        Some(Exit::Restart) => {
            save(&app, &paths);
            restart(&paths)
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

/// Replace this process with the newly installed binary, same arguments.
/// Only returns (with an exit code) if that fails.
fn restart(paths: &Paths) -> i32 {
    use std::os::unix::process::CommandExt;
    let bin = paths.app().join("slot-mini");
    let err = std::process::Command::new(&bin)
        .args(std::env::args_os().skip(1))
        .exec();
    eprintln!("slot-mini: restart {}: {err}", bin.display());
    0
}

fn save(app: &App, paths: &Paths) {
    if let Err(e) = app.state().write(paths) {
        eprintln!("slot-mini: state: {e}");
    }
}
