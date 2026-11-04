// Copied from slot (https://github.com/BrandonKowalski/slot) at e66715d:
//   crates/slot-ui/src/backdrop.rs
// Changes: game bezels removed (out of scope); default_background_owned
//   added; imports point at slot-mini crates (slot_gfx -> slot_mini_gfx with Canvas as Stage, slot_store -> slot_mini_store)
// Copyright (C) Brandon Kowalski and slot contributors. GPL-3.0-or-later.

use std::path::Path;
use std::sync::OnceLock;

use slot_mini_gfx::{Draw, Stage as Canvas, TexId, OUT_H, OUT_W};

use crate::art;

const DEFAULT_PNG: &[u8] = include_bytes!("../../../assets/background_default.png");
const BLUR_PNG: &[u8] = include_bytes!("../../../assets/background_default_blur.png");

const SCRIM: f32 = 0.62;

pub fn wallpaper_face(path: &Path, w: u32, h: u32) -> Option<Vec<u8>> {
    art::cover(path, w, h)
}

pub fn default_background() -> Option<&'static [u8]> {
    static P: OnceLock<Option<Vec<u8>>> = OnceLock::new();
    P.get_or_init(|| art::cover_bytes(DEFAULT_PNG, OUT_W, OUT_H))
        .as_deref()
}

/// The default ground, decoded each time and not kept (slot-mini addition:
/// the launcher crops its own copy, so the cached one would be dead weight).
pub fn default_background_owned() -> Option<Vec<u8>> {
    art::cover_bytes(DEFAULT_PNG, OUT_W, OUT_H)
}

/// The blurred carousel ground, scaled to cover the whole screen the same way as the default.
pub fn blur_background() -> Option<&'static [u8]> {
    static P: OnceLock<Option<Vec<u8>>> = OnceLock::new();
    P.get_or_init(|| art::cover_bytes(BLUR_PNG, OUT_W, OUT_H))
        .as_deref()
}

pub fn draw_ground(canvas: Canvas, face: Option<(TexId, u32)>, out: &mut Vec<Draw>) {
    let Some(face) = face else {
        return;
    };
    out.push(ground(canvas, face));
}

pub fn draw_backdrop(canvas: Canvas, face: Option<(TexId, u32)>, out: &mut Vec<Draw>) {
    let Some(face) = face else {
        return;
    };
    out.push(ground(canvas, face));
    out.push(Draw::Rect {
        x: 0.0,
        y: 0.0,
        w: canvas.w,
        h: OUT_H as f32,
        colour: [0.0, 0.0, 0.0, SCRIM],
    });
}

fn ground(canvas: Canvas, (tex, tex_w): (TexId, u32)) -> Draw {
    Draw::Part {
        x: 0.0,
        y: 0.0,
        w: canvas.w,
        h: OUT_H as f32,
        tex,
        alpha: 1.0,
        uv: canvas.cover(tex_w as f32),
    }
}
