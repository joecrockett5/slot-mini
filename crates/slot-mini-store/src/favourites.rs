//! The favourites list, in slot's format (slot-store/src/slot_state.rs): one
//! `<platform>/<stem>` per line, platform lowercase. slot-mini on the Miyoo
//! and slot-mini PC each keep one, and sync merges them, so the format lives
//! here. (Moved from slot-mini's `settings.rs`.)

use std::path::Path;

use crate::{atomic_write, Platform};

/// An entry as its line.
pub fn list_line(platform: Platform, stem: &str) -> String {
    format!("{}/{}", platform.dir_name().to_ascii_lowercase(), stem)
}

/// A line as its entry; `None` for anything else.
pub fn list_entry(value: &str) -> Option<(Platform, String)> {
    let (platform, stem) = value.split_once('/')?;
    let platform = Platform::ALL
        .into_iter()
        .find(|p| platform.eq_ignore_ascii_case(p.dir_name()))?;
    (!stem.is_empty()).then(|| (platform, stem.to_string()))
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Favourites {
    pub list: Vec<(Platform, String)>,
}

impl Favourites {
    /// The list in `file`; empty if it cannot be read.
    pub fn read(file: &Path) -> Favourites {
        let text = std::fs::read_to_string(file).unwrap_or_default();
        let mut list = Vec::new();
        for e in text.lines().filter_map(list_entry) {
            if !list.contains(&e) {
                list.push(e);
            }
        }
        Favourites { list }
    }

    /// Write the list to `file`, unless that is what it already holds.
    pub fn write(&self, file: &Path) -> std::io::Result<()> {
        let text: String = self
            .list
            .iter()
            .map(|(p, s)| list_line(*p, s) + "\n")
            .collect();
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        if std::fs::read(file).is_ok_and(|old| old == text.as_bytes()) {
            return Ok(());
        }
        atomic_write(file, text.as_bytes())
    }

    pub fn contains(&self, platform: Platform, stem: &str) -> bool {
        self.list.iter().any(|(p, s)| *p == platform && s == stem)
    }

    /// Heart or un-heart; returns whether it is now a favourite.
    pub fn toggle(&mut self, platform: Platform, stem: &str) -> bool {
        if self.contains(platform, stem) {
            self.list.retain(|(p, s)| !(*p == platform && s == stem));
            return false;
        }
        self.list.push((platform, stem.to_string()));
        true
    }
}
