// Copied from slot (https://github.com/BrandonKowalski/slot) at e66715d:
//   crates/slot-ui/src/battery.rs
// Changes: imports point at slot-mini crates (slot_gfx -> slot_mini_gfx with Canvas as Stage, slot_store -> slot_mini_store);
//   while charging the fill is green (CHARGING_FILL), asked for by slot-mini's maintainer.
// Copyright (C) Brandon Kowalski and slot contributors. GPL-3.0-or-later.

use crate::power::{Battery, Charge};
use slot_mini_gfx::{Draw, TexId};

use crate::footer::Printed;
use crate::plate::HINT_H;

pub const BOLT_PX: f32 = 18.0;

/// The gauge's fill while charging (slot-mini addition).
pub const CHARGING_FILL: [u8; 3] = [0x4c, 0xd9, 0x64];

pub const GAUGE_W: f32 = 26.4;
pub const GAUGE_H: f32 = 13.2;
const _: () = assert!(GAUGE_H < GAUGE_W);
const NUB_W: f32 = 3.0;
const NUB_H: f32 = 4.8;
pub const WALL: f32 = 1.8;
const GAP: f32 = 7.0;

const BOLT_W: f32 = 14.0;
const BOLT_H: f32 = 14.0;
const BOLT_GAP: f32 = 5.0;
const _: () = assert!(BOLT_GAP > 0.0);
pub const GAUGE_ROW_W: f32 = BOLT_W + BOLT_GAP + GAUGE_W + NUB_W + GAP;

pub fn draw_gauge(
    x: f32,
    y: f32,
    battery: Option<Battery>,
    percent: Printed,
    bolt: Option<TexId>,
    ink: [u8; 3],
    out: &mut Vec<Draw>,
) {
    let Some(b) = battery else {
        return;
    };
    let colour = [
        ink[0] as f32 / 255.0,
        ink[1] as f32 / 255.0,
        ink[2] as f32 / 255.0,
        1.0,
    ];

    let rect = |x: f32, y: f32, w: f32, h: f32, out: &mut Vec<Draw>| {
        out.push(Draw::Rect { x, y, w, h, colour });
    };

    let cx = x + BOLT_W + BOLT_GAP;

    rect(cx, y, GAUGE_W, WALL, out);
    rect(cx, y + GAUGE_H - WALL, GAUGE_W, WALL, out);
    rect(cx, y, WALL, GAUGE_H, out);
    rect(cx + GAUGE_W - WALL, y, WALL, GAUGE_H, out);
    rect(cx + GAUGE_W, y + (GAUGE_H - NUB_H) / 2.0, NUB_W, NUB_H, out);

    let inner = GAUGE_W - 4.0 * WALL;
    let fill = inner * f32::from(b.percent.min(100)) / 100.0;
    if fill > 0.0 {
        let colour = match b.charge {
            Charge::Charging => [
                CHARGING_FILL[0] as f32 / 255.0,
                CHARGING_FILL[1] as f32 / 255.0,
                CHARGING_FILL[2] as f32 / 255.0,
                1.0,
            ],
            _ => colour,
        };
        out.push(Draw::Rect {
            x: cx + 2.0 * WALL,
            y: y + 2.0 * WALL,
            w: fill,
            h: GAUGE_H - 4.0 * WALL,
            colour,
        });
    }

    if let (Charge::Charging, Some(tex)) = (b.charge, bolt) {
        out.push(Draw::Tex {
            x,
            y: y + (GAUGE_H - BOLT_H) / 2.0,
            w: BOLT_W,
            h: BOLT_H,
            tex,
            alpha: 1.0,
        });
    }

    if percent.w > 0 {
        let px = cx + GAUGE_W + NUB_W + GAP;
        let py = y + (GAUGE_H - HINT_H as f32) / 2.0;
        out.push(match percent.face {
            Some(tex) => Draw::Tex {
                x: px,
                y: py,
                w: percent.w as f32,
                h: HINT_H as f32,
                tex,
                alpha: 1.0,
            },
            None => Draw::Rect {
                x: px,
                y: py,
                w: percent.w as f32,
                h: HINT_H as f32,
                colour: [1.0, 1.0, 1.0, 0.08],
            },
        });
    }
}
