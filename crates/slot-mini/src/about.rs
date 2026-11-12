//! MENU: about slot-mini, its few settings, and the way back to Onion.
//! Drawn on slot's menu ground: the wallpaper under slot's scrim, rows in
//! slot's title type, selected row bright and the rest dimmed as in slot's
//! quick menu.

use slot_mini_gfx::{Button, Draw, Renderer, Stage, TexId};
use slot_mini_ui::{draw_backdrop, hint_face, line_ink, title_line_face, word_ink, FOOTER_Y};

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
}

/// How a line of the About screen is set.
#[derive(Clone, Copy, Debug)]
enum Style {
    /// slot's title type: the version and the rows.
    Title,
    /// Small type in slot's word width: the credits.
    Small,
    /// Small type up to nearly the screen's width.
    Note,
}

/// The widest MENU's note line may be.
const NOTE_W: f32 = 600.0;

const INFO: [&str; 4] = [
    concat!("slot-mini ", env!("CARGO_PKG_VERSION")),
    "Based on slot by Brandon Kowalski",
    "Licensed GPL-3.0-or-later",
    "Not affiliated with slot, Onion or Miyoo",
];

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
            Style::Small => word_ink(text, [0xf6, 0xf4, 0xef]),
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

    pub fn draw(
        &mut self,
        stage: Stage,
        ground: Option<(TexId, u32)>,
        settings: &Settings,
        update: &Status,
        gfx: &mut Renderer,
        out: &mut Vec<Draw>,
    ) {
        draw_backdrop(stage, ground, out);
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
        let mut y = 30.0;
        for (i, line) in INFO.iter().enumerate() {
            let style = if i == 0 { Style::Title } else { Style::Small };
            let f = self.face(gfx, line, style);
            put(out, f, y, if i == 0 { 1.0 } else { 0.8 });
            y += if i == 0 { 42.0 } else { 22.0 };
        }
        y += 6.0;
        for (i, row) in self.rows.clone().into_iter().enumerate() {
            let text = self.label(row, settings, update);
            let f = self.face(gfx, &text, Style::Title);
            put(out, f, y, if i == self.row { 1.0 } else { 0.5 });
            // Seven rows and the note line fit above slot's footer.
            y += 33.0;
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
