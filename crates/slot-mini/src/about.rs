//! MENU: about slot-mini, its few settings, and the way back to Onion.
//! Drawn on slot's menu ground: the wallpaper under slot's scrim, slot's
//! About sticker at the top (peeling in as MENU opens), then rows in slot's
//! title type, selected row bright and the rest dimmed as in slot's quick
//! menu.

use slot_mini_gfx::{Button, Draw, Renderer, Sprite, Stage, TexId};
use slot_mini_ui::curl_runs::{joined, wider};
use slot_mini_ui::sticker::{sticker_face, StickerFields, STICKER_H, STICKER_W};
use slot_mini_ui::sticker_curl::{strips, Pose};
use slot_mini_ui::{draw_backdrop, ease, hint_face, line_ink, title_line_face, FOOTER_Y};

use crate::paths::Paths;
use crate::settings::Settings;
use crate::update::{Status, Updater};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Row {
    Sounds,
    IngameMenu,
    Recents,
    BootLauncher,
    BootScreen,
    Update,
    ExitToOnion,
}

pub enum Outcome {
    Stay,
    Close,
    ExitToOnion,
}

pub struct About {
    pub row: usize,
    rows: Vec<Row>,
    /// Whether slot-mini is installed as the boot launcher, and a message
    /// from the last attempt to change that.
    pub launcher: bool,
    /// Whether Onion shows slot's boot and shutdown screens.
    pub boot_screen: bool,
    pub note: Option<String>,
    /// Shown on the note line when there is no note: the CPU's state.
    pub diag: Option<String>,
    faces: Vec<(String, TexId, u32, u32)>,
    /// When MENU opened, for the sticker's peel-in.
    opened: Option<u64>,
    /// The sticker, scaled to [`STICKER_SCALE`], and the battery it shows.
    sticker: Option<(TexId, Option<u8>)>,
    /// A 1x1 paper texture for the backs of the curling strips.
    paper: Option<TexId>,
}

/// How a line of the About screen is set.
#[derive(Clone, Copy, Debug)]
enum Style {
    /// slot's title type: the version and the rows.
    Title,
    /// Small type up to nearly the screen's width.
    Note,
}

/// The widest MENU's note line may be.
const NOTE_W: f32 = 600.0;

/// slot draws its sticker 608 px wide on a screen of its own; MENU shares
/// the screen with seven rows, so it is drawn at three quarters of its size.
const STICKER_SCALE: f32 = 0.75;
const STICKER_Y: f32 = 10.0;
/// The rows' pitch under the sticker (slot's quick menu uses more).
const ROW_PITCH: f32 = 28.0;
/// slot's 8 px strips taken this many at a time: the sticker is large, and
/// software pays for every strip on every row (decision 60).
const CURL_GROUP: usize = 2;
/// How long the sticker takes to peel in.
pub const PEEL_MS: u64 = 600;
/// The backs of the strips: slot's sticker paper.
const PAPER: [u8; 4] = [0xf4, 0xf1, 0xea, 0xff];
/// The serial printed and barcoded on the sticker: slot prints its git
/// hash; slot-mini prints its version, so the sticker only changes with a
/// release.
fn serial() -> String {
    let digits: String = env!("CARGO_PKG_VERSION")
        .chars()
        .filter(|c| c.is_ascii_digit())
        .collect();
    let body = format!("SM{digits}");
    format!("{body}{}", check39(&body))
}

/// Code 39's modulo-43 check character.
fn check39(s: &str) -> char {
    const SET: &str = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ-. $/+%";
    let sum: usize = s.chars().filter_map(|c| SET.find(c)).sum();
    SET.chars().nth(sum % 43).unwrap_or('0')
}

impl About {
    pub fn new(paths: &Paths) -> About {
        About {
            row: 0,
            rows: vec![
                Row::Sounds,
                Row::IngameMenu,
                Row::Recents,
                Row::BootLauncher,
                Row::BootScreen,
                Row::Update,
                Row::ExitToOnion,
            ],
            launcher: crate::switch::installed(paths),
            boot_screen: crate::bootscreen::installed(paths),
            note: None,
            diag: None,
            faces: Vec::new(),
            opened: None,
            sticker: None,
            paper: None,
        }
    }

    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    pub fn press(
        &mut self,
        b: Button,
        settings: &mut Settings,
        paths: &Paths,
        updater: &mut Updater,
    ) -> Outcome {
        let n = self.rows.len();
        // Leaving mid-install would hide the restart that follows it.
        if updater.busy() && matches!(b, Button::B | Button::Menu) {
            return Outcome::Stay;
        }
        match b {
            Button::Up => self.row = (self.row + n - 1) % n,
            Button::Down => self.row = (self.row + 1) % n,
            Button::B | Button::Menu => return Outcome::Close,
            Button::A | Button::Left | Button::Right => match self.rows[self.row] {
                Row::Sounds => {
                    settings.sounds = !settings.sounds;
                    if let Err(e) = settings.write(paths) {
                        eprintln!("slot-mini: settings: {e}");
                    }
                }
                Row::IngameMenu => {
                    settings.ingame_menu = !settings.ingame_menu;
                    if let Err(e) = settings.write(paths) {
                        eprintln!("slot-mini: settings: {e}");
                    }
                }
                Row::Recents => {
                    settings.recents = !settings.recents;
                    if let Err(e) = settings.write(paths) {
                        eprintln!("slot-mini: settings: {e}");
                    }
                }
                Row::BootLauncher => {
                    let result = match self.launcher {
                        false => crate::switch::install(paths),
                        true => crate::switch::restore(paths),
                    };
                    self.note = Some(match result {
                        Ok(()) if !self.launcher => {
                            "Installed. Takes effect after this app closes.".into()
                        }
                        Ok(()) => "Onion's launcher is back.".into(),
                        Err(e) => format!("Not changed: {e}"),
                    });
                    self.launcher = crate::switch::installed(paths);
                    // Remembered, so an Onion update does not turn it off.
                    settings.boot_launcher = self.launcher;
                    if let Err(e) = settings.write(paths) {
                        eprintln!("slot-mini: settings: {e}");
                    }
                }
                Row::BootScreen => {
                    let result = match self.boot_screen {
                        false => crate::bootscreen::install(paths),
                        true => crate::bootscreen::restore(paths),
                    };
                    self.note = Some(match result {
                        Ok(()) if !self.boot_screen => {
                            "slot's boot and shutdown screens are on.".into()
                        }
                        Ok(()) => "Onion's boot and shutdown screens are back.".into(),
                        Err(e) => format!("Not changed: {e}"),
                    });
                    self.boot_screen = crate::bootscreen::installed(paths);
                    settings.boot_screen = self.boot_screen;
                    if let Err(e) = settings.write(paths) {
                        eprintln!("slot-mini: settings: {e}");
                    }
                }
                Row::Update if b == Button::A => {
                    self.note = None;
                    updater.press(paths, &settings.update_repo);
                }
                Row::Update => {}
                Row::ExitToOnion if b == Button::A => return Outcome::ExitToOnion,
                Row::ExitToOnion => {}
            },
            _ => {}
        }
        Outcome::Stay
    }

    fn face(&mut self, gfx: &mut Renderer, text: &str, style: Style) -> (TexId, u32, u32) {
        let key = format!("{style:?}:{text}");
        if let Some((_, t, w, h)) = self.faces.iter().find(|f| f.0 == key) {
            return (*t, *w, *h);
        }
        let f = match style {
            Style::Title => title_line_face(text),
            // Notes can be a sentence: nearly the screen's width.
            Style::Note => line_ink(text, [0xf6, 0xf4, 0xef], NOTE_W),
        };
        let t = gfx.upload_rgba(f.w, f.h, &f.rgba);
        self.faces.push((key, t, f.w, f.h));
        (t, f.w, f.h)
    }

    pub fn close(self, gfx: &mut Renderer) {
        for (_, t, ..) in self.faces {
            gfx.free(t);
        }
        for t in self.sticker.map(|s| s.0).into_iter().chain(self.paper) {
            gfx.free(t);
        }
    }

    /// Whether the sticker is still peeling in at `now`.
    pub fn peeling(&self, now: u64) -> bool {
        self.opened.is_none_or(|t| now.saturating_sub(t) < PEEL_MS)
    }

    /// The sticker's face, made again when the battery changes (as slot's
    /// `sync_about` does).
    fn sticker(&mut self, battery: Option<u8>, gfx: &mut Renderer) -> TexId {
        match self.sticker {
            Some((t, b)) if b == battery => return t,
            Some((t, _)) => gfx.free(t),
            None => {}
        }
        let face = sticker_face(&StickerFields {
            battery,
            serial: &serial(),
            version: env!("CARGO_PKG_VERSION"),
            baseos: None,
        });
        let (w, h) = sticker_size();
        let sprite = Sprite::from_rgba(face.w, face.h, &face.rgba).resized(w as u32, h as u32);
        let t = gfx.upload(sprite);
        self.sticker = Some((t, battery));
        t
    }

    fn draw_sticker(
        &mut self,
        stage: Stage,
        now: u64,
        battery: Option<u8>,
        gfx: &mut Renderer,
        out: &mut Vec<Draw>,
    ) {
        let tex = self.sticker(battery, gfx);
        let (w, h) = sticker_size();
        let x = ((stage.w - w) / 2.0).round();
        let opened = *self.opened.get_or_insert(now);
        let t = now.saturating_sub(opened) as f32 / PEEL_MS as f32;
        if t >= 1.0 {
            out.push(Draw::Tex {
                x,
                y: STICKER_Y,
                w,
                h,
                tex,
                alpha: 1.0,
            });
            return;
        }
        // Laid down from the left: the free end starts curled up off the
        // page and unrolls flat.
        let p = ease(t);
        let paper = *self
            .paper
            .get_or_insert_with(|| gfx.upload_rgba(1, 1, &PAPER));
        let pose = Pose {
            y: -12.0 * (1.0 - p),
            z: 40.0 * (1.0 - p),
            ..Pose::REST
        };
        let focus = [stage.w / 2.0, 240.0];
        let alpha = (t * 4.0).min(1.0);
        let rect = [x, STICKER_Y, w, h];
        let curl = strips(rect, 0.1 + 0.9 * p, 220.0 * (1.0 - p), &pose, focus);
        for strip in joined(wider(curl, CURL_GROUP)) {
            let (tex, uv) = match strip.back {
                true => (paper, [0.0, 0.0, 1.0, 1.0]),
                false => (tex, strip.uv),
            };
            out.push(Draw::Quad {
                corners: strip.corners,
                tex,
                uv,
                alpha,
            });
        }
    }

    fn label(&self, row: Row, settings: &Settings, update: &Status) -> String {
        let on = |b: bool| if b { "On" } else { "Off" };
        match row {
            Row::Update => match update {
                Status::Idle | Status::Failed(_) => "Check for updates".into(),
                Status::Checking => "Checking for updates...".into(),
                Status::UpToDate => "Up to date".into(),
                Status::Available(r) => format!("Install update {}", r.version),
                Status::Downloading { percent, .. } => format!("Downloading {percent}%"),
                Status::Installing(_) => "Installing...".into(),
                Status::Installed(_) => "Updated. Restarting...".into(),
            },
            Row::Sounds => format!("Sounds: {}", on(settings.sounds)),
            Row::IngameMenu => format!("In-game menu: {}", on(settings.ingame_menu)),
            Row::Recents => format!("Recents: {}", on(settings.recents)),
            Row::BootLauncher => format!("Boot launcher: {}", on(self.launcher)),
            Row::BootScreen => format!(
                "Splash screens: {}",
                if self.boot_screen { "slot" } else { "Onion" }
            ),
            Row::ExitToOnion => "Exit to Onion".into(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        stage: Stage,
        now: u64,
        battery: Option<u8>,
        ground: Option<(TexId, u32)>,
        settings: &Settings,
        update: &Status,
        gfx: &mut Renderer,
        out: &mut Vec<Draw>,
    ) {
        draw_backdrop(stage, ground, out);
        self.draw_sticker(stage, now, battery, gfx, out);
        let put = |out: &mut Vec<Draw>, (tex, w, h): (TexId, u32, u32), y: f32, alpha: f32| {
            out.push(Draw::Tex {
                x: ((stage.w - w as f32) / 2.0).round(),
                y,
                w: w as f32,
                h: h as f32,
                tex,
                alpha,
            });
        };
        let mut y = STICKER_Y + sticker_size().1 + 8.0;
        for (i, row) in self.rows.clone().into_iter().enumerate() {
            let text = self.label(row, settings, update);
            let f = self.face(gfx, &text, Style::Title);
            put(out, f, y, if i == self.row { 1.0 } else { 0.5 });
            // Seven rows and the note line fit above slot's footer.
            y += ROW_PITCH;
        }
        let update_note = match update {
            Status::UpToDate => Some(format!(
                "slot-mini {} is the latest release.",
                env!("CARGO_PKG_VERSION")
            )),
            Status::Available(r) => Some(format!(
                "slot-mini {} is out. A installs it and restarts.",
                r.version
            )),
            Status::Downloading { version, .. } | Status::Installing(version) => Some(format!(
                "Installing slot-mini {version}. Keep the power on."
            )),
            Status::Failed(e) => Some(e.to_string()),
            _ => None,
        };
        if let Some(note) = self.note.clone().or(update_note).or(self.diag.clone()) {
            let f = self.face(gfx, &note, Style::Note);
            put(out, f, y + 4.0, 0.9);
        }
        slot_mini_ui::draw_empty_slot(stage, None, out);
        for (i, (key, label)) in [("B", "BACK"), ("A", "SELECT")].into_iter().enumerate() {
            let key_text = format!("{key}:{label}");
            let (tex, w, h) = match self.faces.iter().find(|f| f.0 == key_text) {
                Some((_, t, w, h)) => (*t, *w, *h),
                None => {
                    let f = hint_face(key, label);
                    let t = gfx.upload_rgba(f.w, f.h, &f.rgba);
                    self.faces.push((key_text, t, f.w, f.h));
                    (t, f.w, f.h)
                }
            };
            let x = if i == 0 {
                24.0
            } else {
                stage.w - 24.0 - w as f32
            };
            out.push(Draw::Tex {
                x,
                y: FOOTER_Y,
                w: w as f32,
                h: h as f32,
                tex,
                alpha: 1.0,
            });
        }
    }
}

/// The sticker's size on MENU.
fn sticker_size() -> (f32, f32) {
    (
        (STICKER_W as f32 * STICKER_SCALE).round(),
        (STICKER_H as f32 * STICKER_SCALE).round(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_serial_ends_in_its_code39_check() {
        // "SM062": S=28 M=22 0 6 2, sum 58, 58 mod 43 = 15 = F.
        assert_eq!(check39("SM062"), 'F');
        assert!(serial().starts_with("SM"));
    }
}
