//! The PC's shortcut bar: what the shelf's buttons do, listed above the
//! footer. With a controller or the keyboard each entry names its button
//! or key; with the mouse the entries are buttons to click. The Miyoo has
//! no bar: its buttons are printed on it.

use slot_mini_gfx::{Button, Draw, TexId};
use slot_mini_ui::{arrows_hint_face, button_face, hint_face, UndoFace, FOOTER_Y, HINT_H};

use super::{App, Phase};
use crate::legend::Glyphs;
use crate::library::ShelfKind;

/// One entry of the bar.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Item {
    Play,
    /// The first-run screen's offer.
    Import,
    Fave,
    Studio,
    /// The shelf before and the shelf after.
    Shelf,
    Core,
    Menu,
    /// To the grid (`true`) or back to the row.
    View(bool),
    /// Take the cart off Recents.
    Forget,
}

impl Item {
    fn label(self) -> &'static str {
        match self {
            Item::Play => "PLAY",
            Item::Import => "IMPORT",
            Item::Fave => "FAVE",
            Item::Studio => "STUDIO",
            Item::Shelf => "SHELF",
            Item::Core => "CORE",
            Item::Menu => "MENU",
            Item::View(true) => "GRID",
            Item::View(false) => "ROW",
            Item::Forget => "FORGET",
        }
    }

    /// The controller's button and the keyboard's key for this entry
    /// (crates/slot-mini-pc/src/input.rs).
    fn keys(self) -> (&'static str, &'static str) {
        match self {
            Item::Play | Item::Import => ("A", "ENTER"),
            Item::Fave => ("Y", "A"),
            Item::Studio => ("X", "BKSP"),
            Item::Shelf => ("L1 R1", "Q W"),
            Item::Core => ("START", "SPACE"),
            Item::Menu => ("SELECT", "ESC"),
            Item::View(_) => ("R2", "TAB"),
            Item::Forget => ("L3", "S"),
        }
    }

    fn face(self, glyphs: Glyphs) -> UndoFace {
        let (pad, keyboard) = self.keys();
        match glyphs {
            Glyphs::Pad => hint_face(pad, self.label()),
            Glyphs::Keyboard => hint_face(keyboard, self.label()),
            Glyphs::Mouse if self == Item::Shelf => arrows_hint_face(self.label()),
            Glyphs::Mouse => button_face(self.label()),
        }
    }
}

/// The bar as it is printed: what it was printed for, and each entry's
/// texture and place.
#[derive(Default)]
pub struct Bar {
    made: Option<(Glyphs, Vec<Item>)>,
    faces: Vec<(Item, TexId, [f32; 4])>,
    /// The entry the mouse is holding down (PLAY, for Hold A).
    held: bool,
}

/// The space between entries, between rows, and at the screen's edges.
const GAP: f32 = 14.0;
const ROW_GAP: f32 = 4.0;
const EDGE: f32 = 16.0;
/// How far the bar's ground reaches past its rows.
const PAD: f32 = 5.0;

/// Where entries `widths` wide go: rows filled left to right, each
/// centred, the last one just above the footer.
pub fn layout(widths: &[u32], stage_w: f32, floor: f32) -> Vec<[f32; 4]> {
    let mut rows: Vec<Vec<usize>> = vec![Vec::new()];
    let mut used = 0.0;
    for (i, &w) in widths.iter().enumerate() {
        let w = w as f32;
        let row = rows.last_mut().expect("there is always a row");
        if !row.is_empty() && used + GAP + w > stage_w - 2.0 * EDGE {
            rows.push(vec![i]);
            used = w;
        } else {
            used += if row.is_empty() { w } else { GAP + w };
            row.push(i);
        }
    }
    let mut out = vec![[0.0; 4]; widths.len()];
    let n = rows.len() as f32;
    let top = floor - PAD - n * HINT_H as f32 - (n - 1.0) * ROW_GAP;
    for (r, row) in rows.iter().enumerate() {
        let total: f32 = row.iter().map(|&i| widths[i] as f32).sum::<f32>()
            + GAP * row.len().saturating_sub(1) as f32;
        let mut x = ((stage_w - total) / 2.0).round();
        let y = top + r as f32 * (HINT_H as f32 + ROW_GAP);
        for &i in row {
            out[i] = [x, y, widths[i] as f32, HINT_H as f32];
            x += widths[i] as f32 + GAP;
        }
    }
    out
}

impl App {
    /// Whether the bar is on screen: on the plain shelf or grid.
    fn bar_shown(&self) -> bool {
        self.paths.is_pc() && self.phase == Phase::Shelf && self.core.is_none()
    }

    /// What the bar lists for the cart in hand.
    pub fn bar_items(&self) -> Vec<Item> {
        let Some((kind, cart)) = self.current() else {
            return vec![Item::Import, Item::Menu];
        };
        let mut items = vec![Item::Play, Item::Fave, Item::Studio];
        if self.shelves.len() > 1 {
            items.push(Item::Shelf);
        }
        if crate::core_scene::offers(cart) {
            items.push(Item::Core);
        }
        if kind == ShelfKind::Recents {
            items.push(Item::Forget);
        }
        items.push(Item::View(!self.grid_on()));
        items.push(Item::Menu);
        items
    }

    /// Print the bar again when its entries or the device in hand change.
    pub(super) fn sync_bar(&mut self) {
        if !self.bar_shown() {
            return;
        }
        let want = (self.glyphs, self.bar_items());
        if self.bar.made.as_ref() == Some(&want) {
            return;
        }
        for (_, tex, _) in self.bar.faces.drain(..) {
            self.gfx.free(tex);
        }
        let faces: Vec<UndoFace> = want.1.iter().map(|i| i.face(want.0)).collect();
        let widths: Vec<u32> = faces.iter().map(|f| f.w).collect();
        let floor = FOOTER_Y - (slot_mini_ui::MOUTH_H - HINT_H as f32) / 2.0;
        let at = layout(&widths, self.stage.w, floor);
        for ((item, face), rect) in want.1.iter().zip(&faces).zip(at) {
            let tex = self.gfx.upload_rgba(face.w, face.h, &face.rgba);
            self.bar.faces.push((*item, tex, rect));
        }
        self.bar.made = Some(want);
    }

    /// The bar, on a ground of its own (a wallpaper can be anything). It
    /// gives way to the shelf's name while that shows in the same place.
    pub(super) fn draw_bar(&self, out: &mut Vec<Draw>) {
        if !self.bar_shown() || self.bar.faces.is_empty() {
            return;
        }
        let alpha = 1.0 - self.title_alpha();
        if alpha <= 0.0 {
            return;
        }
        let top = self
            .bar
            .faces
            .iter()
            .map(|f| f.2[1])
            .fold(f32::MAX, f32::min);
        let bottom = self
            .bar
            .faces
            .iter()
            .map(|f| f.2[1] + f.2[3])
            .fold(0.0, f32::max);
        out.push(Draw::Rect {
            x: 0.0,
            y: top - PAD,
            w: self.stage.w,
            h: bottom - top + 2.0 * PAD,
            colour: [0.0, 0.0, 0.0, 0.62 * alpha],
        });
        for &(_, tex, [x, y, w, h]) in &self.bar.faces {
            out.push(Draw::Tex {
                x,
                y,
                w,
                h,
                tex,
                alpha,
            });
        }
    }

    /// The bar's entry under a point, and whether the point is on its
    /// right half.
    pub fn bar_at(&self, x: f32, y: f32) -> Option<(Item, bool)> {
        if !self.bar_shown() || self.title_alpha() >= 1.0 {
            return None;
        }
        self.bar
            .faces
            .iter()
            .find(|(_, _, [fx, fy, w, h])| x >= *fx && x < fx + w && y >= *fy && y < fy + h)
            .map(|&(item, _, [fx, _, w, _])| (item, x >= fx + w / 2.0))
    }

    /// The mouse went down on an entry.
    pub(super) fn bar_click(&mut self, item: Item, right: bool) {
        match item {
            // Down now and up when the mouse lets go, so holding it is
            // Hold A: the game from the beginning.
            Item::Play => {
                self.press(Button::A);
                self.bar.held = true;
            }
            Item::Import => self.tap(Button::A),
            Item::Fave => self.tap(Button::Y),
            Item::Studio => self.tap(Button::Select),
            Item::Shelf if right => self.tap(Button::R1),
            Item::Shelf => self.tap(Button::L1),
            Item::Core => self.tap(Button::Start),
            Item::Menu => self.tap(Button::Menu),
            Item::View(_) => self.tap(Button::R2),
            Item::Forget => self.tap(Button::X),
        }
    }

    /// The mouse let go.
    pub(super) fn bar_release(&mut self) {
        if std::mem::take(&mut self.bar.held) {
            self.release(Button::A);
        }
    }

    /// The legend under a point on the screens that have slot's own
    /// (MENU, the cart studio, the core picker): which one, counting in
    /// the order they are drawn, and whether the point is on its right
    /// half.
    fn legend_at(&self, x: f32, y: f32) -> Option<(usize, bool)> {
        if !self.paths.is_pc() || y < FOOTER_Y || y >= FOOTER_Y + HINT_H as f32 {
            return None;
        }
        self.draws
            .iter()
            .filter_map(|d| match *d {
                Draw::Tex {
                    x: lx, y: ly, w, h, ..
                } if ly == FOOTER_Y && h == HINT_H as f32 => Some((lx, w)),
                _ => None,
            })
            .enumerate()
            .find(|(_, (lx, w))| x >= *lx && x < lx + w)
            .map(|(i, (lx, w))| (i, x >= lx + w / 2.0))
    }

    /// A click on one of those legends presses its button. Whether there
    /// was a legend there.
    pub(super) fn legend_click(&mut self, x: f32, y: f32) -> bool {
        let Some((i, right)) = self.legend_at(x, y) else {
            return false;
        };
        let key = match self.phase {
            Phase::About => ["B", "A"].get(i).copied(),
            Phase::Studio => self
                .studio
                .as_ref()
                .and_then(|ui| ui.legend_keys.get(i).copied()),
            Phase::Shelf if self.core.is_some() => ["B", "LR", "A"].get(i).copied(),
            _ => None,
        };
        let b = match key {
            Some("A") => Button::A,
            Some("B") => Button::B,
            Some("X") => Button::X,
            Some("Y") => Button::Y,
            Some("LR") if right => Button::Right,
            Some("LR") => Button::Left,
            // Up is more, as on the hex digits.
            Some("UD") if right => Button::Up,
            Some("UD") => Button::Down,
            _ => return false,
        };
        self.tap(b);
        true
    }
}
