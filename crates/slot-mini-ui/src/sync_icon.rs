//! The footer's sync status icon, beside slot's Wi-Fi icon. slot-mini's
//! own: slot has no sync, and its symbols font is cut down to the glyphs
//! slot uses, so the four marks are drawn here as strokes.

use slot_mini_gfx::{Draw, TexId};

use crate::battery::GAUGE_ROW_W;
use crate::footer::{Printed, FOOTER_Y};
use crate::plate::{UndoFace, HINT_H};

/// The footer's side margin, as `footer.rs` has it.
const FOOTER_MARGIN: f32 = 24.0;

/// The icon's box: as tall as the Wi-Fi bars.
pub const SYNC_ICON_W: u32 = 14;
pub const SYNC_ICON_H: u32 = 12;

/// What the icon says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncMark {
    /// A tick.
    Synced,
    /// Two arrows, one each way.
    Syncing,
    /// A cross: the PC cannot be reached.
    Unreachable,
    /// An exclamation mark: a save was kept twice.
    Conflict,
}

impl SyncMark {
    pub const ALL: [SyncMark; 4] = [
        SyncMark::Synced,
        SyncMark::Syncing,
        SyncMark::Unreachable,
        SyncMark::Conflict,
    ];

    pub fn index(self) -> usize {
        self as usize
    }

    /// The strokes: `[x0, y0, x1, y1, width]` in the icon's box.
    fn strokes(self) -> &'static [[f32; 5]] {
        match self {
            SyncMark::Synced => &[[1.5, 6.5, 5.0, 10.0, 2.0], [5.0, 10.0, 12.5, 2.0, 2.0]],
            SyncMark::Syncing => &[
                [1.0, 3.5, 12.0, 3.5, 1.6],
                [9.0, 0.8, 12.2, 3.5, 1.6],
                [9.0, 6.2, 12.2, 3.5, 1.6],
                [2.0, 8.5, 13.0, 8.5, 1.6],
                [5.0, 5.8, 1.8, 8.5, 1.6],
                [5.0, 11.2, 1.8, 8.5, 1.6],
            ],
            SyncMark::Unreachable => &[[2.5, 1.5, 11.5, 10.5, 2.0], [11.5, 1.5, 2.5, 10.5, 2.0]],
            SyncMark::Conflict => &[[7.0, 1.0, 7.0, 7.0, 2.4], [7.0, 10.6, 7.0, 10.8, 2.4]],
        }
    }
}

/// How far `(x, y)` is from the segment `a`-`b`.
fn distance(x: f32, y: f32, s: &[f32; 5]) -> f32 {
    let (dx, dy) = (s[2] - s[0], s[3] - s[1]);
    let len2 = dx * dx + dy * dy;
    let t = match len2 > 0.0 {
        true => (((x - s[0]) * dx + (y - s[1]) * dy) / len2).clamp(0.0, 1.0),
        false => 0.0,
    };
    let (px, py) = (s[0] + t * dx - x, s[1] + t * dy - y);
    (px * px + py * py).sqrt()
}

/// The icon's pixels, smoothed by sampling each pixel 4x4.
pub fn sync_face(mark: SyncMark, ink: [u8; 3]) -> UndoFace {
    let (w, h) = (SYNC_ICON_W, SYNC_ICON_H);
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for py in 0..h {
        for px in 0..w {
            let mut hits = 0u32;
            for sy in 0..4 {
                for sx in 0..4 {
                    let x = px as f32 + (sx as f32 + 0.5) / 4.0;
                    let y = py as f32 + (sy as f32 + 0.5) / 4.0;
                    if mark
                        .strokes()
                        .iter()
                        .any(|s| distance(x, y, s) <= s[4] / 2.0)
                    {
                        hits += 1;
                    }
                }
            }
            let at = ((py * w + px) * 4) as usize;
            rgba[at..at + 3].copy_from_slice(&ink);
            rgba[at + 3] = (hits * 255 / 16) as u8;
        }
    }
    UndoFace { rgba, w, h }
}

const GAP: f32 = 12.0;
const AFTER_WIFI: f32 = 8.0;

/// Draw the icon right of the Wi-Fi icon (`wifi_w` wide), or in its place
/// when Wi-Fi is off.
pub fn draw_sync_icon(
    percent: Printed,
    wifi_w: Option<u32>,
    face: (TexId, u32, u32),
    out: &mut Vec<Draw>,
) {
    let (tex, w, h) = face;
    let x = FOOTER_MARGIN
        + GAUGE_ROW_W
        + percent.w as f32
        + GAP
        + wifi_w.map_or(0.0, |w| w as f32 + AFTER_WIFI);
    let y = FOOTER_Y + (HINT_H as f32 - h as f32) / 2.0;
    out.push(Draw::Tex {
        x: x.round(),
        y: y.round(),
        w: w as f32,
        h: h as f32,
        tex,
        alpha: 1.0,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_marks_are_inked_and_differ() {
        let mut faces = Vec::new();
        for m in SyncMark::ALL {
            let f = sync_face(m, [255, 255, 255]);
            assert_eq!((f.w, f.h), (SYNC_ICON_W, SYNC_ICON_H));
            let inked = f.rgba.chunks(4).filter(|p| p[3] > 128).count();
            assert!((10..100).contains(&inked), "{m:?} inks {inked} pixels");
            faces.push(f.rgba);
        }
        faces.dedup();
        assert_eq!(faces.len(), 4);
    }
}
