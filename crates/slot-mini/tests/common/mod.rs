#![allow(dead_code)]

use std::path::{Path, PathBuf};

use slot_mini::status::FixedStatus;
use slot_mini::{App, Options, Paths};
use slot_mini_gfx::{Button, Canvas, Event, SCREEN_H, SCREEN_W};

pub const FRAME_MS: u64 = 16;

pub struct Rig {
    pub dir: tempfile::TempDir,
    pub paths: Paths,
}

impl Rig {
    /// A fake Onion card with labels.
    pub fn new() -> Rig {
        Rig::with(true)
    }

    pub fn with(labels: bool) -> Rig {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path().join("sd"), dir.path().join("tmp"));
        slot_mini::fixture::build(&paths.sd, labels).unwrap();
        std::fs::create_dir_all(&paths.tmp).unwrap();
        // Online labels are on by default; tests stay off the network
        // unless they turn them on (tests/label_service.rs).
        std::fs::create_dir_all(paths.data()).unwrap();
        std::fs::write(paths.data().join("settings.txt"), "online_labels = off\n").unwrap();
        Rig { dir, paths }
    }

    pub fn app(&self) -> Driver {
        let status = FixedStatus {
            battery: Some(76),
            wifi: false,
            clock: (12, 0),
            charging: false,
        };
        self.app_with(status)
    }

    pub fn app_with(&self, status: FixedStatus) -> Driver {
        let opts = Options {
            sound: None,
            face_wait: None,
            seed: 7,
        };
        let app = App::new(self.paths.clone(), Box::new(status), opts, 0);
        let mut d = Driver {
            app,
            frame: Canvas::new(SCREEN_W, SCREEN_H),
            now: 0,
        };
        d.frame();
        d
    }
}

pub struct Driver {
    pub app: App,
    pub frame: Canvas,
    pub now: u64,
}

impl Driver {
    pub fn frame(&mut self) {
        self.app.tick(self.now);
        self.app.render(&mut self.frame);
    }

    /// Run frames for `ms` milliseconds.
    pub fn run(&mut self, ms: u64) {
        let end = self.now + ms;
        while self.now < end {
            self.now += FRAME_MS.min(end - self.now);
            self.frame();
        }
    }

    /// Run frames until `done`, for up to a minute of real time. Work done
    /// on threads (faces, prints, downloads) takes real time while the
    /// test's clock is simulated, and a loaded machine or qemu can take
    /// many times longer than a desktop: wait for the state, not a count.
    pub fn wait_until(&mut self, what: &str, done: impl Fn(&Driver) -> bool) {
        let start = std::time::Instant::now();
        while !done(self) {
            assert!(
                start.elapsed() < std::time::Duration::from_secs(60),
                "gave up waiting for {what}"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
            self.run(FRAME_MS);
        }
    }

    pub fn press(&mut self, b: Button) {
        self.app.handle(Event::Press(b));
    }

    pub fn release(&mut self, b: Button) {
        self.app.handle(Event::Release(b));
    }

    /// Press, one frame, release, one frame.
    pub fn tap(&mut self, b: Button) {
        self.press(b);
        self.run(FRAME_MS);
        self.release(b);
        self.run(FRAME_MS);
    }

    pub fn selected(&self) -> String {
        self.app
            .current()
            .map(|(_, c)| c.stem.clone())
            .unwrap_or_default()
    }

    /// Tap Right until `stem` is selected.
    pub fn go_to(&mut self, stem: &str) {
        for _ in 0..64 {
            if self.selected() == stem {
                return;
            }
            self.tap(Button::Right);
        }
        panic!("{stem} is not on this shelf");
    }
}

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn failures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/golden-failures")
}

/// Compare against `tests/golden/<name>.png`. With `UPDATE_GOLDEN=1` the
/// golden is (re)written instead. Small differences are allowed so that the
/// same test passes on x86 and on armv7 under qemu, whose float rounding can
/// move an anti-aliased edge by a step.
pub fn golden(name: &str, frame: &Canvas) {
    let path = golden_dir().join(format!("{name}.png"));
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        slot_mini_gfx::save_png(frame, &path).unwrap();
        return;
    }
    let Ok(file) = std::fs::File::open(&path) else {
        let out = failures_dir().join(format!("{name}.png"));
        slot_mini_gfx::save_png(frame, &out).unwrap();
        panic!(
            "no golden {}; wrote {} (run with UPDATE_GOLDEN=1)",
            path.display(),
            out.display()
        );
    };
    let decoder = png::Decoder::new(file);
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();
    assert_eq!(
        (info.width, info.height),
        (frame.w, frame.h),
        "{name}: size"
    );
    let want = &buf[..info.buffer_size()];
    let got = frame.to_rgba();
    let mut off = 0usize;
    let mut worst = 0u8;
    for (a, b) in want.chunks_exact(4).zip(got.chunks_exact(4)) {
        let d = (0..3).map(|i| a[i].abs_diff(b[i])).max().unwrap();
        worst = worst.max(d);
        if d > 24 {
            off += 1;
        }
    }
    let limit = (frame.w * frame.h) as usize / 500;
    if off > limit {
        let out = failures_dir().join(format!("{name}.png"));
        slot_mini_gfx::save_png(frame, &out).unwrap();
        panic!(
            "{name}: {off} pixels differ (limit {limit}, worst {worst}); actual written to {}",
            out.display()
        );
    }
}
