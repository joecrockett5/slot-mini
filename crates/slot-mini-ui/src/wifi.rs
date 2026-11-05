// Copied from slot (https://github.com/BrandonKowalski/slot) at e66715d:
//   crates/slot-ui/src/network_menu.rs (the Wi-Fi bar glyphs only)
// Changes: the network menu itself is out of scope; only wifi_face and the
//   glyph helpers it needs are kept.
// Copyright (C) Brandon Kowalski and slot contributors. GPL-3.0-or-later.

use crate::icon::symbols_font;
use crate::plate::UndoFace;

pub const NET_ICON_PX: f32 = 30.0;
pub const WIFI_ICON_PX: f32 = 14.0;

const BAR_GLYPHS: [char; 4] = ['\u{f092f}', '\u{f091f}', '\u{f0925}', '\u{f0928}'];
const LOCK: char = '\u{f033e}';
const NET_GLYPHS: [char; 5] = [
    BAR_GLYPHS[0],
    BAR_GLYPHS[1],
    BAR_GLYPHS[2],
    BAR_GLYPHS[3],
    LOCK,
];

#[derive(Clone, Copy)]
struct Frame {
    w: u32,
    h: u32,
    left: i32,
    top: i32,
}

fn frame_at(px: f32, glyphs: &[char]) -> Option<Frame> {
    let font = symbols_font()?;
    let (mut left, mut right, mut top, mut bottom) = (i32::MAX, i32::MIN, i32::MIN, i32::MAX);
    for &ch in glyphs {
        let m = font.metrics(ch, px);
        left = left.min(m.xmin);
        right = right.max(m.xmin + m.width as i32);
        top = top.max(m.ymin + m.height as i32);
        bottom = bottom.min(m.ymin);
    }
    Some(Frame {
        w: (right - left).max(1) as u32,
        h: (top - bottom).max(1) as u32,
        left,
        top,
    })
}

fn glyph_face(ch: char, px: f32, glyphs: &[char], ink: [u8; 3]) -> UndoFace {
    let (Some(font), Some(f)) = (symbols_font(), frame_at(px, glyphs)) else {
        return UndoFace {
            rgba: vec![0; 4],
            w: 1,
            h: 1,
        };
    };
    let (m, cov) = font.rasterize(ch, px);
    let x0 = m.xmin - f.left;
    let y0 = f.top - (m.ymin + m.height as i32);
    let mut rgba = vec![0u8; (f.w * f.h * 4) as usize];
    for gy in 0..m.height {
        for gx in 0..m.width {
            let (dx, dy) = (x0 + gx as i32, y0 + gy as i32);
            if dx < 0 || dy < 0 || dx >= f.w as i32 || dy >= f.h as i32 {
                continue;
            }
            let at = ((dy as u32 * f.w + dx as u32) * 4) as usize;
            rgba[at..at + 3].copy_from_slice(&ink);
            rgba[at + 3] = cov[gy * m.width + gx];
        }
    }
    UndoFace {
        rgba,
        w: f.w,
        h: f.h,
    }
}

pub fn wifi_face(bars: u8, ink: [u8; 3]) -> UndoFace {
    glyph_face(
        BAR_GLYPHS[bars.min(3) as usize],
        WIFI_ICON_PX,
        &BAR_GLYPHS,
        ink,
    )
}
