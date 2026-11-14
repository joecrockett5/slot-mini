//! MENU: about slot-mini, its settings, updates and the way back to Onion.
//! The first page holds Settings, Check for updates and Exit to Onion;
//! Settings opens a second page of switches, and B comes back from it.
//! Drawn on slot's menu ground: the wallpaper under slot's scrim, slot's
//! About sticker at the top (peeling in as MENU opens), then rows in slot's
//! title type, selected row bright and the rest dimmed as in slot's quick
//! menu.

use slot_mini_gfx::{Button, Draw, Renderer, Sprite, Stage, TexId};
use slot_mini_ui::curl_runs::{joined, wider};
use slot_mini_ui::sticker::{sticker_face, StickerFields, STICKER_H, STICKER_W};
use slot_mini_ui::sticker_curl::{strips, Pose};
use slot_mini_ui::{draw_backdrop, ease, line_ink, title_line_face, FOOTER_Y};

use crate::legend::Glyphs;
use crate::paths::Paths;
use crate::settings::Settings;
use crate::sync::{Progress, Stage as SyncStage, Sync, View as SyncView};
use crate::update::{Status, Updater};

#[cfg(feature = "pc")]
#[path = "about_pc.rs"]
mod pc;
#[cfg(feature = "pc")]
pub use pc::{short_path, Host, HostRow, Offer, Page, PageRow};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Row {
    /// Opens the settings page.
    Settings,
    Sounds,
    IngameMenu,
    Recents,
    BootLauncher,
    BootScreen,
    OnlineLabels,
    Update,
    ExitToOnion,
    /// MENU's first page, while paired: a full sync now.
    SyncNow,
    /// Opens the sync page.
    Sync,
    /// The sync page: find a PC, or the PC this Miyoo is paired with.
    Pair,
    AutoSync,
    Unpair,
    /// The shelf on screen as a scrolling shelf or as a grid.
    View,
    /// A row the PC app answers for itself.
    #[cfg(feature = "pc")]
    Host(HostRow),
}

/// MENU's first page.
const MAIN: [Row; 3] = [Row::Settings, Row::Update, Row::ExitToOnion];
/// The settings page.
const SETTINGS: [Row; 8] = [
    Row::Sounds,
    Row::IngameMenu,
    Row::Recents,
    Row::View,
    Row::BootLauncher,
    Row::BootScreen,
    Row::OnlineLabels,
    Row::Sync,
];
/// The sync page before pairing, and after.
const SYNC_UNPAIRED: [Row; 1] = [Row::Pair];
const SYNC_PAIRED: [Row; 3] = [Row::Pair, Row::AutoSync, Row::Unpair];
/// How many rows show at once; a longer page scrolls with the selection.
const ROWS_SHOWN: usize = 7;
/// How many faces are kept: a sync's progress makes new ones all the time.
const FACES_KEPT: usize = 48;

pub enum Outcome {
    Stay,
    Close,
    ExitToOnion,
    /// The view row was turned: the shelf on screen changes its view.
    ToggleView,
    /// The PC app's row was chosen (A) or changed (Left, Right).
    #[cfg(feature = "pc")]
    Host(HostRow),
}

pub struct About {
    pub row: usize,
    rows: Vec<Row>,
    /// The pages B leads back through, and the row each was left on.
    stack: Vec<(Vec<Row>, usize)>,
    /// Which buttons the legend names.
    pub glyphs: Glyphs,
    /// What the PC app shows on its rows; `None` on the Miyoo.
    #[cfg(feature = "pc")]
    pub host: Option<Host>,
    /// The id of the host's page, while it shows.
    #[cfg(feature = "pc")]
    paged: Option<u32>,
    /// Whether slot-mini is installed as the boot launcher, and a message
    /// from the last attempt to change that.
    pub launcher: bool,
    /// Whether Onion shows slot's boot and shutdown screens.
    pub boot_screen: bool,
    pub note: Option<String>,
    /// Whether the shelf on screen is drawn as a grid (the view row), and
    /// `None` where there is no shelf to turn.
    pub grid: Option<bool>,
    /// The row chosen in the list of PCs found.
    hub_row: usize,
    /// Unpair was pressed once: the next A unpairs.
    unpair_armed: bool,
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
    /// A sheet's large type: the pairing code.
    #[cfg(feature = "pc")]
    Big,
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
    serial_of("SM", env!("CARGO_PKG_VERSION"))
}

fn serial_of(prefix: &str, version: &str) -> String {
    let digits: String = version.chars().filter(|c| c.is_ascii_digit()).collect();
    let body = format!("{prefix}{digits}");
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
            rows: MAIN.to_vec(),
            stack: Vec::new(),
            glyphs: Glyphs::default(),
            #[cfg(feature = "pc")]
            host: None,
            #[cfg(feature = "pc")]
            paged: None,
            launcher: crate::switch::installed(paths),
            boot_screen: crate::bootscreen::installed(paths),
            note: None,
            grid: None,
            hub_row: 0,
            unpair_armed: false,
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

    /// Whether the settings page is showing.
    pub fn in_settings(&self) -> bool {
        !self.stack.is_empty()
    }

    /// Open a page under this one.
    fn open(&mut self, rows: &[Row]) {
        let from = std::mem::replace(&mut self.rows, rows.to_vec());
        self.stack.push((from, self.row));
        self.row = 0;
        self.note = None;
    }

    /// B: the page this one was opened from.
    fn back(&mut self) {
        if let Some((rows, row)) = self.stack.pop() {
            self.rows = rows;
            self.row = row;
            self.note = None;
        }
    }

    /// The settings page: the Miyoo's, or the PC's.
    fn settings_rows(&self) -> Vec<Row> {
        #[cfg(feature = "pc")]
        if self.host.is_some() {
            return pc::SETTINGS.to_vec();
        }
        SETTINGS
            .into_iter()
            .filter(|r| *r != Row::View || self.grid.is_some())
            .collect()
    }

    /// Keep the rows that come and go with pairing in step: Sync now on
    /// the first page, and the sync page's own.
    fn pairing_rows(&mut self, paired: bool) {
        #[cfg(feature = "pc")]
        if self.host.is_some() {
            return;
        }
        let fix = |rows: &mut Vec<Row>, row: &mut usize| {
            let was = rows.get(*row).copied();
            if rows.first() == Some(&Row::Settings) {
                let has = rows.contains(&Row::SyncNow);
                match (paired, has) {
                    (true, false) => rows.insert(1, Row::SyncNow),
                    (false, true) => rows.retain(|r| *r != Row::SyncNow),
                    _ => return,
                }
            } else if rows.first() == Some(&Row::Pair) {
                if paired == (rows.len() > 1) {
                    return;
                }
                *rows = match paired {
                    true => SYNC_PAIRED.to_vec(),
                    false => SYNC_UNPAIRED.to_vec(),
                };
            } else {
                return;
            }
            // Stay on the row that was selected where it is still there.
            *row = was
                .and_then(|w| rows.iter().position(|r| *r == w))
                .unwrap_or(0);
        };
        fix(&mut self.rows, &mut self.row);
        for (rows, row) in &mut self.stack {
            fix(rows, row);
        }
    }

    /// A press while pairing or a sync has the page.
    fn stage_press(&mut self, b: Button, stage: SyncStage, sync: &mut Sync, wifi: bool) {
        let back = matches!(b, Button::B | Button::Menu);
        match stage {
            SyncStage::Idle => {}
            SyncStage::Hubs(found) => match b {
                Button::Up if !found.is_empty() => {
                    self.hub_row = (self.hub_row + found.len() - 1) % found.len()
                }
                Button::Down if !found.is_empty() => {
                    self.hub_row = (self.hub_row + 1) % found.len()
                }
                Button::A => match found.get(self.hub_row.min(found.len().max(1) - 1)) {
                    Some(hub) => sync.choose(hub.clone()),
                    None => sync.search(wifi),
                },
                _ if back => sync.dismiss(),
                _ => {}
            },
            SyncStage::Code { .. } if b == Button::A => sync.confirm(),
            // B stops a sync at its next safe point; the summary follows.
            SyncStage::Syncing(_) if b == Button::B => sync.cancel(),
            SyncStage::Syncing(_) => {}
            SyncStage::Paired(_) | SyncStage::Summary(_) | SyncStage::Failed(_)
                if b == Button::A || back =>
            {
                sync.dismiss()
            }
            _ if back => sync.dismiss(),
            _ => {}
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn press(
        &mut self,
        b: Button,
        settings: &mut Settings,
        paths: &Paths,
        updater: &mut Updater,
        sync: &mut Sync,
        wifi: bool,
    ) -> Outcome {
        // Leaving mid-install would hide the restart that follows it.
        if updater.busy() && matches!(b, Button::B | Button::Menu) {
            return Outcome::Stay;
        }
        let view = sync.view();
        self.pairing_rows(view.hub.is_some());
        if view.stage != SyncStage::Idle {
            self.stage_press(b, view.stage, sync, wifi);
            return Outcome::Stay;
        }
        let n = self.rows.len();
        #[cfg(feature = "pc")]
        if self.paged.is_some() && (b == Button::B || n == 0) {
            return match b {
                Button::B => Outcome::Host(HostRow::PageBack),
                Button::Menu => Outcome::Close,
                _ => Outcome::Stay,
            };
        }
        if !matches!(b, Button::A) || self.rows[self.row] != Row::Unpair {
            self.unpair_armed = false;
        }
        match b {
            Button::Up => self.row = (self.row + n - 1) % n,
            Button::Down => self.row = (self.row + 1) % n,
            Button::B if self.in_settings() => self.back(),
            Button::B | Button::Menu => return Outcome::Close,
            Button::A | Button::Left | Button::Right => match self.rows[self.row] {
                Row::Settings if b == Button::A => {
                    let rows = self.settings_rows();
                    self.open(&rows);
                }
                Row::Settings => {}
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
                Row::OnlineLabels => {
                    settings.label_service = !settings.label_service;
                    self.note = settings.label_service.then(|| match paths.is_pc() {
                        false => {
                            "Carts without a label get one from slot's art host on Wi-Fi.".into()
                        }
                        true => "Carts without a label get one from slot's art host.".into(),
                    });
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
                Row::SyncNow if b == Button::A => sync.sync_now(wifi),
                Row::Sync if b == Button::A => {
                    let rows = match view.hub {
                        Some(_) => SYNC_PAIRED.to_vec(),
                        None => SYNC_UNPAIRED.to_vec(),
                    };
                    self.open(&rows);
                }
                Row::Pair if b == Button::A && view.hub.is_none() => {
                    self.hub_row = 0;
                    sync.search(wifi);
                }
                Row::SyncNow | Row::Sync | Row::Pair => {}
                Row::AutoSync => {
                    settings.auto_sync = !settings.auto_sync;
                    if let Err(e) = settings.write(paths) {
                        eprintln!("slot-mini: settings: {e}");
                    }
                }
                Row::Unpair if b == Button::A && self.unpair_armed => {
                    self.unpair_armed = false;
                    self.note = None;
                    sync.unpair();
                    self.pairing_rows(false);
                }
                Row::Unpair if b == Button::A => {
                    self.unpair_armed = true;
                    self.note = Some("A again unpairs. Nothing is deleted.".into());
                }
                Row::Unpair => {}
                Row::View => {
                    if let Some(grid) = self.grid.as_mut() {
                        *grid = !*grid;
                        return Outcome::ToggleView;
                    }
                }
                #[cfg(feature = "pc")]
                Row::Host(row) => return self.host_press(row, b),
            },
            _ => {}
        }
        Outcome::Stay
    }

    fn face(&mut self, gfx: &mut Renderer, text: &str, style: Style) -> (TexId, u32, u32) {
        let key = format!("{style:?}:{text}");
        if let Some(i) = self.faces.iter().position(|f| f.0 == key) {
            // The last used go to the back; the front is dropped when the
            // list is full, which a frame's own faces never reach.
            let f = self.faces.remove(i);
            let hit = (f.1, f.2, f.3);
            self.faces.push(f);
            return hit;
        }
        if self.faces.len() >= FACES_KEPT {
            let (_, t, ..) = self.faces.remove(0);
            gfx.free(t);
        }
        let f = match style {
            Style::Title => title_line_face(text),
            // Notes can be a sentence: nearly the screen's width.
            Style::Note => line_ink(text, [0xf6, 0xf4, 0xef], NOTE_W),
            #[cfg(feature = "pc")]
            Style::Big => slot_mini_ui::code_face(text),
        };
        // A host's page can count as it goes: its old lines are let go.
        #[cfg(feature = "pc")]
        if self.faces.len() >= 160 {
            for (_, t, ..) in self.faces.drain(..) {
                gfx.free(t);
            }
        }
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
        #[cfg(feature = "pc")]
        let pc_version = self.host.as_ref().map(|h| h.version.clone());
        #[cfg(not(feature = "pc"))]
        let pc_version: Option<String> = None;
        let face = match pc_version {
            Some(version) => sticker_face(&StickerFields {
                battery,
                serial: &serial_of("SP", &version),
                version: &version,
                baseos: None,
                pc: true,
            }),
            None => sticker_face(&StickerFields {
                battery,
                serial: &serial(),
                version: env!("CARGO_PKG_VERSION"),
                baseos: None,
                pc: false,
            }),
        };
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

    fn label(&self, row: Row, settings: &Settings, update: &Status, sync: &SyncView) -> String {
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
            Row::Settings => "Settings".into(),
            Row::OnlineLabels => format!("Online labels: {}", on(settings.label_service)),
            Row::Sounds => format!("Sounds: {}", on(settings.sounds)),
            Row::IngameMenu => format!("In-game menu: {}", on(settings.ingame_menu)),
            Row::Recents => format!("Recents: {}", on(settings.recents)),
            Row::BootLauncher => format!("Boot launcher: {}", on(self.launcher)),
            Row::BootScreen => format!(
                "Splash screens: {}",
                if self.boot_screen { "slot" } else { "Onion" }
            ),
            Row::ExitToOnion => "Exit to Onion".into(),
            Row::SyncNow => "Sync now".into(),
            Row::Sync => "Sync".into(),
            Row::Pair => match &sync.hub {
                Some(name) => format!("PC: {}", short_name(name)),
                None => "Pair with a PC".into(),
            },
            Row::AutoSync => format!("Auto sync: {}", on(settings.auto_sync)),
            Row::Unpair => "Unpair".into(),
            Row::View => format!(
                "View: {}",
                if self.grid == Some(true) {
                    "Grid"
                } else {
                    "Shelf"
                }
            ),
            #[cfg(feature = "pc")]
            Row::Host(row) => self.host_label(row),
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
        sync: &SyncView,
        gfx: &mut Renderer,
        out: &mut Vec<Draw>,
    ) {
        self.pairing_rows(sync.hub.is_some());
        draw_backdrop(stage, ground, out);
        #[cfg(feature = "pc")]
        let sheet = self.sheet().cloned();
        #[cfg(not(feature = "pc"))]
        let sheet: Option<()> = None;
        if sheet.is_none() {
            self.draw_sticker(stage, now, battery, gfx, out);
        }
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
        #[cfg(feature = "pc")]
        if let Some(page) = &sheet {
            // A wallpaper can be anything: the words get a ground of
            // their own.
            out.push(Draw::Rect {
                x: 12.0,
                y: 12.0,
                w: stage.w - 24.0,
                h: FOOTER_Y - 30.0,
                colour: [0.0, 0.0, 0.0, 0.6],
            });
            let mut top = pc::SHEET_TOP;
            let f = self.face(gfx, page.title.as_deref().unwrap_or(""), Style::Title);
            put(out, f, top, 1.0);
            top += pc::SHEET_TITLE_H;
            if let Some(big) = &page.big {
                let f = self.face(gfx, big, Style::Big);
                put(out, f, top, 1.0);
                top += pc::SHEET_BIG_H;
            }
            for line in &page.lines {
                if !line.is_empty() {
                    let f = self.face(gfx, line, Style::Note);
                    put(out, f, top, 0.9);
                }
                top += pc::SHEET_LINE_PITCH;
            }
            y = About::sheet_rows_top(page);
        }
        // Pairing and syncs take the rows' place while they run.
        let staged = stage_lines(sync, self.hub_row);
        let rows = match &staged {
            Some(_) => Vec::new(),
            None => self.rows.clone(),
        };
        // Up to seven rows and the note line fit above slot's footer; a
        // longer page scrolls, its edge row fading where more lie beyond.
        let first = first_shown(self.row, rows.len());
        let last = (first + ROWS_SHOWN).min(rows.len());
        for (i, row) in rows.iter().copied().enumerate().take(last).skip(first) {
            let text = self.label(row, settings, update, sync);
            let f = self.face(gfx, &text, Style::Title);
            let beyond = (i == first && first > 0) || (i + 1 == last && last < rows.len());
            let alpha = match (i == self.row, beyond) {
                (true, _) => 1.0,
                (false, true) => 0.22,
                (false, false) => 0.5,
            };
            put(out, f, y, alpha);
            y += ROW_PITCH;
        }
        let mut stage_note = None;
        if let Some(lines) = staged {
            for line in lines {
                match line {
                    Line::Row(text, alpha) => {
                        let f = self.face(gfx, &text, Style::Title);
                        put(out, f, y, alpha);
                        y += ROW_PITCH;
                    }
                    Line::Small(text) => {
                        let f = self.face(gfx, &text, Style::Note);
                        put(out, f, y + 4.0, 0.9);
                        y += SMALL_PITCH;
                    }
                    Line::Note(text) => stage_note = Some(text),
                }
            }
            // The note sits where a full page of rows would put it.
            y = y.max(STICKER_Y + sticker_size().1 + 8.0 + 3.0 * ROW_PITCH);
        }
        let staging = stage_note.is_some() || sync.stage != SyncStage::Idle;
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
        #[cfg(feature = "pc")]
        let update_note = update_note.or_else(|| self.host_note());
        let row_note = match self.rows.get(self.row) {
            Some(Row::Pair) if sync.hub.is_none() => {
                Some("Open slot-mini on your PC, on the same Wi-Fi, then press A.")
            }
            Some(Row::Pair) => Some("Paired. MENU, Sync now syncs everything."),
            Some(Row::AutoSync) if settings.auto_sync => {
                Some("Saves sync at start, before and after a game, on Wi-Fi.")
            }
            Some(Row::AutoSync) => Some("Only MENU, Sync now syncs."),
            Some(Row::Sync) if sync.hub.is_none() => {
                Some("Share games and saves with slot-mini on a PC.")
            }
            _ => None,
        }
        .map(String::from);
        let note = match staging {
            true => stage_note,
            false => self
                .note
                .clone()
                .or(update_note)
                .or(row_note)
                .or(self.diag.clone()),
        };
        if let Some(note) = note {
            let f = self.face(gfx, &note, Style::Note);
            put(out, f, y + 4.0, 0.9);
        }
        slot_mini_ui::draw_empty_slot(stage, None, out);
        for (i, (key, label)) in [("B", "BACK"), ("A", "SELECT")].into_iter().enumerate() {
            let key = self.glyphs.cap(key);
            let key_text = format!("{key}:{label}");
            let (tex, w, h) = match self.faces.iter().find(|f| f.0 == key_text) {
                Some((_, t, w, h)) => (*t, *w, *h),
                None => {
                    let f = self.glyphs.face(key, label);
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

impl About {
    /// The row drawn at height `y` of the screen, for the mouse.
    pub fn row_at(&self, y: f32) -> Option<usize> {
        let top = STICKER_Y + sticker_size().1 + 8.0;
        #[cfg(feature = "pc")]
        let top = self.sheet().map_or(top, About::sheet_rows_top);
        let i = ((y - top) / ROW_PITCH).floor();
        let i = (i >= 0.0).then_some(i as usize)?;
        let at = first_shown(self.row, self.rows.len()) + i;
        (i < ROWS_SHOWN && at < self.rows.len()).then_some(at)
    }
}

/// A line of the page while pairing or a sync has it.
enum Line {
    /// In the rows' type, at this brightness.
    Row(String, f32),
    /// In the note's type, under the rows.
    Small(String),
    /// On the note line.
    Note(String),
}

/// The pitch of small lines (a summary).
const SMALL_PITCH: f32 = 22.0;

/// The first row a page of `n` rows shows with `row` selected.
fn first_shown(row: usize, n: usize) -> usize {
    if n <= ROWS_SHOWN {
        return 0;
    }
    // The selection keeps a row of the page after it in sight.
    (row + 2).saturating_sub(ROWS_SHOWN).min(n - ROWS_SHOWN)
}

/// A PC's name, short enough for a row.
fn short_name(name: &str) -> String {
    const MAX: usize = 22;
    match name.chars().count() > MAX {
        true => format!("{}...", name.chars().take(MAX - 3).collect::<String>()),
        false => name.to_string(),
    }
}

/// What the page shows in place of its rows, if pairing or a sync has it.
fn stage_lines(sync: &SyncView, hub_row: usize) -> Option<Vec<Line>> {
    let row = |s: String| Line::Row(s, 1.0);
    let pc = sync.hub.clone().unwrap_or_else(|| "the PC".into());
    Some(match &sync.stage {
        SyncStage::Idle => return None,
        SyncStage::Searching => vec![
            row("Looking for a PC...".into()),
            Line::Note("B cancels.".into()),
        ],
        SyncStage::Hubs(found) if found.is_empty() => vec![
            row("No PC found".into()),
            Line::Note("Open slot-mini on the PC, on the same Wi-Fi. A looks again.".into()),
        ],
        SyncStage::Hubs(found) => {
            let at = hub_row.min(found.len() - 1);
            let first = first_shown(at, found.len());
            let mut lines: Vec<Line> = found
                .iter()
                .enumerate()
                .skip(first)
                .take(ROWS_SHOWN)
                .map(|(i, f)| Line::Row(short_name(&f.name), if i == at { 1.0 } else { 0.5 }))
                .collect();
            lines.push(Line::Note("A pairs with this PC. B cancels.".into()));
            lines
        }
        SyncStage::Connecting(name) => vec![
            row(format!("Asking {}...", short_name(name))),
            Line::Note("B cancels.".into()),
        ],
        SyncStage::Code { hub, code } => vec![
            Line::Row(short_name(hub), 0.5),
            row(spaced(code)),
            Line::Note("If the PC shows the same code, press A. B cancels.".into()),
        ],
        SyncStage::Confirming { hub, code } => vec![
            Line::Row(short_name(hub), 0.5),
            row(spaced(code)),
            Line::Note("Now confirm on the PC. B cancels.".into()),
        ],
        SyncStage::Paired(name) => vec![
            row(format!("Paired with {}", short_name(name))),
            Line::Note("MENU, Sync now copies games and saves both ways.".into()),
        ],
        SyncStage::Syncing(p) => {
            let mut lines = vec![Line::Row(format!("Syncing with {}", short_name(&pc)), 0.5)];
            match p {
                Progress::Connecting => lines.push(row("Connecting...".into())),
                Progress::Checking { percent } => {
                    lines.push(row(format!("Checking this card {percent}%")))
                }
                Progress::Planning => lines.push(row("Comparing...".into())),
                Progress::Transfer {
                    index,
                    count,
                    name,
                    to_miyoo,
                    done,
                    size,
                } => {
                    let verb = if *to_miyoo { "Getting" } else { "Sending" };
                    lines.push(row(format!("{verb} {index} of {count}")));
                    lines.push(Line::Small(short_name_to(name, 56)));
                    lines.push(Line::Small(format!(
                        "{} of {}",
                        crate::sync::size_text(*done),
                        crate::sync::size_text(*size)
                    )));
                }
            }
            lines.push(Line::Note("B stops safely.".into()));
            lines
        }
        SyncStage::Summary(text) => {
            let mut lines = vec![row("Sync finished".into())];
            // Seven small lines fit; the most important come first.
            lines.extend(text.iter().take(7).cloned().map(Line::Small));
            lines
        }
        SyncStage::Failed(why) => vec![row("Not synced".into()), Line::Note(why.clone())],
    })
}

/// A pairing code in two groups: "123 456".
fn spaced(code: &str) -> String {
    match code.len() {
        6 if code.is_ascii() => format!("{} {}", &code[..3], &code[3..]),
        _ => code.to_string(),
    }
}

/// `text` cut to `max` characters, keeping its end (a file's name).
fn short_name_to(text: &str, max: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    match chars.len() > max {
        true => format!(
            "...{}",
            chars[chars.len() - (max - 3)..].iter().collect::<String>()
        ),
        false => text.to_string(),
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
