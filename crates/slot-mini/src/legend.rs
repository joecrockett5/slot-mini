//! Which buttons the footer's legend names. The Miyoo's own, and a
//! controller's, are the letters slot prints; the PC app's keyboard has
//! its own keys for them (crates/slot-mini-pc/src/input.rs).

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Glyphs {
    /// The Miyoo's buttons, and a controller's.
    #[default]
    Pad,
    #[cfg(feature = "pc")]
    Keyboard,
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
        }
    }
}
