//! What the PC app (crates/slot-mini-pc) asks of the shared app: it runs
//! games itself and stays alive around them, where the Miyoo's launcher
//! exits and is started again.

use std::path::PathBuf;

use slot_mini_gfx::{Button, Draw, Sprite};
use slot_mini_store::Cart;
use slot_mini_ui::{UndoFace, FOOTER_Y, HINT_H};

use super::{App, Phase, NOT_STARTED};
use crate::about::{Host, HostRow};
use crate::legend::Glyphs;
use crate::library;
use crate::settings::{Recents, State};

impl App {
    /// A game is about to run: remember it the way `launch::hand_off` does
    /// on the Miyoo (the place on the shelf, and the PC's own Recents).
    pub fn game_started(&mut self, cart: &Cart) {
        let state = State {
            shelf: Some(self.shelf_key()),
            cart: Some((cart.platform, cart.stem.clone())),
            launched: false,
        };
        if let Err(e) = state.write(&self.paths) {
            eprintln!("slot-mini: state: {e}");
        }
        let mut played = Recents::read(&self.paths);
        played.played(cart.platform, &cart.stem);
        if let Err(e) = played.write(&self.paths) {
            eprintln!("slot-mini: recents.txt: {e}");
        }
    }

    /// The game has ended (or never started): the cart comes back out.
    pub fn game_ended(&mut self) {
        self.exit = None;
        let Phase::Inserting { stem, .. } = &self.phase else {
            return;
        };
        let stem = stem.clone();
        let keep = self.current().map(|(k, c)| (k, c.platform, c.stem.clone()));
        self.recents = Recents::read(&self.paths);
        self.restock();
        if let Some((kind, platform, stem)) = keep {
            // Recents now leads with this cart; stay on the shelf it was
            // played from.
            self.select(kind, platform, &stem);
        }
        self.phase = Phase::Ejecting {
            at: NOT_STARTED,
            stem,
        };
    }

    /// Look at the library again (the window came back to the front): new
    /// ROMs, labels and art show up without a restart. Only while the shelf
    /// is at rest.
    pub fn rescan(&mut self) -> bool {
        if !self.at_rest() {
            return false;
        }
        let carts = library::scan(&self.paths);
        let favourites =
            crate::settings::Favourites::read(&crate::settings::favourites_file(&self.paths));
        if carts == self.carts && favourites == self.favourites {
            return false;
        }
        self.carts = carts;
        self.favourites = favourites;
        self.restock();
        self.refresh_view();
        true
    }

    /// Whether nothing is open or moving over the shelf.
    pub fn at_rest(&self) -> bool {
        self.phase == Phase::Shelf
            && self.studio.is_none()
            && self.about.is_none()
            && self.core.is_none()
    }

    /// RetroArch was found, moved or lost: where its cores are now.
    pub fn set_cores(&mut self, cores: Option<PathBuf>) {
        if let Some(pc) = self.paths.pc.as_mut() {
            pc.cores = cores;
        }
        self.cores_changed();
    }

    /// Cores were installed (or RetroArch moved): what the shelf says
    /// about a missing one is checked again.
    pub fn cores_changed(&mut self) {
        let had = self.notice.take();
        self.name_missing_core();
        if self.notice != had {
            self.shelf_named = Some(self.now);
        }
    }

    /// Show `icon` left of the clock (the sync status), or nothing.
    pub fn set_status_icon(&mut self, icon: Option<Sprite>) {
        if let Some((old, _, _)) = self.status_icon.take() {
            self.gfx.free(old);
        }
        self.status_icon = icon.map(|s| {
            let (w, h) = (s.w, s.h);
            (self.gfx.upload(s), w, h)
        });
    }

    /// What the PC app's rows on MENU show.
    pub fn set_host(&mut self, host: Host) {
        if host == self.host {
            return;
        }
        if let Some(about) = self.about.as_mut() {
            about.set_host(host.clone());
        }
        self.host = host;
    }

    pub fn host(&self) -> &Host {
        &self.host
    }

    /// The PC row chosen on MENU since this was last asked.
    pub fn take_host_action(&mut self) -> Option<HostRow> {
        self.host_action.take()
    }

    /// A line under MENU's rows (what a host row came to).
    pub fn set_about_note(&mut self, note: Option<String>) {
        if let Some(about) = self.about.as_mut() {
            about.note = note;
        }
    }

    /// The player picked up the keyboard or a controller: the legends
    /// name its buttons.
    pub fn set_glyphs(&mut self, glyphs: Glyphs) {
        if glyphs == self.glyphs {
            return;
        }
        self.glyphs = glyphs;
        if let Some(about) = self.about.as_mut() {
            about.glyphs = glyphs;
        }
        if let Some(ui) = self.studio.as_mut() {
            ui.glyphs = glyphs;
        }
        if let Some(statics) = self.core_statics.as_mut() {
            statics.set_glyphs(&mut self.gfx, glyphs);
        }
    }

    /// The mouse on MENU: the row under it is selected, a click chooses it.
    fn about_pointer(&mut self, p: Pointer) {
        let (Pointer::Move(_, y) | Pointer::Click(_, y)) = p else {
            return;
        };
        let Some(about) = self.about.as_mut() else {
            return;
        };
        let Some(row) = about.row_at(y) else {
            return;
        };
        about.row = row;
        if matches!(p, Pointer::Click(..)) {
            self.tap(Button::A);
        }
    }

    /// The mouse in the cart studio: the row under it is selected and a
    /// click opens it; a click picks a colour chip (twice on Custom opens
    /// it); anywhere else a click is A.
    fn studio_pointer(&mut self, p: Pointer) {
        use crate::studio::Screen;
        use crate::studio_view::{hit, Hit};
        let (Pointer::Move(x, y) | Pointer::Click(x, y)) = p else {
            return;
        };
        let click = matches!(p, Pointer::Click(..));
        let w = self.stage.w;
        let Some(ui) = self.studio.as_mut() else {
            return;
        };
        if ui.applying() {
            return;
        }
        let studio = &mut ui.studio;
        match (hit(studio, w, x, y), studio.screen) {
            (Some(Hit::Row(row)), _) => {
                studio.screen = Screen::List { row };
                if click {
                    self.tap(Button::A);
                }
            }
            (Some(Hit::Chip(i)), Screen::Chips { at, .. }) if click => match i == at {
                true => self.tap(Button::A),
                false => studio.chips_jump(i),
            },
            (None, Screen::Sheet { .. } | Screen::Custom { .. } | Screen::Hex { .. }) if click => {
                self.tap(Button::A)
            }
            _ => {}
        }
    }

    /// On a system's shelf, say so when the core its games run on is not
    /// in RetroArch's cores folder (or RetroArch was not found).
    pub(super) fn name_missing_core(&mut self) {
        let Some((crate::library::ShelfKind::System(platform), _)) = self.shelves.get(self.at)
        else {
            return;
        };
        let core = crate::cores::pc_default(*platform);
        if crate::cores::installed(&self.paths, core.libretro()) {
            return;
        }
        self.notice = Some(match self.paths.pc_cores() {
            Some(_) => format!("{}: {} is not installed", platform.name(), core.text()),
            None => format!("{}: RetroArch was not found", platform.name()),
        });
    }

    /// The first-run screen's lines: where games go, the import, and that
    /// sync is still to come.
    fn welcome_lines(&self) -> Vec<String> {
        if !self.paths.is_pc() || !self.shelves.is_empty() {
            return Vec::new();
        }
        let sep = std::path::MAIN_SEPARATOR;
        let import = match &self.host.import_note {
            Some(note) => note.clone(),
            None if self.glyphs == Glyphs::Mouse => {
                "or click IMPORT to copy them from your Miyoo's SD card.".into()
            }
            None => format!(
                "or press {} to import them from your Miyoo's SD card.",
                self.glyphs.cap("A")
            ),
        };
        vec![
            "Welcome to slot-mini".into(),
            "Your games live here, laid out like an Onion SD card:".into(),
            crate::about::short_path(&self.host.library),
            format!("Put ROMs in Roms{sep}GBA, Roms{sep}GBC or Roms{sep}GB there,"),
            import,
            "Saves do not sync with the Miyoo yet: that is coming.".into(),
        ]
    }

    /// Print the first-run screen's lines again when they change.
    pub(super) fn print_welcome(&mut self) {
        let lines = self.welcome_lines();
        if lines == self.welcome.1 {
            return;
        }
        for (tex, _, _) in self.welcome.0.drain(..) {
            self.gfx.free(tex);
        }
        for (i, line) in lines.iter().enumerate() {
            let face = match i {
                0 => slot_mini_ui::title_line_face(line),
                _ => slot_mini_ui::line_ink(line, [0xf6, 0xf4, 0xef], WELCOME_W),
            };
            let tex = self.gfx.upload_rgba(face.w, face.h, &face.rgba);
            self.welcome.0.push((tex, face.w, face.h));
        }
        self.welcome.1 = lines;
    }

    pub(super) fn draw_welcome(&self, out: &mut Vec<Draw>) {
        // The title, a gap, then the lines in pairs.
        let tops = [96.0, 168.0, 196.0, 244.0, 272.0, 320.0];
        if self.welcome.0.is_empty() {
            return;
        }
        // A wallpaper can be anything: the words get a ground of their own.
        out.push(Draw::Rect {
            x: 12.0,
            y: 76.0,
            w: self.stage.w - 24.0,
            h: 284.0,
            colour: [0.0, 0.0, 0.0, 0.6],
        });
        for ((tex, w, h), top) in self.welcome.0.iter().zip(tops) {
            out.push(Draw::Tex {
                x: ((self.stage.w - *w as f32) / 2.0).round(),
                y: top,
                w: *w as f32,
                h: *h as f32,
                tex: *tex,
                alpha: 1.0,
            });
        }
    }

    /// Whether the app is on the plain shelf, where the mouse picks carts.
    fn on_shelf(&self) -> bool {
        self.phase == Phase::Shelf && self.core.is_none()
    }

    pub(super) fn tap(&mut self, b: Button) {
        self.press(b);
        self.release(b);
    }

    /// The mouse did something, in the frame's own pixels.
    pub fn pointer(&mut self, p: Pointer) {
        if p == Pointer::Release {
            return self.bar_release();
        }
        // slot's own legends (MENU, the studio, the core picker) are
        // buttons to the mouse.
        if let Pointer::Click(x, y) = p {
            if self.legend_click(x, y) {
                return;
            }
        }
        if self.phase == Phase::About && matches!(p, Pointer::Move(..) | Pointer::Click(..)) {
            return self.about_pointer(p);
        }
        if self.phase == Phase::Studio && matches!(p, Pointer::Move(..) | Pointer::Click(..)) {
            return self.studio_pointer(p);
        }
        if !self.on_shelf() {
            // Other screens are lists: the wheel walks them and the right
            // button goes back.
            match p {
                Pointer::Back => self.tap(Button::B),
                Pointer::Wheel(_, _, lines) => {
                    let b = if lines > 0 { Button::Down } else { Button::Up };
                    for _ in 0..lines.unsigned_abs().min(8) {
                        self.tap(b);
                    }
                }
                _ => {}
            }
            return;
        }
        if let Pointer::Click(x, y) = p {
            if let Some((item, right)) = self.bar_at(x, y) {
                return self.bar_click(item, right);
            }
        }
        if self.shelves.is_empty() {
            // The first-run screen: a click takes its offer, as A does.
            if matches!(p, Pointer::Click(..)) {
                self.tap(Button::A);
            }
            return;
        }
        if let Pointer::Double(x, y) = p {
            // The second click on the bar is not a cart being opened.
            if self.bar_at(x, y).is_some() {
                return;
            }
        }
        let stage = self.stage;
        let grid = self.grid_on();
        match p {
            Pointer::Back | Pointer::Release => {}
            Pointer::Move(x, y) => {
                // Only the grid follows the pointer: on the shelf every
                // cart that came under it would pull the next one along.
                if !grid {
                    return;
                }
                let Some((_, shelf)) = self.shelves.get_mut(self.at) else {
                    return;
                };
                if let Some(i) = self.grid.hit(shelf, stage, x, y) {
                    if i != shelf.index {
                        shelf.select(i);
                        self.grid_settle = None;
                    }
                }
            }
            Pointer::Click(x, y) | Pointer::Double(x, y) => {
                if self.view_button(x, y) {
                    return self.toggle_view();
                }
                let double = matches!(p, Pointer::Double(..));
                let Some((_, shelf)) = self.shelves.get_mut(self.at) else {
                    return;
                };
                let on_selected = match grid {
                    true => match self.grid.hit(shelf, stage, x, y) {
                        Some(i) => {
                            if i != shelf.index {
                                shelf.select(i);
                                self.grid_settle = None;
                            }
                            true
                        }
                        None => false,
                    },
                    false => match shelf.hit(stage, x, y) {
                        Some(0) => true,
                        Some(slot) => {
                            for _ in 0..slot.abs() {
                                match slot < 0 {
                                    true => shelf.left(),
                                    false => shelf.right(),
                                }
                            }
                            false
                        }
                        None => false,
                    },
                };
                if double && on_selected {
                    self.insert(false);
                }
            }
            Pointer::Wheel(_, _, lines) => {
                let Some((_, shelf)) = self.shelves.get_mut(self.at) else {
                    return;
                };
                if grid {
                    self.grid.wheel(shelf, lines);
                    self.grid_settle = None;
                    return;
                }
                for _ in 0..lines.unsigned_abs().min(8) {
                    match lines < 0 {
                        true => shelf.left(),
                        false => shelf.right(),
                    }
                }
            }
        }
    }

    /// The left edge of the view button: just left of the clock.
    fn view_x(&self) -> f32 {
        (self.stage.w - EDGE - self.clock.printed.w as f32 - GAP - VIEW_PX as f32).round()
    }

    /// Whether a point is on the view button (with some slack around it).
    pub fn view_button(&self, x: f32, y: f32) -> bool {
        let mid = FOOTER_Y + HINT_H as f32 / 2.0;
        let half = VIEW_PX as f32 / 2.0 + 8.0;
        (x - (self.view_x() + VIEW_PX as f32 / 2.0)).abs() <= half && (y - mid).abs() <= half
    }

    /// The PC's corner of the footer, left of the clock: the view button,
    /// then the host's icon.
    pub(super) fn draw_pc_footer(&self, out: &mut Vec<Draw>) {
        let mid = FOOTER_Y + HINT_H as f32 / 2.0;
        let mut icon = |x: f32, (tex, w, h): (slot_mini_gfx::TexId, u32, u32)| {
            out.push(Draw::Tex {
                x: x.round(),
                y: (mid - h as f32 / 2.0).round(),
                w: w as f32,
                h: h as f32,
                tex,
                alpha: 1.0,
            });
        };
        if let Some(faces) = self.statics.view {
            // The button shows the view it leads to.
            icon(self.view_x(), faces[self.grid_on() as usize]);
        }
        if let Some(face) = self.status_icon {
            icon(self.view_x() - GAP - face.1 as f32, face);
        }
    }
}

/// What the mouse did, in the frame's pixels. A double click arrives as a
/// `Click` and then a `Double`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pointer {
    Move(f32, f32),
    Click(f32, f32),
    Double(f32, f32),
    /// The wheel turned by this many notches; down (towards later carts)
    /// is positive.
    Wheel(f32, f32, i32),
    /// The right button: back, like B.
    Back,
    /// The left button came up (after a `Click`).
    Release,
}

/// The footer's margin, and the space between the things in it.
const EDGE: f32 = 24.0;
const GAP: f32 = 14.0;
const VIEW_PX: u32 = 18;
/// The widest a line of the first-run screen may be.
const WELCOME_W: f32 = 600.0;

/// The view button's two faces: a grid of carts (shown on the shelf) and a
/// row of them with the middle one large (shown on the grid).
pub(super) fn view_faces(ink: [u8; 3]) -> [UndoFace; 2] {
    let face = |rects: &[[u32; 4]]| {
        let mut rgba = vec![0u8; (VIEW_PX * VIEW_PX * 4) as usize];
        for &[x, y, w, h] in rects {
            for py in y..y + h {
                for px in x..x + w {
                    let at = ((py * VIEW_PX + px) * 4) as usize;
                    rgba[at..at + 3].copy_from_slice(&ink);
                    rgba[at + 3] = 0xff;
                }
            }
        }
        UndoFace {
            rgba,
            w: VIEW_PX,
            h: VIEW_PX,
        }
    };
    let mut cells = Vec::new();
    for row in 0..3 {
        for col in 0..3 {
            cells.push([col * 7, row * 7, 4, 4]);
        }
    }
    [
        face(&cells),
        face(&[[0, 4, 3, 10], [5, 0, 8, 18], [15, 4, 3, 10]]),
    ]
}
