//! The grid view: a shelf's carts standing in rows on the rails of a
//! cartridge storage case, labels facing out, with the selected cart lifted
//! off its rail. slot-mini's own (slot has only the scrolling shelf); it is
//! drawn with slot's parts: the shelf's cart faces, the slot housing's
//! colours for the rails, and the same empty slot and footer underneath.
//!
//! Like the shelf, it is built to be composited once: every resting cart is
//! drawn at one fixed scale ([`GRID_SCALE`]), so a face pre-scaled to it by
//! the face workers is copied 1:1, and only the lifted cart is scaled on the
//! fly.

use slot_mini_gfx::{Draw, Stage as Canvas, OUT_H};

use crate::cart::{cart_box, label_rect, CART_H, GB_CART_H};
use crate::hud::Millis;
use crate::shelf::{repeat_gap, Shelf, REPEAT_DELAY_MS};
use crate::slot_chrome::MOUTH_H;

/// A resting cart's size against its full size on the shelf.
pub const GRID_SCALE: f32 = 0.36;
/// Carts in a row.
pub const GRID_COLS: usize = 4;

/// One cell's width; four fill the 640 stage with a 24 px margin.
const CELL_W: f32 = 148.0;
/// Space above the first row, enough for a lifted cart.
const TOP: f32 = 40.0;
/// From one rail's top to the top of the tallest cart on the next row.
const ROW_GAP: f32 = 30.0;
/// The rail the carts stand on.
const RAIL_H: f32 = 7.0;
const RAIL_INSET: f32 = 14.0;
/// The selected cart: how far it lifts and how much it grows.
const LIFT: f32 = 9.0;
const GROW: f32 = 0.14;
/// How fast the lift follows the selection, and the rows the scroll.
const LIFT_RATE: f32 = 14.0;
const OMEGA: f32 = 18.0;
/// The grid ends where the slot's band begins.
const FLOOR: f32 = OUT_H as f32 - MOUTH_H;
const HEART_INSET: f32 = 3.0;

const RAIL: [[u8; 3]; 2] = [[0x24, 0x24, 0x29], [0xc9, 0xc9, 0xcf]];
const RAIL_EDGE: [[u8; 3]; 2] = [[0x4d, 0x4d, 0x57], [0xee, 0xee, 0xf2]];
const RAIL_FOOT: [[u8; 3]; 2] = [[0x05, 0x05, 0x08], [0x8c, 0x8c, 0x96]];

fn rgba(c: [u8; 3], a: f32) -> [f32; 4] {
    [
        c[0] as f32 / 255.0,
        c[1] as f32 / 255.0,
        c[2] as f32 / 255.0,
        a,
    ]
}

/// A direction on the D-pad.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Step {
    Left,
    Right,
    Up,
    Down,
}

/// The grid's own state: where the rows have scrolled to and how far the
/// selected cart has lifted. The selection itself is the shelf's `index`,
/// so both views of a shelf always show the same cart.
#[derive(Default)]
pub struct Grid {
    scroll: f32,
    vel: f32,
    /// The cart lifted and how far (0 to 1), and the one settling back.
    up: Option<(usize, f32)>,
    down: Option<(usize, f32)>,
    held: Option<(Step, Millis, usize)>,
    settled: bool,
}

fn row_h(shelf: &Shelf) -> f32 {
    let tall = shelf
        .carts
        .iter()
        .map(|c| cart_box(c.platform).1)
        .max()
        .unwrap_or(CART_H)
        .clamp(CART_H, GB_CART_H);
    (tall as f32 * GRID_SCALE).round()
}

fn pitch(shelf: &Shelf) -> f32 {
    row_h(shelf) + RAIL_H + ROW_GAP
}

fn rows(shelf: &Shelf) -> usize {
    shelf.carts.len().div_ceil(GRID_COLS)
}

fn left(canvas: Canvas) -> f32 {
    ((canvas.w - CELL_W * GRID_COLS as f32) / 2.0).round()
}

/// The rail under row `r`, before scrolling.
fn rail_y(shelf: &Shelf, r: usize) -> f32 {
    TOP + r as f32 * pitch(shelf) + row_h(shelf)
}

/// How far the rows can scroll: the last rail stays clear of the slot.
fn max_scroll(shelf: &Shelf) -> f32 {
    match rows(shelf) {
        0 => 0.0,
        n => (rail_y(shelf, n - 1) + RAIL_H + 12.0 - FLOOR).max(0.0),
    }
}

impl Grid {
    pub fn new() -> Grid {
        Grid::default()
    }

    /// Where the rows should rest so the selected row is in full view.
    fn target(&self, shelf: &Shelf) -> f32 {
        if shelf.carts.is_empty() {
            return 0.0;
        }
        let rail = rail_y(shelf, shelf.index / GRID_COLS);
        let top = rail - row_h(shelf) - TOP;
        let bottom = rail + RAIL_H + 12.0 - FLOOR;
        // Keep where we are if the row already shows; else move the least.
        let to = self.scroll_rest().clamp(bottom.min(top), top.max(bottom));
        to.clamp(0.0, max_scroll(shelf)).round()
    }

    fn scroll_rest(&self) -> f32 {
        self.scroll.round()
    }

    /// Show the selected row at once (a shelf just opened in this view).
    pub fn snap(&mut self, shelf: &Shelf) {
        self.scroll = 0.0;
        self.scroll = self.target(shelf);
        self.vel = 0.0;
        self.up = (!shelf.carts.is_empty()).then_some((shelf.index, 1.0));
        self.down = None;
        self.held = None;
        self.settled = true;
    }

    /// Whether the grid has anything still moving.
    pub fn moving(&self, shelf: &Shelf) -> bool {
        (self.scroll - self.target(shelf)).abs() > 0.5
            || self.down.is_some()
            || self.up.is_some_and(|(_, t)| t < 1.0)
    }

    pub fn update(&mut self, shelf: &Shelf, dt: f32) {
        if !self.settled {
            return self.snap(shelf);
        }
        // The same critically damped spring the shelf scrolls with.
        let target = self.target(shelf);
        let off = self.scroll - target;
        if off.abs() < 0.5 && self.vel.abs() < 1.0 {
            self.scroll = target;
            self.vel = 0.0;
        } else {
            let pull = self.vel + OMEGA * off;
            let decay = (-OMEGA * dt).exp();
            self.scroll = target + (off + pull * dt) * decay;
            self.vel = (self.vel - OMEGA * pull * dt) * decay;
        }
        let step = LIFT_RATE * dt;
        match self.up {
            Some((i, t)) if i == shelf.index => {
                self.up = Some((i, (t + step).min(1.0)));
            }
            other => {
                // The old one settles from wherever it had got to.
                self.down = other.filter(|(i, _)| *i < shelf.carts.len());
                self.up = (!shelf.carts.is_empty()).then_some((shelf.index, step.min(1.0)));
            }
        }
        if let Some((i, t)) = self.down {
            let t = t - step;
            self.down = (t > 0.0 && i != shelf.index).then_some((i, t));
        }
    }

    /// Move the selection one cell. Left and right run on through the rows
    /// and wrap at the ends, as the shelf does; up and down stop at the
    /// first and last rows, and down from above a short last row lands on
    /// its last cart.
    pub fn step(&mut self, shelf: &mut Shelf, step: Step) {
        let n = shelf.carts.len();
        if n < 2 {
            return;
        }
        let i = shelf.index;
        let to = match step {
            Step::Left => (i + n - 1) % n,
            Step::Right => (i + 1) % n,
            Step::Up if i >= GRID_COLS => i - GRID_COLS,
            Step::Down if i + GRID_COLS < n => i + GRID_COLS,
            Step::Down if i / GRID_COLS < (n - 1) / GRID_COLS => n - 1,
            _ => i,
        };
        if to != i {
            shelf.select(to);
        }
    }

    /// A D-pad press: step now and repeat while it is held, at the shelf's
    /// pace.
    pub fn hold(&mut self, shelf: &mut Shelf, step: Step, now: Millis) {
        self.step(shelf, step);
        self.held = Some((step, now + REPEAT_DELAY_MS, 0));
    }

    pub fn release(&mut self, step: Step) {
        if matches!(self.held, Some((held, _, _)) if held == step) {
            self.held = None;
        }
    }

    pub fn release_hold(&mut self) {
        self.held = None;
    }

    pub fn tick(&mut self, shelf: &mut Shelf, now: Millis) {
        let Some((step, due, fired)) = self.held else {
            return;
        };
        if now < due {
            return;
        }
        self.step(shelf, step);
        self.held = Some((step, now + repeat_gap(fired), fired + 1));
    }

    /// Scroll by `rows` (the mouse wheel): the selection moves that many
    /// rows, keeping its column where it can.
    pub fn wheel(&mut self, shelf: &mut Shelf, rows: i32) {
        let n = shelf.carts.len() as i32;
        if n == 0 || rows == 0 {
            return;
        }
        let to = (shelf.index as i32 + rows * GRID_COLS as i32).clamp(0, n - 1);
        if to as usize != shelf.index {
            shelf.select(to as usize);
        }
    }

    /// Where cart `i` rests, as `[x, y, w, h]`.
    pub fn cell(&self, shelf: &Shelf, canvas: Canvas, i: usize) -> [f32; 4] {
        let (cw, ch) = shelf.carts.get(i).map_or((0, 0), |c| cart_box(c.platform));
        let (w, h) = (
            (cw as f32 * GRID_SCALE).round(),
            (ch as f32 * GRID_SCALE).round(),
        );
        let x = left(canvas) + (i % GRID_COLS) as f32 * CELL_W + ((CELL_W - w) / 2.0).round();
        let y = rail_y(shelf, i / GRID_COLS) - h - self.scroll.round();
        [x, y, w, h]
    }

    /// The cell lifted by `t` (0 resting, 1 fully lifted).
    fn lifted(rect: [f32; 4], t: f32) -> [f32; 4] {
        let [x, y, w, h] = rect;
        let s = 1.0 + GROW * t;
        let (lw, lh) = (w * s, h * s);
        [x - (lw - w) / 2.0, y + h - lh - LIFT * t, lw, lh]
    }

    /// Where the selected cart is drawn now.
    pub fn selected_rect(&self, shelf: &Shelf, canvas: Canvas) -> [f32; 4] {
        let t = match self.up {
            Some((i, t)) if i == shelf.index => ease(t),
            _ => 0.0,
        };
        Self::lifted(self.cell(shelf, canvas, shelf.index), t)
    }

    /// The carts with any part in view, the selected one first.
    pub fn on_screen(&self, shelf: &Shelf, canvas: Canvas) -> Vec<usize> {
        self.within(shelf, canvas, 0.0)
    }

    /// The carts in view or within a row of it (kept decoded, so a scroll
    /// by one row shows faces at once).
    pub fn near(&self, shelf: &Shelf, canvas: Canvas) -> Vec<usize> {
        self.within(shelf, canvas, pitch(shelf))
    }

    fn within(&self, shelf: &Shelf, canvas: Canvas, margin: f32) -> Vec<usize> {
        let mut out = Vec::new();
        if shelf.carts.is_empty() {
            return out;
        }
        out.push(shelf.index);
        for i in 0..shelf.carts.len() {
            let [_, y, _, h] = self.cell(shelf, canvas, i);
            if i != shelf.index && y + h > -margin && y - LIFT < FLOOR + margin {
                out.push(i);
            }
        }
        out
    }

    /// The cart under a point, if it is in view.
    pub fn hit(&self, shelf: &Shelf, canvas: Canvas, x: f32, y: f32) -> Option<usize> {
        if y >= FLOOR {
            return None;
        }
        // The lifted cart is on top.
        let [sx, sy, sw, sh] = self.selected_rect(shelf, canvas);
        if !shelf.carts.is_empty() && x >= sx && x < sx + sw && y >= sy && y < sy + sh {
            return Some(shelf.index);
        }
        (0..shelf.carts.len()).find(|&i| {
            let [cx, cy, cw, ch] = self.cell(shelf, canvas, i);
            x >= cx && x < cx + cw && y >= cy && y < cy + ch
        })
    }

    /// Draw the rails and the carts on them. `hidden` leaves one cart out
    /// (it is in the slot, or flying to it) and `alpha` fades the whole case
    /// while it does.
    pub fn draw(
        &self,
        shelf: &Shelf,
        canvas: Canvas,
        light: bool,
        hidden: Option<&str>,
        alpha: f32,
        out: &mut Vec<Draw>,
    ) {
        let alpha = alpha.clamp(0.0, 1.0);
        if alpha <= 0.0 {
            return;
        }
        let tone = usize::from(light);
        let scroll = self.scroll.round();
        for r in 0..rows(shelf) {
            let y = rail_y(shelf, r) - scroll;
            if y + RAIL_H + 3.0 <= 0.0 || y - row_h(shelf) - LIFT >= FLOOR {
                continue;
            }
            let (x, w) = (RAIL_INSET, canvas.w - 2.0 * RAIL_INSET);
            // The rail: a lit top edge, the body, and the shade under it.
            for (dy, h, colour, a) in [
                (0.0, RAIL_H, RAIL[tone], 1.0),
                (0.0, 1.0, RAIL_EDGE[tone], 1.0),
                (RAIL_H, 3.0, RAIL_FOOT[tone], 0.35),
            ] {
                out.push(Draw::Rect {
                    x,
                    y: y + dy,
                    w,
                    h,
                    colour: rgba(colour, a * alpha),
                });
            }
        }
        let skip = |i: usize| hidden == Some(shelf.carts[i].stem.as_str());
        let mut top: Vec<(usize, f32)> = Vec::new();
        for i in 0..shelf.carts.len() {
            let rect = self.cell(shelf, canvas, i);
            if rect[1] + rect[3] <= 0.0 || rect[1] - LIFT >= FLOOR || skip(i) {
                continue;
            }
            let lift = [self.down, self.up]
                .into_iter()
                .flatten()
                .find(|(c, _)| *c == i)
                .map(|(_, t)| ease(t));
            match lift {
                Some(t) if t > 0.0 => top.push((i, t)),
                _ => self.cart(shelf, i, rect, alpha, out),
            }
        }
        // Lifted carts go over their neighbours, the selected one last.
        top.sort_by_key(|(i, _)| *i == shelf.index);
        for (i, t) in top {
            let rect = self.cell(shelf, canvas, i);
            // What it left on the rail: a shade where it stood.
            out.push(Draw::Rect {
                x: rect[0] + 6.0,
                y: rect[1] + rect[3] - 2.0,
                w: rect[2] - 12.0,
                h: 2.0,
                colour: [0.0, 0.0, 0.0, 0.45 * t * alpha],
            });
            self.cart(shelf, i, Self::lifted(rect, t), alpha, out);
        }
    }

    fn cart(&self, shelf: &Shelf, i: usize, rect: [f32; 4], alpha: f32, out: &mut Vec<Draw>) {
        let [x, y, w, h] = rect;
        if let Some(tex) = shelf.wrapped(i) {
            let (bw, bh) = crate::shelf::box_size(shelf.carts[i].platform);
            let s = w / cart_box(shelf.carts[i].platform).0 as f32;
            let (bw, bh) = (bw as f32 * s, bh as f32 * s);
            out.push(Draw::Tex {
                x: x + (w - bw) / 2.0,
                y: y + h - bh,
                w: bw,
                h: bh,
                tex,
                alpha,
            });
            return;
        }
        shelf.draw_cart_faded(i, rect, alpha, out);
        if let Some((tex, sw, sh)) = shelf.heart_on(i) {
            // Half the shelf's heart: the cart is about a third the size.
            let (sw, sh) = ((sw / 2.0).round(), (sh / 2.0).round());
            let (lx, ly, lw, _) = label_rect(shelf.shell_of(i));
            out.push(Draw::Tex {
                x: (x + (lx + lw) * w - sw - HEART_INSET).round(),
                y: (y + ly * h + HEART_INSET).round(),
                w: sw,
                h: sh,
                tex,
                alpha,
            });
        }
    }
}

fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A rectangle `t` of the way from `a` to `b` (a cart flying between its
/// cell and the place the shelf holds it over the slot).
pub fn between(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    let t = ease(t);
    [0, 1, 2, 3].map(|k| a[k] + (b[k] - a[k]) * t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use slot_mini_store::{Cart, Platform};

    fn shelf(n: usize) -> Shelf {
        Shelf::new(
            (0..n)
                .map(|i| Cart {
                    platform: Platform::Gba,
                    stem: format!("Game {i:02}"),
                    rom: format!("{i}.gba").into(),
                    label: None,
                    title: format!("Game {i:02}"),
                    code: String::new(),
                    shell: None,
                })
                .collect(),
        )
    }

    #[test]
    fn the_dpad_moves_between_cells() {
        let mut s = shelf(10);
        let mut g = Grid::new();
        g.step(&mut s, Step::Right);
        assert_eq!(s.index, 1);
        g.step(&mut s, Step::Down);
        assert_eq!(s.index, 5);
        g.step(&mut s, Step::Down);
        assert_eq!(s.index, 9);
        g.step(&mut s, Step::Down);
        assert_eq!(s.index, 9);
        g.step(&mut s, Step::Up);
        assert_eq!(s.index, 5);
        g.step(&mut s, Step::Left);
        g.step(&mut s, Step::Left);
        assert_eq!(s.index, 3);
        g.step(&mut s, Step::Up);
        assert_eq!(s.index, 3);
    }

    #[test]
    fn left_and_right_wrap_and_down_lands_on_a_short_last_row() {
        let mut s = shelf(6);
        let mut g = Grid::new();
        g.step(&mut s, Step::Left);
        assert_eq!(s.index, 5);
        g.step(&mut s, Step::Right);
        assert_eq!(s.index, 0);
        s.select(3);
        g.step(&mut s, Step::Down);
        assert_eq!(s.index, 5);
    }

    #[test]
    fn the_selected_row_scrolls_into_view_and_can_be_hit() {
        let mut s = shelf(40);
        let mut g = Grid::new();
        g.snap(&s);
        assert_eq!(g.scroll, 0.0);
        s.select(39);
        for _ in 0..200 {
            g.update(&s, 0.016);
        }
        let [x, y, w, h] = g.selected_rect(&s, Canvas::MINI);
        assert!(y >= 0.0 && y + h <= FLOOR, "{y} {h}");
        assert_eq!(g.hit(&s, Canvas::MINI, x + w / 2.0, y + h / 2.0), Some(39));
        assert!(g.on_screen(&s, Canvas::MINI).contains(&39));
        assert!(!g.on_screen(&s, Canvas::MINI).contains(&0));
        assert!(!g.moving(&s));
    }

    #[test]
    fn holding_a_direction_repeats() {
        let mut s = shelf(12);
        let mut g = Grid::new();
        g.hold(&mut s, Step::Right, 0);
        g.tick(&mut s, 399);
        assert_eq!(s.index, 1);
        g.tick(&mut s, 400);
        assert_eq!(s.index, 2);
        g.release(Step::Right);
        g.tick(&mut s, 2000);
        assert_eq!(s.index, 2);
    }
}
