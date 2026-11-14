//! What the PC app (crates/slot-mini-pc) asks of the shared app: it runs
//! games itself and stays alive around them, where the Miyoo's launcher
//! exits and is started again.

use std::path::PathBuf;

use slot_mini_gfx::{Button, Draw, Sprite};
use slot_mini_store::Cart;
use slot_mini_ui::{UndoFace, FOOTER_Y, HINT_H};

use super::{App, Phase, NOT_STARTED};
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
        let quiet = self.phase == Phase::Shelf
            && self.studio.is_none()
            && self.about.is_none()
            && self.core.is_none();
        if !quiet {
            return false;
        }
        let carts = library::scan(&self.paths);
        if carts == self.carts {
            return false;
        }
        self.carts = carts;
        self.restock();
        self.refresh_view();
        true
    }

    /// RetroArch was found, moved or lost: where its cores are now.
    pub fn set_cores(&mut self, cores: Option<PathBuf>) {
        if let Some(pc) = self.paths.pc.as_mut() {
            pc.cores = cores;
        }
    }

    /// The wallpaper behind the shelf on screen, for the window's borders.
    pub fn wallpaper(&self) -> Option<(&Sprite, u64)> {
        let (tex, _) = self.ground()?;
        Some((self.gfx.get(tex)?, tex.raw() as u64))
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

    /// Whether the app is on the plain shelf, where the mouse picks carts.
    fn on_shelf(&self) -> bool {
        self.phase == Phase::Shelf && self.core.is_none()
    }

    fn tap(&mut self, b: Button) {
        self.press(b);
        self.release(b);
    }

    /// The mouse did something, in the frame's own pixels.
    pub fn pointer(&mut self, p: Pointer) {
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
        let stage = self.stage;
        let grid = self.grid_on();
        match p {
            Pointer::Back => {}
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
}

/// The footer's margin, and the space between the things in it.
const EDGE: f32 = 24.0;
const GAP: f32 = 14.0;
const VIEW_PX: u32 = 18;

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
