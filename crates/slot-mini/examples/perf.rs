//! Times start-up and a full carousel scroll on a fake card.
//!
//!     cargo run --release --example perf -- [games] [cold|warm]
//!
//! Prints the first-frame time (cold: label and header caches empty), the
//! average and worst frame while scrolling every cart, and memory held by
//! textures. Run it under `qemu-arm-static` for a rough armv7 figure.

use std::time::{Duration, Instant};

use slot_mini::status::FixedStatus;
use slot_mini::{App, Options, Paths};
use slot_mini_gfx::{Button, Canvas, Event, SCREEN_H, SCREEN_W};

fn main() {
    let mut args = std::env::args().skip(1);
    let games: usize = args.next().and_then(|a| a.parse().ok()).unwrap_or(200);
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::new(dir.path().join("sd"), dir.path().join("tmp"));
    slot_mini::fixture::build(&paths.sd, true).unwrap();
    std::fs::create_dir_all(&paths.tmp).unwrap();
    // Copy the fixture's GBA ROMs up to `games` carts, every 4th with a label.
    let gba = paths.sd.join("Roms/GBA");
    let src = gba.join("Metroid Fusion (USA).gba");
    let label = paths
        .sd
        .join("Labels/GBA/Pokemon - Emerald Version (USA, Europe).png");
    for i in 0..games {
        std::fs::copy(&src, gba.join(format!("Game {i:04}.gba"))).unwrap();
        if i % 4 == 0 {
            std::fs::copy(&label, paths.sd.join(format!("Labels/GBA/Game {i:04}.png"))).unwrap();
        }
    }
    {
        let sprite = slot_mini_gfx::Sprite::from_rgba(360, 212, &vec![200u8; 360 * 212 * 4]);
        let t = Instant::now();
        let _ = sprite.resized(281, 165);
        println!(
            "resize 360x212 -> 281x165: {:.1} ms",
            t.elapsed().as_secs_f64() * 1000.0
        );
        let cart =
            slot_mini_store::scan_platform(&paths.sd, slot_mini_store::Platform::Gba).unwrap();
        let t = Instant::now();
        let f = slot_mini_ui::cart_face(&cart[0]);
        println!(
            "cart face ({}x{}), first (decodes shell art): {:.1} ms",
            f.w,
            f.h,
            t.elapsed().as_secs_f64() * 1000.0
        );
        let t = Instant::now();
        let _ = slot_mini_ui::cart_face(&cart[1]);
        println!(
            "cart face with a text label: {:.1} ms",
            t.elapsed().as_secs_f64() * 1000.0
        );
        let lab = cart.iter().find(|c| c.label.is_some()).unwrap();
        let t = Instant::now();
        let art = slot_mini::label_cache::label_art(&paths.sd, lab);
        println!(
            "label art (cold): {:.1} ms",
            t.elapsed().as_secs_f64() * 1000.0
        );
        let t = Instant::now();
        let _ = slot_mini::label_cache::label_art(&paths.sd, lab);
        println!(
            "label art (cached): {:.1} ms",
            t.elapsed().as_secs_f64() * 1000.0
        );
        let t = Instant::now();
        let _ = slot_mini_ui::cart_face_with(lab, art);
        println!(
            "cart face with label: {:.1} ms",
            t.elapsed().as_secs_f64() * 1000.0
        );
    }
    for pass in ["cold", "warm"] {
        let t0 = Instant::now();
        let status = FixedStatus {
            battery: Some(80),
            wifi: true,
            clock: (12, 0),
            charging: false,
        };
        let opts = Options {
            sound: None,
            face_wait: Some(slot_mini::app::FACE_WAIT),
            seed: 1,
        };
        let mut app = App::new(paths.clone(), Box::new(status), opts, 0);
        let mut frame = Canvas::new(SCREEN_W, SCREEN_H);
        app.tick(0);
        app.render(&mut frame);
        let first = t0.elapsed();
        // Scroll the whole GBA shelf with Right held (slot's key repeat).
        let n = app.current().map(|_| games + 7).unwrap_or(0);
        app.handle(Event::Press(Button::Right));
        let mut now = 0u64;
        let mut worst = Duration::ZERO;
        let mut total = Duration::ZERO;
        let mut frames = 0u32;
        let start_stem = app.current().unwrap().1.stem.clone();
        let mut moved = 0usize;
        let mut last = start_stem.clone();
        while moved < n && frames < 20_000 {
            now += 16;
            let t = Instant::now();
            app.tick(now);
            app.render(&mut frame);
            let dt = t.elapsed();
            worst = worst.max(dt);
            total += dt;
            frames += 1;
            let here = app.current().unwrap().1.stem.clone();
            if here != last {
                moved += 1;
                last = here;
            }
        }
        app.handle(Event::Release(Button::Right));
        // Letter jumps and shelf switches: each lands where nothing is built.
        let mut jump_worst = Duration::ZERO;
        for b in [
            Button::Up,
            Button::Up,
            Button::Up,
            Button::R1,
            Button::L1,
            Button::Up,
            Button::R1,
            Button::R1,
        ] {
            app.handle(Event::Press(b));
            now += 16;
            let t = Instant::now();
            app.tick(now);
            app.render(&mut frame);
            jump_worst = jump_worst.max(t.elapsed());
            app.handle(Event::Release(b));
            for _ in 0..30 {
                now += 16;
                app.tick(now);
                app.render(&mut frame);
            }
        }
        println!(
            "{pass}: worst frame after a letter jump or shelf switch {:.1} ms",
            jump_worst.as_secs_f64() * 1000.0
        );
        println!(
            "{pass}: {games} extra carts, first frame {:.0} ms, scroll {} frames, avg {:.2} ms, worst {:.2} ms, textures {:.1} MB",
            first.as_secs_f64() * 1000.0,
            frames,
            total.as_secs_f64() * 1000.0 / frames as f64,
            worst.as_secs_f64() * 1000.0,
            app.gfx.bytes() as f64 / 1e6,
        );
    }
    studio(&paths);
    menu(&paths);
}

/// The cart studio: a whole frame redrawn, and the apply scene's curling
/// sticker drawn as software quads (slot's 8 px strips).
fn studio(paths: &Paths) {
    let status = FixedStatus {
        battery: Some(80),
        wifi: true,
        clock: (12, 0),
        charging: false,
    };
    let opts = Options {
        sound: None,
        face_wait: None,
        seed: 1,
    };
    let mut app = App::new(paths.clone(), Box::new(status), opts, 0);
    let mut frames = [
        Canvas::new(SCREEN_W, SCREEN_H),
        Canvas::new(SCREEN_W, SCREEN_H),
    ];
    let mut now = 16;
    app.tick(now);
    app.handle(Event::Press(Button::Select));
    app.handle(Event::Release(Button::Select));
    // Two canvases in turn, so every frame is drawn in full.
    let mut total = Duration::ZERO;
    for i in 0..120 {
        now += 16;
        let t = Instant::now();
        app.tick(now);
        app.render(&mut frames[i % 2]);
        if i > 0 {
            total += t.elapsed();
        }
    }
    println!(
        "studio: full frame avg {:.2} ms",
        total.as_secs_f64() * 1000.0 / 119.0
    );
    let sticker = slot_mini_gfx::Sprite::from_rgba(205, 106, &vec![180u8; 205 * 106 * 4]);
    let rect = [70.0, 160.0, 205.0, 106.0];
    let mut canvas = Canvas::new(SCREEN_W, SCREEN_H);
    for (what, flat, curl) in [
        ("flat", 1.0, 150.0),
        ("peeling", 0.5, 285.0),
        ("tossed", 0.0, 420.0),
    ] {
        let pose = slot_mini_ui::sticker_curl::Pose {
            turn_deg: -7.0,
            ..slot_mini_ui::sticker_curl::Pose::REST
        };
        let strips = slot_mini_ui::curl_runs::joined(slot_mini_ui::sticker_curl::strips(
            rect,
            flat,
            curl,
            &pose,
            [320.0, 240.0],
        ));
        let t = Instant::now();
        for _ in 0..100 {
            for s in &strips {
                canvas.blit_quad(&sticker, s.corners, s.uv, 255);
            }
        }
        println!(
            "studio: {what} sticker, {} strips, {:.2} ms",
            strips.len(),
            t.elapsed().as_secs_f64() * 1000.0 / 100.0
        );
    }
}

/// MENU while the sticker peels in, then lying flat.
fn menu(paths: &Paths) {
    let status = FixedStatus {
        battery: Some(80),
        wifi: true,
        clock: (12, 0),
        charging: false,
    };
    let opts = Options {
        sound: None,
        face_wait: None,
        seed: 1,
    };
    let mut app = App::new(paths.clone(), Box::new(status), opts, 0);
    let mut frames = [
        Canvas::new(SCREEN_W, SCREEN_H),
        Canvas::new(SCREEN_W, SCREEN_H),
    ];
    let mut now = 16;
    app.tick(now);
    app.handle(Event::Press(Button::Menu));
    app.handle(Event::Release(Button::Menu));
    // The first frame makes the sticker's face; time the rest.
    now += 16;
    app.tick(now);
    app.render(&mut frames[1]);
    let (mut peel, mut peel_worst, mut peels, mut flat, mut flats) =
        (Duration::ZERO, Duration::ZERO, 0u32, Duration::ZERO, 0u32);
    for i in 0..80 {
        now += 16;
        let t = Instant::now();
        app.tick(now);
        app.render(&mut frames[i % 2]);
        let took = t.elapsed();
        if now < 16 + slot_mini::about::PEEL_MS {
            peel += took;
            peel_worst = peel_worst.max(took);
            peels += 1;
        } else {
            flat += took;
            flats += 1;
        }
    }
    println!(
        "menu: peel-in frame avg {:.2} ms, worst {:.2} ms; flat frame avg {:.2} ms",
        peel.as_secs_f64() * 1000.0 / peels as f64,
        peel_worst.as_secs_f64() * 1000.0,
        flat.as_secs_f64() * 1000.0 / flats as f64
    );
}
