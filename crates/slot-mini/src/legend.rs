//! Which buttons the footer's legend names. The Miyoo's own, and a
//! controller's, are the letters slot prints; the PC app's keyboard has
//! its own keys for them (crates/slot-mini-pc/src/input.rs), and its
//! mouse has none: its legends are buttons to click.

use slot_mini_ui::{hint_face, UndoFace};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Glyphs {
    /// The Miyoo's buttons, and a controller's.
    #[default]
    Pad,
    #[cfg(feature = "pc")]
    Keyboard,
    #[cfg(feature = "pc")]
    Mouse,
}

impl Glyphs {
    /// What to print on the cap for the Miyoo's button `key`.
    pub fn cap(self, key: &'static str) -> &'static str {
        match self {
            Glyphs::Pad => key,
            #[cfg(feature = "pc")]
            Glyphs::Keyboard => match key {
                "A" => "ENTER",
                "B" => "X",
                "X" => "S",
                "Y" => "A",
                "L1" => "Q",
                "R1" => "W",
                "SELECT" => "BKSP",
                "START" => "SPACE",
                "MENU" => "ESC",
                other => other,
            },
            // No cap at all; the arrows stay, to be clicked.
            #[cfg(feature = "pc")]
            Glyphs::Mouse => match key {
                "LR" | "UD" => key,
                _ => "",
            },
        }
    }

    /// A legend: `cap` (from [`Glyphs::cap`]) beside `label`, or for the
    /// mouse the label as a button.
    pub fn face(self, cap: &str, label: &str) -> UndoFace {
        match self {
            #[cfg(feature = "pc")]
            Glyphs::Mouse => slot_mini_ui::button_face(label),
            _ => hint_face(cap, label),
        }
    }
}
