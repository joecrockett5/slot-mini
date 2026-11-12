// Adapted from slot (https://github.com/BrandonKowalski/slot) at e66715d:
//   crates/slot-ui/src/lib.rs
// Changes: only the modules slot-mini draws (shelf, carts, slot chrome,
//   footer, icons, legends); GL-only and out-of-scope screens are left out.
// Copyright (C) Brandon Kowalski and slot contributors. GPL-3.0-or-later.

#![allow(dead_code)] // Copied modules keep more of slot's API than slot-mini calls.

mod art;
pub use art::{cover, cover_bytes, decode_png_bytes};
mod backdrop;
mod barcode;
mod battery;
mod bitmaps;
mod board;
mod cart;
pub mod curl_runs;
mod footer;
pub mod house_label;
pub mod hud;
pub mod hue;
mod icon;
pub mod liner;
pub mod palette;
mod plate;
pub mod power;
pub mod quick;
mod refusal;
mod shelf;
mod shell;
mod silhouette;
mod slot_chrome;
pub mod sticker;
pub mod sticker_curl;
pub mod text;
mod wifi;
pub use wifi::wifi_face;

pub use art::decode as decode_png;
pub use backdrop::{
    blur_background, default_background, default_background_owned, draw_backdrop, draw_ground,
    wallpaper_face,
};
pub use battery::{draw_gauge, BOLT_PX, GAUGE_H, GAUGE_W, WALL};
pub use bitmaps::{
    slot_back, slot_back_light, slot_front, slot_front_light, slot_plugged, slot_plugged_light,
    Mould, SLOT_H, SLOT_W,
};
pub use board::{
    board_face, board_from_shaped, board_in, chip_face, chip_shadow_face, grown, lid_at,
    lid_from_shaped, lift_of, padded, seats, shelf_cart_shaped, socket_face, Placed, Seats,
    CHIP_TIP, GBA_SEATS, HOP_LIFT, SHADOW_H, SHADOW_W, TURN_PAD,
};
pub use cart::{
    bare_face, cart_backing, cart_box, cart_face, cart_face_with, cart_interior, cart_shadow,
    clean_label, gb_cart_shadow, gb_shell_of, is_clear, label_colour, label_rect, label_size,
    label_text, shelf_shape, shows_interior, text_label, CartFace, CART_H, CART_W, GBC_LABEL_H,
    GBC_LABEL_W, GBC_LABEL_X, GBC_LABEL_Y, GB_CART_H, GB_CART_W, GB_LABEL_H, GB_LABEL_W,
    GB_LABEL_X, GB_LABEL_Y, LABEL_H, LABEL_W, LABEL_X, LABEL_Y,
};
pub use footer::{draw_corners, draw_footer, draw_wifi_icon, Printed, FOOTER_Y};
pub use hud::{footer_ink, Millis, HUD_INK, PLATE_H};
pub use icon::{badge_face, heart_face, icon_box, icon_face, warm_fonts, Badge, Icon};
pub use plate::{
    arrows_hint_face, hint_face, hint_row, hint_width, line_ink, title_face, title_line_face,
    word_face, word_ink, Hint, UndoFace, HINT_H,
};
pub use refusal::Refusal;
pub use shelf::{
    box_size, foot_y, repeat_gap, rest_y, Shelf, HEART_INK, HEART_SCALE, REPEAT_DELAY_MS,
    SIDE_SCALE,
};
pub use shell::{
    gb_plastics, gba_plastics, shell_for, shell_presets, Finish, Shell, DEFAULT_SHELL,
};
pub use silhouette::GbShell;
pub use slot_chrome::{
    draw_empty_slot, draw_plugged_slot, draw_slot_back, draw_slot_front, draw_slot_title, ease,
    housing, opening, slot_name_y, SlotChrome, LIP_H, MOUTH_H, MOUTH_W,
};
