// Copied from slot (https://github.com/BrandonKowalski/slot) at e66715d:
//   crates/slot-ui/src/quick_menu.rs (layout constants, the pointer and the
//     row text faces) and crates/slot-ui/src/power_menu.rs (menu type size,
//     `menu_face`)
// Changes: only the pieces slot-mini's in-game menu draws; the rest of slot's
//   quick menu (its settings pages) is not copied. Constants are public.
// Copyright (C) Brandon Kowalski and slot contributors. GPL-3.0-or-later.

use slot_mini_gfx::OUT_W;

use crate::plate::UndoFace;
use crate::text;

pub const MENU_PX: f32 = 30.0;
pub const MENU_MIN_PX: f32 = 18.0;
pub const MENU_H: u32 = 40;
pub const MENU_PAD: u32 = 18;
pub const MENU_INK: [u8; 3] = [0xf6, 0xf4, 0xef];

pub fn menu_face(label: &str) -> UndoFace {
    let Some(font) = text::label_font() else {
        return UndoFace {
            rgba: Vec::new(),
            w: 0,
            h: 0,
        };
    };
    let ink = text::line_width(font, label, MENU_PX, 0.0).ceil() as u32;
    let w = ink + 2 * MENU_PAD;
    let mut rgba = vec![0u8; (w * MENU_H * 4) as usize];
    let layout = text::fit(font, label, w as f32, 1, MENU_PX, MENU_MIN_PX);
    text::draw_centred(&mut rgba, w, MENU_H, &layout, MENU_INK);
    UndoFace { rgba, w, h: MENU_H }
}

pub const QUICK_PITCH: f32 = 52.0;
pub const QUICK_EDGE: f32 = 32.0;
pub const QUICK_LABEL_X: f32 = 53.0;
pub const QUICK_VALUE_GAP: f32 = 24.0;
pub const VALUE_MIN_SCALE: f32 = 0.6;
pub const POINTER_GAP: f32 = 17.0;
pub const POINTER_SCALE: usize = 3;
pub const POINTER: [&str; 11] = [
    "X.....", "XX....", "XoX...", "XooX..", "XoooX.", "XooooX", "XoooX.", "XooX..", "XoX...",
    "XX....", "X.....",
];
pub const POINTER_BLEED: u32 = 4;
pub const GROUND_SCRIM: f32 = 0.65;
pub const TYPE_DROP: f32 = 4.0;
pub const LEGEND_Y: f32 = 427.0;
pub const DIM_INK: [u8; 3] = [0x9a, 0x9a, 0xa4];
fn cap_centre(label: &fontdue::Font, h: u32) -> f32 {
    match label.horizontal_line_metrics(MENU_PX) {
        Some(v) => {
            (h as f32 - v.new_line_size) / 2.0 + v.ascent
                - label.metrics('H', MENU_PX).height as f32 / 2.0
        }
        None => h as f32 / 2.0,
    }
}

pub fn quick_pointer_face() -> UndoFace {
    let s = POINTER_SCALE;
    let w = POINTER[0].len() * s;
    let h = (MENU_H + 2 * POINTER_BLEED) as usize;
    let centre = match text::label_font() {
        Some(label) => cap_centre(label, MENU_H),
        None => MENU_H as f32 / 2.0,
    } + POINTER_BLEED as f32;
    let tip = POINTER.len() / 2 * s + s / 2;
    let top = (centre - tip as f32).round() as usize;
    let mut rgba = vec![0u8; w * h * 4];
    for (y, row) in POINTER.iter().enumerate() {
        for (x, _) in row.chars().enumerate().filter(|(_, ch)| *ch != '.') {
            for py in top + y * s..top + (y + 1) * s {
                for px in x * s..(x + 1) * s {
                    let at = (py * w + px) * 4;
                    rgba[at..at + 4].copy_from_slice(&[
                        MENU_INK[0],
                        MENU_INK[1],
                        MENU_INK[2],
                        0xff,
                    ]);
                }
            }
        }
    }
    UndoFace {
        rgba,
        w: w as u32,
        h: h as u32,
    }
}

pub fn quick_text_face(label: &str, colour: [u8; 3]) -> UndoFace {
    let Some(font) = text::label_font() else {
        return UndoFace {
            rgba: Vec::new(),
            w: 0,
            h: 0,
        };
    };
    let layout = text::fit(font, label, OUT_W as f32, 1, MENU_PX, MENU_PX);
    let set = layout
        .lines
        .iter()
        .map(|l| text::line_width(font, l, layout.px, layout.tracking))
        .fold(0.0, f32::max);
    let w = set.ceil() as u32 + 2 * MENU_PAD;
    let mut rgba = vec![0u8; (w * MENU_H * 4) as usize];
    text::draw_centred(&mut rgba, w, MENU_H, &layout, colour);
    UndoFace { rgba, w, h: MENU_H }
}
