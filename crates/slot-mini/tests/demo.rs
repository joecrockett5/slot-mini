//! Records the README's demo: the app driven headlessly through a short
//! tour on a card of homebrew games, every other frame written as raw RGBA
//! for `scripts/record-demo.sh` to turn into a GIF, and a still of the
//! shelf for the banner (`scripts/banner.py`).
//!
//! Ignored by default; run it with the script.

mod common;

use std::io::Write;
use std::path::{Path, PathBuf};

use common::{Driver, Rig, FRAME_MS};
use slot_mini_gfx::Button;
use slot_mini_store::Platform;

/// Free homebrew, so the demo shows no one else's games.
/// (platform, file name, header title, CGB flag, shell)
const CARD: &[(Platform, &str, &str, u8, Option<&str>)] = &[
    (
        Platform::Gba,
        "Anguna.gba",
        "ANGUNA",
        0,
        Some("auto 3a7d44 clear"),
    ),
    (Platform::Gba, "Apotris.gba", "APOTRIS", 0, None),
    (
        Platform::Gba,
        "Butano Fighter.gba",
        "BTNFIGHTER",
        0,
        Some("auto c23b22 solid"),
    ),
    (
        Platform::Gba,
        "Celeste Classic.gba",
        "CELESTE",
        0,
        Some("auto 7b4fb0 glitter"),
    ),
    (
        Platform::Gba,
        "Goodboy Galaxy.gba",
        "GOODBOY",
        0,
        Some("auto f2b134 solid"),
    ),
    (
        Platform::Gba,
        "Varooom 3D.gba",
        "VAROOOM3D",
        0,
        Some("auto 2f5cc0 clear"),
    ),
    (
        Platform::Gbc,
        "Dangan GB.gbc",
        "DANGAN",
        0xC0,
        Some("auto d8442c solid"),
    ),
    (
        Platform::Gbc,
        "Infinity.gbc",
        "INFINITY",
        0xC0,
        Some("auto 1c8c8c clear"),
    ),
    (
        Platform::Gbc,
        "Shock Lobster.gbc",
        "SHOCKLOBSTR",
        0xC0,
        Some("auto e8743b glitter"),
    ),
    (
        Platform::Gbc,
        "Tobu Tobu Girl Deluxe.gbc",
        "TOBUDX",
        0xC0,
        None,
    ),
    (
        Platform::Gb,
        "Deadeus.gb",
        "DEADEUS",
        0,
        Some("rounded 4a4a4a solid"),
    ),
    (
        Platform::Gb,
        "Libbet and the Magic Floor.gb",
        "LIBBET",
        0,
        None,
    ),
    (
        Platform::Gb,
        "Petris.gb",
        "PETRIS",
        0,
        Some("notched 2f5cc0 glitter"),
    ),
    (
        Platform::Gb,
        "Tobu Tobu Girl.gb",
        "TOBUTOBU",
        0,
        Some("notched 6aa84f solid"),
    ),
];

fn rom(platform: Platform, title: &str, cgb: u8) -> Vec<u8> {
    let mut rom = vec![0u8; 0x8000];
    if platform == Platform::Gba {
        rom[0xa0..0xa0 + title.len()].copy_from_slice(title.as_bytes());
        rom[0xb2] = 0x96;
    } else {
        rom[0x134..0x134 + title.len()].copy_from_slice(title.as_bytes());
        rom[0x143] = cgb;
        rom[0x14a] = 1;
    }
    rom
}

/// The fixture's card with its games swapped for the homebrew above.
fn demo_card(sd: &Path) {
    for p in [Platform::Gba, Platform::Gbc, Platform::Gb] {
        let dir = sd.join("Roms").join(p.dir_name());
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_file() {
                std::fs::remove_file(path).unwrap();
            }
        }
    }
    let mut shells = String::new();
    for (p, name, title, cgb, shell) in CARD {
        let path = sd.join("Roms").join(p.dir_name()).join(name);
        std::fs::write(path, rom(*p, title, *cgb)).unwrap();
        if let Some(shell) = shell {
            let stem = name.rsplit_once('.').unwrap().0;
            shells.push_str(&format!("{stem} = {shell}\n"));
        }
    }
    let labels = sd.join("Labels/GBA");
    std::fs::create_dir_all(&labels).unwrap();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/fetch_example_logo.png"),
        labels.join("Apotris.png"),
    )
    .unwrap();
    std::fs::write(sd.join("Labels/cart_shell.txt"), shells).unwrap();
}

/// Keeps every other frame (about 31 fps).
struct Film {
    out: std::io::BufWriter<std::fs::File>,
    tick: u64,
    frames: u64,
}

impl Film {
    /// Run `ms` of frames, filming them. Each frame also gives the app's
    /// threads (label faces) a moment of real time, as a device would.
    fn run(&mut self, d: &mut Driver, ms: u64) {
        let end = d.now + ms;
        while d.now < end {
            std::thread::sleep(std::time::Duration::from_millis(3));
            d.run(FRAME_MS.min(end - d.now));
            self.tick += 1;
            if self.tick.is_multiple_of(2) {
                self.out.write_all(&d.frame.to_rgba()).unwrap();
                self.frames += 1;
            }
        }
    }

    fn tap(&mut self, d: &mut Driver, b: Button, then_ms: u64) {
        d.press(b);
        self.run(d, FRAME_MS * 2);
        d.release(b);
        self.run(d, then_ms);
    }
}

#[test]
#[ignore = "records the README demo; run scripts/record-demo.sh"]
fn record_demo() {
    let out: PathBuf = std::env::var_os("DEMO_OUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/demo/frames.rgba")
        });
    std::fs::create_dir_all(out.parent().unwrap()).unwrap();
    let rig = Rig::with(false);
    demo_card(&rig.paths.sd);
    let mut d = rig.app();
    // Let the label art load before the camera rolls.
    d.wait_until("the shelf", |d| d.selected() == "Anguna");
    for _ in 0..40 {
        std::thread::sleep(std::time::Duration::from_millis(10));
        d.run(FRAME_MS);
    }
    let mut film = Film {
        out: std::io::BufWriter::new(std::fs::File::create(&out).unwrap()),
        tick: 0,
        frames: 0,
    };
    let f = &mut film;
    // Browse the GBA shelf.
    f.run(&mut d, 1200);
    for _ in 0..3 {
        f.tap(&mut d, Button::Right, 650);
    }
    f.tap(&mut d, Button::Left, 650);
    f.tap(&mut d, Button::Left, 1000);
    // Over to Game Boy Color and Game Boy, and back.
    f.tap(&mut d, Button::R1, 1100);
    f.tap(&mut d, Button::Right, 650);
    f.tap(&mut d, Button::Right, 900);
    f.tap(&mut d, Button::R1, 1100);
    f.tap(&mut d, Button::Right, 650);
    f.tap(&mut d, Button::Right, 900);
    f.tap(&mut d, Button::R1, 1100);
    // Heart Apotris: it gets its own Favorites shelf.
    assert_eq!(d.selected(), "Apotris");
    f.tap(&mut d, Button::Y, 1400);
    // The README banner's screenshot.
    slot_mini_gfx::save_png(&d.frame, &out.with_file_name("shelf.png")).unwrap();
    // The cart studio: try a few shell colours on Butano Fighter.
    f.tap(&mut d, Button::Right, 800);
    f.tap(&mut d, Button::Select, 900);
    f.tap(&mut d, Button::Down, 400);
    f.tap(&mut d, Button::Down, 400);
    f.tap(&mut d, Button::A, 700);
    for _ in 0..3 {
        f.tap(&mut d, Button::Down, 350);
    }
    for _ in 0..4 {
        f.tap(&mut d, Button::Right, 450);
    }
    f.tap(&mut d, Button::Down, 450);
    f.tap(&mut d, Button::Down, 600);
    f.tap(&mut d, Button::B, 500);
    f.tap(&mut d, Button::B, 1000);
    // MENU: settings and updates.
    f.tap(&mut d, Button::Menu, 1800);
    f.tap(&mut d, Button::B, 900);
    // Back to Apotris and play it.
    f.tap(&mut d, Button::Left, 1000);
    assert_eq!(d.selected(), "Apotris");
    f.tap(&mut d, Button::A, 0);
    let start = d.now;
    while d.app.exit.is_none() && d.now - start < 5000 {
        f.run(&mut d, FRAME_MS);
    }
    assert!(d.app.exit.is_some(), "the cart went in");
    f.run(&mut d, 300);
    film.out.flush().unwrap();
    println!("DEMO_FRAMES={} {}", film.frames, out.display());
}
