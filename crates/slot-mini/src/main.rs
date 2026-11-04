use std::path::PathBuf;
use std::time::{Duration, Instant};

use slot_mini::status::OnionStatus;
use slot_mini::{App, Exit, Options, Paths};
use slot_mini_gfx::{Backend, Canvas, Headless, SCREEN_H, SCREEN_W};

struct Args {
    sd: PathBuf,
    tmp: PathBuf,
    backend: String,
    shot: Option<PathBuf>,
    frames: Option<u64>,
}

fn args() -> Args {
    let mut a = Args {
        sd: PathBuf::from("/mnt/SDCARD"),
        tmp: PathBuf::from("/tmp"),
        backend: default_backend().into(),
        shot: None,
        frames: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(k) = it.next() {
        let mut v = || it.next().unwrap_or_default();
        match k.as_str() {
            "--sd" => a.sd = v().into(),
            "--tmp" => a.tmp = v().into(),
            "--backend" => a.backend = v(),
            "--shot" => a.shot = Some(v().into()),
            "--frames" => a.frames = v().parse().ok(),
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
            "--version" => {
                println!("slot-mini {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            _ => {
                eprintln!("usage: slot-mini [--sd DIR] [--tmp DIR] [--backend headless|desktop] [--frames N] [--shot out.png]");
                std::process::exit(2);
            }
        }
    }
    a
}

fn default_backend() -> &'static str {
    if cfg!(feature = "desktop") {
        "desktop"
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
        _ => Box::new(Headless::new(SCREEN_W, SCREEN_H)),
    }
}

fn main() {
    let a = args();
    let started = Instant::now();
    let paths = Paths::new(a.sd, a.tmp);
    let status = Box::new(OnionStatus::new(&paths));
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64);
    let opts = Options {
        seed,
        ..Options::default()
    };
    let mut app = App::new(paths, status, opts, 0);
    let mut out = backend(&a.backend);
    let mut frame = Canvas::new(SCREEN_W, SCREEN_H);
    let mut n = 0u64;
    let frame_time = Duration::from_micros(16_667);
    loop {
        let t = Instant::now();
        for ev in out.poll() {
            app.handle(ev);
        }
        app.tick(started.elapsed().as_millis() as u64);
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
        if let Some(left) = frame_time.checked_sub(t.elapsed()) {
            if a.backend == "headless" {
                std::thread::sleep(left);
            }
        }
    }
    if let Some(p) = &a.shot {
        if let Err(e) = slot_mini_gfx::save_png(&frame, p) {
            eprintln!("slot-mini: {}: {e}", p.display());
        }
    }
    let code = match app.exit {
        Some(Exit::ToOnion) => 10,
        _ => 0,
    };
    std::process::exit(code);
}
