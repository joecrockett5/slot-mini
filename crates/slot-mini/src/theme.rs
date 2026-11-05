//! `theme.txt` in slot-mini's app folder: slot's Dark/Light appearance plus a
//! few colour overrides, in slot's `key = value` format (slot-store's ini.rs).
//!
//! ```text
//! appearance = light        # dark (default) or light
//! footer_ink = ffffff       # battery, clock and legend text
//! heart_ink = e83a3f        # the favourite heart
//! wallpaper_tint = 30       # darken the wallpaper, 0-100 percent
//! ```
//!
//! Unreadable lines are ignored; missing keys keep slot's defaults.

use slot_mini_store::ini;

use crate::paths::Paths;

pub const THEME_FILE: &str = "theme.txt";

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    pub light: bool,
    pub footer_ink: [u8; 3],
    pub heart_ink: [u8; 3],
    /// Black overlay alpha over the wallpaper, 0..=1.
    pub wallpaper_tint: f32,
}

impl Default for Theme {
    fn default() -> Self {
        Theme {
            light: false,
            footer_ink: slot_mini_ui::footer_ink(false),
            heart_ink: slot_mini_ui::HEART_INK,
            wallpaper_tint: 0.0,
        }
    }
}

pub fn hex(v: &str) -> Option<[u8; 3]> {
    let v = v.trim().trim_start_matches('#');
    if v.len() != 6 || !v.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    Some([0, 2, 4].map(|i| u8::from_str_radix(&v[i..i + 2], 16).unwrap_or(0)))
}

impl Theme {
    pub fn parse(text: &str) -> Theme {
        let m = ini::parse(text);
        let light = m
            .get("appearance")
            .is_some_and(|v| v.eq_ignore_ascii_case("light"));
        let d = Theme::default();
        Theme {
            light,
            footer_ink: m
                .get("footer_ink")
                .and_then(|v| hex(v))
                .unwrap_or(slot_mini_ui::footer_ink(light)),
            heart_ink: m
                .get("heart_ink")
                .and_then(|v| hex(v))
                .unwrap_or(d.heart_ink),
            wallpaper_tint: m
                .get("wallpaper_tint")
                .and_then(|v| v.trim_end_matches('%').trim().parse::<f32>().ok())
                .map(|p| (p / 100.0).clamp(0.0, 1.0))
                .unwrap_or(0.0),
        }
    }

    pub fn read(paths: &Paths) -> Theme {
        Theme::parse(&std::fs::read_to_string(paths.app().join(THEME_FILE)).unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_is_slots_dark_look() {
        assert_eq!(Theme::parse(""), Theme::default());
    }

    #[test]
    fn light_flips_the_footer_ink() {
        let t = Theme::parse("appearance = light\n");
        assert!(t.light);
        assert_eq!(t.footer_ink, [0, 0, 0]);
    }

    #[test]
    fn overrides_and_junk() {
        let t = Theme::parse(
            "# comment\nfooter_ink = #12ab34\nheart_ink = nothex\nwallpaper_tint = 150%\nnonsense\n",
        );
        assert_eq!(t.footer_ink, [0x12, 0xab, 0x34]);
        assert_eq!(t.heart_ink, slot_mini_ui::HEART_INK);
        assert_eq!(t.wallpaper_tint, 1.0);
    }

    #[test]
    fn hex_needs_six_digits() {
        assert_eq!(hex("fff"), None);
        assert_eq!(hex("A0b1C2"), Some([0xa0, 0xb1, 0xc2]));
    }
}
