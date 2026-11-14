//! Logical item keys (docs/SYNC.md, "Items and keys").

use std::fmt;

use slot_mini_store::Platform;

use crate::names::fold;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub enum Kind {
    Label,
    Art,
    Save,
    Rom,
}

impl Kind {
    fn prefix(self) -> &'static str {
        match self {
            Kind::Rom => "rom",
            Kind::Label => "label",
            Kind::Art => "art",
            Kind::Save => "save",
        }
    }
}

pub(crate) fn platform_of(s: &str) -> Option<Platform> {
    Platform::ALL
        .into_iter()
        .find(|p| s.eq_ignore_ascii_case(p.dir_name()))
}

/// A file item's key: `rom:GBA/Sub/Game.gba`, `label:GBA/Game`,
/// `art:GBA/Game`, `save:GBA/Game.srm`.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Key {
    pub kind: Kind,
    pub sys: Platform,
    /// ROM: the path under `Roms/<sys>/`. Label and art: the stem. Save:
    /// `<stem>.srm` or `<stem>.rtc`.
    pub name: String,
}

impl Key {
    pub fn new(kind: Kind, sys: Platform, name: impl Into<String>) -> Key {
        Key {
            kind,
            sys,
            name: name.into(),
        }
    }

    pub fn parse(s: &str) -> Option<Key> {
        let (kind, rest) = s.split_once(':')?;
        let kind = match kind {
            "rom" => Kind::Rom,
            "label" => Kind::Label,
            "art" => Kind::Art,
            "save" => Kind::Save,
            _ => return None,
        };
        let (sys, name) = rest.split_once('/')?;
        let sys = platform_of(sys)?;
        if name.is_empty()
            || name
                .split('/')
                .any(|p| p.is_empty() || p == "." || p == "..")
        {
            return None;
        }
        let parts = name.split('/').count();
        let ok = match kind {
            Kind::Rom => parts <= 2,
            Kind::Label | Kind::Art => parts == 1,
            Kind::Save => parts == 1 && (name.ends_with(".srm") || name.ends_with(".rtc")),
        };
        ok.then(|| Key::new(kind, sys, name))
    }

    /// The form keys are compared in.
    pub fn folded(&self) -> String {
        fold(&self.to_string())
    }

    /// The stem that ties the item to its game.
    pub fn stem(&self) -> &str {
        match self.kind {
            Kind::Label | Kind::Art => &self.name,
            Kind::Save => &self.name[..self.name.len() - 4],
            Kind::Rom => {
                let file = self.name.rsplit('/').next().unwrap_or(&self.name);
                match file.rfind('.') {
                    Some(0) | None => file,
                    Some(dot) => &file[..dot],
                }
            }
        }
    }

    /// `srm` or `rtc` for a save.
    pub fn save_ext(&self) -> &str {
        &self.name[self.name.len() - 3..]
    }

    pub fn game(&self) -> GameId {
        GameId::new(self.sys, self.stem())
    }

    /// The path this item has under a root that keeps slot-mini's tree
    /// (not for saves).
    pub fn tree_path(&self) -> Option<String> {
        let sys = self.sys.dir_name();
        match self.kind {
            Kind::Rom => Some(format!("Roms/{sys}/{}", self.name)),
            Kind::Label => Some(format!("Labels/{sys}/{}.png", self.name)),
            Kind::Art => Some(format!("Roms/{sys}/Imgs/{}.png", self.name)),
            Kind::Save => None,
        }
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}/{}",
            self.kind.prefix(),
            self.sys.dir_name(),
            self.name
        )
    }
}

/// A game: a system and a ROM stem, `GBA/Golden Sun (USA)`.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct GameId {
    pub sys: Platform,
    pub stem: String,
}

impl GameId {
    pub fn new(sys: Platform, stem: impl Into<String>) -> GameId {
        GameId {
            sys,
            stem: stem.into(),
        }
    }

    pub fn parse(s: &str) -> Option<GameId> {
        let (sys, stem) = s.split_once('/')?;
        let sys = platform_of(sys)?;
        (!stem.is_empty() && !stem.contains('/')).then(|| GameId::new(sys, stem))
    }

    pub fn folded(&self) -> String {
        fold(&self.to_string())
    }

    /// The game's line in a favourites file.
    pub fn favourite_line(&self) -> String {
        slot_mini_store::list_line(self.sys, &self.stem)
    }
}

impl fmt::Display for GameId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.sys.dir_name(), self.stem)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_round_trip_and_name_their_game() {
        for (text, stem) in [
            ("rom:GBA/Golden Sun (USA).gba", "Golden Sun (USA)"),
            ("rom:GBA/Hacks/A.b.GBA", "A.b"),
            ("label:GB/Tetris (World)", "Tetris (World)"),
            ("art:GBC/Wario Land 3", "Wario Land 3"),
            ("save:GBA/Golden Sun (USA).srm", "Golden Sun (USA)"),
            ("save:GBC/Crystal.rtc", "Crystal"),
        ] {
            let key = Key::parse(text).unwrap();
            assert_eq!(key.to_string(), text);
            assert_eq!(key.stem(), stem);
            assert_eq!(key.game().sys, key.sys);
        }
        assert_eq!(
            Key::parse("rom:GBA/Game.GBA").unwrap().folded(),
            Key::parse("rom:gba/game.gba").unwrap().folded()
        );
        for bad in [
            "rom:NES/a.nes",
            "rom:GBA/a/b/c.gba",
            "rom:GBA/../x.gba",
            "save:GBA/x.sav",
            "label:GBA/a/b",
            "fav:gba/x",
            "rom:GBA/",
        ] {
            assert!(Key::parse(bad).is_none(), "{bad}");
        }
        assert_eq!(
            Key::parse("label:GBA/X").unwrap().tree_path().unwrap(),
            "Labels/GBA/X.png"
        );
        let game = GameId::parse("GBA/Golden Sun").unwrap();
        assert_eq!(game.to_string(), "GBA/Golden Sun");
        assert_eq!(game.favourite_line(), "gba/Golden Sun");
    }
}
