//! How names are compared and which ones a side can store (docs/SYNC.md,
//! "Names").

use unicode_normalization::UnicodeNormalization;

/// The form names are compared in: Unicode NFC, lower-cased. Names are
/// never stored in this form; they travel verbatim.
pub fn fold(name: &str) -> String {
    name.nfc().collect::<String>().to_lowercase()
}

/// What a side's file system can hold.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NameRules {
    /// FAT32 and exFAT: the Miyoo's card.
    Fat,
    /// NTFS as Windows programs see it.
    Windows,
    /// Anything but `/` and NUL.
    Any,
}

const RESERVED: &[&str] = &[
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

impl NameRules {
    /// The rules of the machine this runs on.
    pub fn local() -> NameRules {
        if cfg!(windows) {
            NameRules::Windows
        } else {
            NameRules::Any
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            NameRules::Fat => "fat",
            NameRules::Windows => "windows",
            NameRules::Any => "any",
        }
    }

    pub fn parse(s: &str) -> NameRules {
        match s {
            "fat" => NameRules::Fat,
            "windows" => NameRules::Windows,
            _ => NameRules::Any,
        }
    }

    /// Whether a `/`-separated relative path can be created here.
    pub fn can_store(self, rel: &str) -> bool {
        !rel.is_empty() && rel.split('/').all(|part| self.can_store_name(part))
    }

    fn can_store_name(self, name: &str) -> bool {
        if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\0']) {
            return false;
        }
        if self == NameRules::Any {
            return name.len() <= 255;
        }
        if name.encode_utf16().count() > 255 {
            return false;
        }
        if name
            .chars()
            .any(|c| c.is_control() || "<>:\"/\\|?*".contains(c))
        {
            return false;
        }
        if name.ends_with('.') || name.ends_with(' ') {
            return false;
        }
        let base = name.split('.').next().unwrap_or("").to_ascii_lowercase();
        !RESERVED.contains(&base.trim_end())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folding_ignores_case_and_normal_form() {
        assert_eq!(fold("GAME.gba"), fold("game.GBA"));
        // e + combining acute, and the single code point.
        assert_eq!(fold("Poke\u{301}mon"), fold("Pok\u{e9}mon"));
        assert_ne!(fold("a"), fold("b"));
    }

    #[test]
    fn fat_and_windows_refuse_what_they_cannot_hold() {
        for rules in [NameRules::Fat, NameRules::Windows] {
            assert!(rules.can_store("Golden Sun (USA, Europe).gba"));
            assert!(rules.can_store("Hacks/Pok\u{e9}mon - Emerald.GBA"));
            for bad in [
                "What?.gba",
                "A:B.gba",
                "quote\".gb",
                "trailing.",
                "trailing ",
                "con.gba",
                "NUL",
                "a/../b",
                "tab\there.gb",
                "",
            ] {
                assert!(!rules.can_store(bad), "{bad:?}");
            }
        }
        assert!(NameRules::Any.can_store("What?.gba"));
        assert!(!NameRules::Any.can_store("a//b"));
        assert_eq!(NameRules::parse(NameRules::Fat.as_str()), NameRules::Fat);
    }
}
