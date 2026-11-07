//! slot's boot screen for Onion: the "slot." wordmark slot puts on the
//! RG SP's boot logo, centred on its own near-black, full screen.
//!
//! Onion's runtime shows `skin/extra/bootScreen.png` while it boots, looking
//! first in the profile's theme overrides
//! (`Saves/CurrentProfile/theme/skin/extra/`), then in the theme, then in
//! `.tmp_update/res/` (`theme_getImagePath`, on 4.3.1-1 and the development
//! build alike). slot-mini writes its screen to the overrides, so it wins
//! over every theme and is left alone by Onion's updates. A screen already
//! there (the player's own) is kept in `.tmp_update/slot-mini/` and put back
//! when slot's is turned off. See docs/PLATFORM.md, "Boot screen".

use std::path::{Path, PathBuf};

use crate::paths::Paths;

/// slot's wordmark (slot `card/System/bootlogo.bmp`), 196x75, 24-bit.
const WORDMARK: &[u8] = include_bytes!("../../../assets/bootlogo.bmp");

/// Onion draws its boot screen at 640x480 (and scales it on larger panels).
pub const SIZE: (usize, usize) = (640, 480);

/// The text chunk that marks a boot screen as slot-mini's.
const KEYWORD: &str = "Software";
const MARKER: &str = "slot-mini boot screen";

/// Onion's override for the boot screen.
pub fn target(paths: &Paths) -> PathBuf {
    paths
        .sd
        .join("Saves/CurrentProfile/theme/skin/extra/bootScreen.png")
}

/// Where the player's own override is kept while slot's is in its place.
fn backup(paths: &Paths) -> PathBuf {
    paths.sysdir().join("slot-mini/bootScreen.png")
}

struct Rgb {
    w: usize,
    h: usize,
    px: Vec<[u8; 3]>,
}

fn u32_at(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(i..i + 4)?.try_into().ok()?))
}

/// An uncompressed 24-bit BMP, as slot decodes its wordmark.
fn decode_bmp(b: &[u8]) -> Option<Rgb> {
    if b.get(0..2)? != b"BM" || b.get(28..30)? != [24, 0] || u32_at(b, 30)? != 0 {
        return None;
    }
    let w = (u32_at(b, 18)? as i32).unsigned_abs() as usize;
    let raw_h = u32_at(b, 22)? as i32;
    let h = raw_h.unsigned_abs() as usize;
    let offset = u32_at(b, 10)? as usize;
    let stride = (w * 3 + 3) & !3;
    let data = b.get(offset..offset + stride * h)?;
    let mut px = Vec::with_capacity(w * h);
    for y in 0..h {
        let row = if raw_h < 0 { y } else { h - 1 - y };
        for p in data[row * stride..].chunks_exact(3).take(w) {
            px.push([p[2], p[1], p[0]]);
        }
    }
    Some(Rgb { w, h, px })
}

/// slot's boot logo at `w`x`h`: the screen filled with the wordmark's own
/// ground and the wordmark in the middle at its own size, as slot composes
/// it (`bootlogo::compose`).
fn compose(w: usize, h: usize) -> Rgb {
    let mark = decode_bmp(WORDMARK).expect("slot's wordmark decodes");
    let ground = mark.px[0];
    let mut px = vec![ground; w * h];
    let (x0, y0) = ((w - mark.w) / 2, (h - mark.h) / 2);
    for y in 0..mark.h {
        let at = (y0 + y) * w + x0;
        px[at..at + mark.w].copy_from_slice(&mark.px[y * mark.w..(y + 1) * mark.w]);
    }
    Rgb { w, h, px }
}

/// The boot screen as a PNG, marked as slot-mini's.
pub fn png() -> Vec<u8> {
    let img = compose(SIZE.0, SIZE.1);
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, img.w as u32, img.h as u32);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(png::Compression::Best);
        enc.add_text_chunk(KEYWORD.into(), MARKER.into())
            .expect("a valid text chunk");
        let mut w = enc.write_header().expect("PNG header");
        w.write_image_data(img.px.as_flattened()).expect("PNG data");
    }
    out
}

/// Whether the file is a boot screen slot-mini wrote (by its marker, so an
/// older version of the image still counts).
fn is_ours(path: &Path) -> bool {
    let Ok(b) = std::fs::read(path) else {
        return false;
    };
    let marker = MARKER.as_bytes();
    b.windows(marker.len()).any(|w| w == marker)
}

/// Whether Onion will show slot's boot screen.
pub fn installed(paths: &Paths) -> bool {
    is_ours(&target(paths))
}

fn copy_checked(from: &Path, to: &Path) -> std::io::Result<()> {
    let bytes = std::fs::read(from)?;
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir)?;
    }
    slot_mini_store::atomic_write(to, &bytes)?;
    if std::fs::read(to)? != bytes {
        return Err(std::io::Error::other(format!(
            "{} did not read back the same",
            to.display()
        )));
    }
    Ok(())
}

/// Put slot's boot screen in Onion's overrides, keeping any screen that was
/// there first. Running it again rewrites the image only if it differs.
pub fn install(paths: &Paths) -> std::io::Result<()> {
    let target = target(paths);
    if target.is_file() && !is_ours(&target) {
        copy_checked(&target, &backup(paths))?;
    }
    let image = png();
    if std::fs::read(&target).ok().as_deref() != Some(&image[..]) {
        if let Some(dir) = target.parent() {
            std::fs::create_dir_all(dir)?;
        }
        slot_mini_store::atomic_write(&target, &image)?;
    }
    Ok(())
}

/// Take slot's boot screen away: the player's own goes back if there was
/// one, else Onion's theme shows its own again. A screen that is not
/// slot-mini's is never touched.
pub fn restore(paths: &Paths) -> std::io::Result<()> {
    let target = target(paths);
    if !is_ours(&target) {
        return Ok(());
    }
    let kept = backup(paths);
    match kept.is_file() {
        true => {
            copy_checked(&kept, &target)?;
            std::fs::remove_file(&kept)?;
        }
        false => std::fs::remove_file(&target)?,
    }
    Ok(())
}

/// At start: if the player turned slot's boot screen on and it has gone
/// (Tweaks' "Reset theme overrides" empties the folder), put it back. A
/// screen someone put there since is left alone.
pub fn keep(paths: &Paths, settings: &crate::settings::Settings) {
    if settings.boot_screen && !target(paths).exists() {
        match install(paths) {
            Ok(()) => eprintln!("slot-mini: slot's boot screen put back"),
            Err(e) => eprintln!("slot-mini: boot screen not put back: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card() -> (tempfile::TempDir, Paths) {
        let d = tempfile::tempdir().unwrap();
        let paths = Paths::new(d.path().join("sd"), d.path().join("tmp"));
        (d, paths)
    }

    fn pixels(png_bytes: &[u8]) -> (u32, u32, Vec<u8>) {
        let dec = png::Decoder::new(png_bytes);
        let mut r = dec.read_info().unwrap();
        let mut buf = vec![0; r.output_buffer_size()];
        let info = r.next_frame(&mut buf).unwrap();
        buf.truncate(info.buffer_size());
        (info.width, info.height, buf)
    }

    #[test]
    fn the_screen_is_slots_wordmark_in_the_middle() {
        let (w, h, px) = pixels(&png());
        assert_eq!((w, h), (640, 480));
        let at = |x: usize, y: usize| {
            let i = (y * 640 + x) * 3;
            [px[i], px[i + 1], px[i + 2]]
        };
        // slot's ground everywhere outside the wordmark.
        let ground = [0x0b, 0x0e, 0x11];
        for (x, y) in [(0, 0), (639, 0), (0, 479), (639, 479), (100, 240)] {
            assert_eq!(at(x, y), ground, "({x}, {y})");
        }
        // The wordmark's own pixels, unscaled, centred: 196x75 at (222, 202).
        let mark = decode_bmp(WORDMARK).unwrap();
        assert_eq!((mark.w, mark.h), (196, 75));
        for (x, y) in [(0, 0), (60, 40), (195, 74), (100, 50)] {
            assert_eq!(at(222 + x, 202 + y), mark.px[y * 196 + x]);
        }
        // The letters are light.
        assert!(mark.px.iter().any(|p| p.iter().all(|&c| c > 0xe0)));
    }

    #[test]
    fn on_and_off_with_no_screen_of_the_players_own() {
        let (_d, paths) = card();
        assert!(!installed(&paths));
        install(&paths).unwrap();
        assert!(installed(&paths));
        assert_eq!(std::fs::read(target(&paths)).unwrap(), png());
        install(&paths).unwrap();
        restore(&paths).unwrap();
        assert!(!target(&paths).exists());
        assert!(!backup(&paths).exists());
        // Off when already off changes nothing.
        restore(&paths).unwrap();
    }

    #[test]
    fn the_players_own_screen_is_kept_and_comes_back() {
        let (_d, paths) = card();
        let t = target(&paths);
        std::fs::create_dir_all(t.parent().unwrap()).unwrap();
        std::fs::write(&t, b"my own boot screen").unwrap();
        install(&paths).unwrap();
        assert!(installed(&paths));
        assert_eq!(
            std::fs::read(backup(&paths)).unwrap(),
            b"my own boot screen"
        );
        // Installing again does not back slot's own screen up over it.
        install(&paths).unwrap();
        assert_eq!(
            std::fs::read(backup(&paths)).unwrap(),
            b"my own boot screen"
        );
        restore(&paths).unwrap();
        assert_eq!(std::fs::read(&t).unwrap(), b"my own boot screen");
        assert!(!backup(&paths).exists());
        // Someone else's screen is never removed.
        restore(&paths).unwrap();
        assert_eq!(std::fs::read(&t).unwrap(), b"my own boot screen");
    }

    #[test]
    fn it_comes_back_after_onions_overrides_are_reset() {
        let (_d, paths) = card();
        let mut settings = crate::settings::Settings::default();
        keep(&paths, &settings);
        assert!(!target(&paths).exists(), "off: nothing is written");
        settings.boot_screen = true;
        install(&paths).unwrap();
        // Tweaks -> Reset theme overrides: rm -rf Saves/CurrentProfile/theme/*
        std::fs::remove_dir_all(paths.sd.join("Saves/CurrentProfile/theme/skin")).unwrap();
        keep(&paths, &settings);
        assert!(installed(&paths));
        // A screen put there since is the player's choice.
        std::fs::write(target(&paths), b"another").unwrap();
        keep(&paths, &settings);
        assert_eq!(std::fs::read(target(&paths)).unwrap(), b"another");
    }
}
