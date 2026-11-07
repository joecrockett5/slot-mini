//! slot's boot and shutdown screens for Onion. At boot, the "slot." wordmark
//! slot puts on the RG SP's boot logo, centred on its own near-black. At
//! power off, slot's own shutdown screen: "Powering Down" in slot's menu
//! type, centred on black.
//!
//! Onion's runtime shows `skin/extra/bootScreen.png` while it boots, and
//! `skin/extra/Screen_Off.png` as it powers off (`Screen_Off_Save.png` when
//! it powers off from a game). It looks for each first in the profile's
//! theme overrides (`Saves/CurrentProfile/theme/skin/extra/`), then in the
//! theme, then in `.tmp_update/res/` (`theme_getImagePath`, on 4.3.1-1 and
//! the development build alike). slot-mini writes its screens to the
//! overrides, so they win over every theme and are left alone by Onion's
//! updates. A screen already there (the player's own) is kept in
//! `.tmp_update/slot-mini/` and put back when slot's are turned off. See
//! docs/PLATFORM.md, "Boot and shutdown screens".

use std::path::{Path, PathBuf};

use crate::paths::Paths;

/// slot's wordmark (slot `card/System/bootlogo.bmp`), 196x75, 24-bit.
const WORDMARK: &[u8] = include_bytes!("../../../assets/bootlogo.bmp");

/// Onion draws these screens at 640x480 (and scales them on larger panels).
pub const SIZE: (usize, usize) = (640, 480);

/// The text chunk that marks a screen as slot-mini's.
const KEYWORD: &str = "Software";
const MARKER: &str = "slot-mini boot screen";

/// What slot says while it powers off (slot `frontend.rs`, `PowerChoice`).
const POWERING_DOWN: &str = "Powering Down";

/// The screens slot-mini replaces, all under the one setting.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Screen {
    /// `bootScreen Boot`: while Onion starts.
    Boot,
    /// `bootScreen End`: powering off from Onion's menu or an app.
    Off,
    /// `bootScreen End_Save`: powering off from a game.
    OffSave,
}

impl Screen {
    pub const ALL: [Screen; 3] = [Screen::Boot, Screen::Off, Screen::OffSave];

    fn file(self) -> &'static str {
        match self {
            Screen::Boot => "bootScreen.png",
            Screen::Off => "Screen_Off.png",
            Screen::OffSave => "Screen_Off_Save.png",
        }
    }

    /// The screen as a PNG, marked as slot-mini's. Both shutdown screens are
    /// the same: slot says "Powering Down" from a game too, its save already
    /// made.
    pub fn png(self) -> Vec<u8> {
        let img = match self {
            Screen::Boot => compose(SIZE.0, SIZE.1),
            Screen::Off | Screen::OffSave => powering_down(SIZE.0, SIZE.1),
        };
        encode(&img)
    }
}

/// Onion's override for a screen.
pub fn target(paths: &Paths, screen: Screen) -> PathBuf {
    paths
        .sd
        .join("Saves/CurrentProfile/theme/skin/extra")
        .join(screen.file())
}

/// Where the player's own override is kept while slot's is in its place.
fn backup(paths: &Paths, screen: Screen) -> PathBuf {
    paths.sysdir().join("slot-mini").join(screen.file())
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

/// slot's shutdown screen at `w`x`h`: black, and "Powering Down" in slot's
/// menu type in the middle, placed as slot places it (`draw_app` while
/// shutting down).
fn powering_down(w: usize, h: usize) -> Rgb {
    let mut px = vec![[0u8; 3]; w * h];
    let face = slot_mini_ui::quick::menu_face(POWERING_DOWN);
    let (fw, fh) = (face.w as usize, face.h as usize);
    let (x0, y0) = ((w.saturating_sub(fw)) / 2, (h.saturating_sub(fh)) / 2);
    for y in 0..fh.min(h) {
        for x in 0..fw.min(w) {
            let s = &face.rgba[(y * fw + x) * 4..][..4];
            let a = s[3] as u32;
            px[(y0 + y) * w + x0 + x] = [0, 1, 2].map(|c| ((s[c] as u32 * a + 127) / 255) as u8);
        }
    }
    Rgb { w, h, px }
}

fn encode(img: &Rgb) -> Vec<u8> {
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

/// Whether the file is a screen slot-mini wrote (by its marker, so an older
/// version of the image still counts).
fn is_ours(path: &Path) -> bool {
    let Ok(b) = std::fs::read(path) else {
        return false;
    };
    let marker = MARKER.as_bytes();
    b.windows(marker.len()).any(|w| w == marker)
}

/// Whether Onion will show any of slot's screens.
pub fn installed(paths: &Paths) -> bool {
    Screen::ALL.iter().any(|&s| is_ours(&target(paths, s)))
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

/// Put one of slot's screens in Onion's overrides, keeping any screen that
/// was there first. Running it again rewrites the image only if it differs.
fn install_one(paths: &Paths, screen: Screen) -> std::io::Result<()> {
    let target = target(paths, screen);
    if target.is_file() && !is_ours(&target) {
        copy_checked(&target, &backup(paths, screen))?;
    }
    let image = screen.png();
    if std::fs::read(&target).ok().as_deref() != Some(&image[..]) {
        if let Some(dir) = target.parent() {
            std::fs::create_dir_all(dir)?;
        }
        slot_mini_store::atomic_write(&target, &image)?;
    }
    Ok(())
}

/// Put slot's boot and shutdown screens in Onion's overrides.
pub fn install(paths: &Paths) -> std::io::Result<()> {
    Screen::ALL.iter().try_for_each(|&s| install_one(paths, s))
}

/// Take one of slot's screens away: the player's own goes back if there was
/// one, else Onion's theme shows its own again. A screen that is not
/// slot-mini's is never touched.
fn restore_one(paths: &Paths, screen: Screen) -> std::io::Result<()> {
    let target = target(paths, screen);
    if !is_ours(&target) {
        return Ok(());
    }
    let kept = backup(paths, screen);
    match kept.is_file() {
        true => {
            copy_checked(&kept, &target)?;
            std::fs::remove_file(&kept)?;
        }
        false => std::fs::remove_file(&target)?,
    }
    Ok(())
}

/// Take slot's boot and shutdown screens away. Every screen is tried even
/// if one fails; the first error is reported.
pub fn restore(paths: &Paths) -> std::io::Result<()> {
    let mut result = Ok(());
    for s in Screen::ALL {
        let r = restore_one(paths, s);
        if result.is_ok() {
            result = r;
        }
    }
    result
}

/// At start: if the player turned slot's screens on and one has gone
/// (Tweaks' "Reset theme overrides" empties the folder), or is new in this
/// version, put it in. A screen someone put there since is left alone.
pub fn keep(paths: &Paths, settings: &crate::settings::Settings) {
    if !settings.boot_screen {
        return;
    }
    for s in Screen::ALL {
        if target(paths, s).exists() {
            continue;
        }
        match install_one(paths, s) {
            Ok(()) => eprintln!("slot-mini: slot's {} put back", s.file()),
            Err(e) => eprintln!("slot-mini: {} not put back: {e}", s.file()),
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

    fn all_ours(paths: &Paths) -> bool {
        Screen::ALL.iter().all(|&s| is_ours(&target(paths, s)))
    }

    #[test]
    fn the_screen_is_slots_wordmark_in_the_middle() {
        let (w, h, px) = pixels(&Screen::Boot.png());
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
    fn the_shutdown_screen_is_slots_powering_down_on_black() {
        let image = Screen::Off.png();
        assert_eq!(image, Screen::OffSave.png());
        assert!(is_ours_bytes(&image));
        let (w, h, px) = pixels(&image);
        assert_eq!((w, h), (640, 480));
        let lit: Vec<(usize, usize)> = (0..640 * 480)
            .filter(|i| px[i * 3..i * 3 + 3].iter().any(|&c| c > 0))
            .map(|i| (i % 640, i / 640))
            .collect();
        assert!(!lit.is_empty(), "the words are drawn");
        // Black everywhere but one line of type across the middle.
        let (x0, x1) = (
            lit.iter().map(|p| p.0).min().unwrap(),
            lit.iter().map(|p| p.0).max().unwrap(),
        );
        let (y0, y1) = (
            lit.iter().map(|p| p.1).min().unwrap(),
            lit.iter().map(|p| p.1).max().unwrap(),
        );
        assert!(y0 >= 220 && y1 <= 260, "rows {y0}..{y1}");
        assert!(((x0 + x1) as i32 - 639).abs() <= 4, "centred: {x0}..{x1}");
        assert!(x1 - x0 > 150, "a line of words: {x0}..{x1}");
        // At full strength the ink is slot's menu ink.
        let ink = slot_mini_ui::quick::MENU_INK;
        assert!((0..640 * 480).any(|i| px[i * 3..i * 3 + 3] == ink));
    }

    fn is_ours_bytes(b: &[u8]) -> bool {
        b.windows(MARKER.len()).any(|w| w == MARKER.as_bytes())
    }

    #[test]
    fn on_and_off_with_no_screens_of_the_players_own() {
        let (_d, paths) = card();
        assert!(!installed(&paths));
        install(&paths).unwrap();
        assert!(installed(&paths) && all_ours(&paths));
        for s in Screen::ALL {
            assert_eq!(std::fs::read(target(&paths, s)).unwrap(), s.png());
        }
        install(&paths).unwrap();
        restore(&paths).unwrap();
        for s in Screen::ALL {
            assert!(!target(&paths, s).exists(), "{s:?}");
            assert!(!backup(&paths, s).exists(), "{s:?}");
        }
        assert!(!installed(&paths));
        // Off when already off changes nothing.
        restore(&paths).unwrap();
    }

    #[test]
    fn the_players_own_screens_are_kept_and_come_back() {
        let (_d, paths) = card();
        let boot = target(&paths, Screen::Boot);
        let off = target(&paths, Screen::Off);
        std::fs::create_dir_all(boot.parent().unwrap()).unwrap();
        std::fs::write(&boot, b"my own boot screen").unwrap();
        std::fs::write(&off, b"my own off screen").unwrap();
        install(&paths).unwrap();
        assert!(all_ours(&paths));
        let kept = |s| std::fs::read(backup(&paths, s)).unwrap();
        assert_eq!(kept(Screen::Boot), b"my own boot screen");
        assert_eq!(kept(Screen::Off), b"my own off screen");
        assert!(!backup(&paths, Screen::OffSave).exists());
        // Installing again does not back slot's own screens up over them.
        install(&paths).unwrap();
        assert_eq!(kept(Screen::Boot), b"my own boot screen");
        assert_eq!(kept(Screen::Off), b"my own off screen");
        restore(&paths).unwrap();
        assert_eq!(std::fs::read(&boot).unwrap(), b"my own boot screen");
        assert_eq!(std::fs::read(&off).unwrap(), b"my own off screen");
        assert!(!target(&paths, Screen::OffSave).exists());
        for s in Screen::ALL {
            assert!(!backup(&paths, s).exists(), "{s:?}");
        }
        // Someone else's screens are never removed.
        restore(&paths).unwrap();
        assert_eq!(std::fs::read(&boot).unwrap(), b"my own boot screen");
        assert_eq!(std::fs::read(&off).unwrap(), b"my own off screen");
    }

    #[test]
    fn they_come_back_after_onions_overrides_are_reset() {
        let (_d, paths) = card();
        let mut settings = crate::settings::Settings::default();
        keep(&paths, &settings);
        assert!(
            !target(&paths, Screen::Boot).exists(),
            "off: nothing is written"
        );
        settings.boot_screen = true;
        install(&paths).unwrap();
        // Tweaks -> Reset theme overrides: rm -rf Saves/CurrentProfile/theme/*
        std::fs::remove_dir_all(paths.sd.join("Saves/CurrentProfile/theme/skin")).unwrap();
        keep(&paths, &settings);
        assert!(all_ours(&paths));
        // A screen put there since is the player's choice.
        std::fs::write(target(&paths, Screen::Boot), b"another").unwrap();
        keep(&paths, &settings);
        assert_eq!(
            std::fs::read(target(&paths, Screen::Boot)).unwrap(),
            b"another"
        );
    }

    #[test]
    fn turned_on_before_the_shutdown_screens_existed_they_are_added() {
        // 0.5.1 wrote only the boot screen.
        let (_d, paths) = card();
        let settings = crate::settings::Settings {
            boot_screen: true,
            ..Default::default()
        };
        install_one(&paths, Screen::Boot).unwrap();
        keep(&paths, &settings);
        assert!(all_ours(&paths));
    }
}
