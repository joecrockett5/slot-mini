// Copied from slot (https://github.com/BrandonKowalski/slot) at e66715d:
//   crates/slot-ui/src/hud.rs (constants and footer_ink only)
// Changes: the HUD overlay itself (volume, brightness, toasts) is not used.
// Copyright (C) Brandon Kowalski and slot contributors. GPL-3.0-or-later.

pub type Millis = u64;

pub const HUD_MS: Millis = 1500;
const FADE_MS: Millis = 250;
const PULSE_MS: Millis = 500;

pub const PLATE_H: f32 = 40.0;
pub(crate) const PLATE: [f32; 4] = [0.0, 0.0, 0.0, 0.72];
pub const ALARM_BAND_H: f32 = 120.0;
const ALARM_BRIGHT: [f32; 4] = [0.82, 0.10, 0.10, 0.94];
const ALARM_DIM: [f32; 4] = [0.40, 0.03, 0.03, 0.94];

pub const HUD_ICON_PX: f32 = 24.0;
pub const HUD_INK: [u8; 3] = [0xf5, 0xf2, 0xef];

pub const FOOTER_INK: [u8; 3] = [0xff, 0xff, 0xff];
pub const FOOTER_INK_LIGHT: [u8; 3] = [0x00, 0x00, 0x00];

pub fn footer_ink(light: bool) -> [u8; 3] {
    if light {
        FOOTER_INK_LIGHT
    } else {
        FOOTER_INK
    }
}
