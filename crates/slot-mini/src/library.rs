//! The carts on the card, grouped into slot's shelves.

use slot_mini_store::{sort_key, Cart, Platform};

use crate::paths::Paths;
use crate::settings::Favourites;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ShelfKind {
    Recents,
    Favourites,
    System(Platform),
}

impl ShelfKind {
    /// How the shelf is named in `state.txt`.
    pub fn key(self) -> String {
        match self {
            ShelfKind::Recents => "recents".into(),
            ShelfKind::Favourites => "favorites".into(),
            ShelfKind::System(p) => p.dir_name().into(),
        }
    }

    /// The title shown above the slot when switching shelves.
    pub fn title(self) -> &'static str {
        match self {
            ShelfKind::Recents => "Recents",
            ShelfKind::Favourites => "Favorites",
            ShelfKind::System(p) => p.name(),
        }
    }
}

/// Every cart on the card for systems Onion can launch. A system without
/// `Emu/<sys>/launch.sh` has no emulator installed and gets no shelf.
pub fn scan(paths: &Paths) -> Vec<Cart> {
    let mut carts = Vec::new();
    for p in Platform::ALL {
        if !paths.has_emulator(p) {
            continue;
        }
        match slot_mini_store::scan_platform(&paths.sd, p) {
            Ok(c) => carts.extend(c),
            Err(e) => eprintln!("slot-mini: scan {}: {e}", p.dir_name()),
        }
    }
    carts
}

/// slot's shelf order: Recents, Favorites, then GBA, GBC, GB. Empty shelves
/// are left out, as slot does. `recents` is newest first and keeps that
/// order; games no longer on the card are skipped.
pub fn shelves(
    carts: &[Cart],
    favourites: &Favourites,
    recents: &[(Platform, String)],
) -> Vec<(ShelfKind, Vec<Cart>)> {
    let mut out = Vec::new();
    let recent: Vec<Cart> = recents
        .iter()
        .filter_map(|(p, stem)| carts.iter().find(|c| c.platform == *p && c.stem == *stem))
        .cloned()
        .collect();
    if !recent.is_empty() {
        out.push((ShelfKind::Recents, recent));
    }
    let mut fav: Vec<Cart> = carts
        .iter()
        .filter(|c| favourites.contains(c.platform, &c.stem))
        .cloned()
        .collect();
    // slot-mini can list one stem twice on a system (a .gba and a .zip, or
    // the same name in two subfolders); keep the first, as slot keys by stem.
    fav.dedup_by(|a, b| a.platform == b.platform && a.stem == b.stem);
    fav.sort_by(|a, b| {
        (sort_key(&a.stem), a.platform as u8).cmp(&(sort_key(&b.stem), b.platform as u8))
    });
    if !fav.is_empty() {
        out.push((ShelfKind::Favourites, fav));
    }
    for p in Platform::ALL {
        let mut on: Vec<Cart> = carts.iter().filter(|c| c.platform == p).cloned().collect();
        on.dedup_by(|a, b| a.stem == b.stem);
        if !on.is_empty() {
            out.push((ShelfKind::System(p), on));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn cart(p: Platform, stem: &str) -> Cart {
        Cart {
            platform: p,
            stem: stem.into(),
            rom: PathBuf::from(format!("{stem}.x")),
            label: None,
            title: String::new(),
            code: String::new(),
            shell: None,
        }
    }

    #[test]
    fn order_is_favourites_gba_gbc_gb_and_empty_shelves_hide() {
        let carts = vec![
            cart(Platform::Gba, "B"),
            cart(Platform::Gb, "A"),
            cart(Platform::Gb, "Z"),
        ];
        let mut f = Favourites::default();
        let kinds = |f: &Favourites| {
            shelves(&carts, f, &[])
                .iter()
                .map(|s| s.0)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            kinds(&f),
            vec![
                ShelfKind::System(Platform::Gba),
                ShelfKind::System(Platform::Gb)
            ]
        );
        f.toggle(Platform::Gb, "Z");
        f.toggle(Platform::Gba, "B");
        let s = shelves(&carts, &f, &[]);
        assert_eq!(s[0].0, ShelfKind::Favourites);
        let stems: Vec<_> = s[0].1.iter().map(|c| c.stem.as_str()).collect();
        assert_eq!(stems, vec!["B", "Z"]);
    }

    #[test]
    fn recents_come_first_in_the_order_played() {
        let carts = vec![
            cart(Platform::Gba, "B"),
            cart(Platform::Gb, "A"),
            cart(Platform::Gb, "Z"),
        ];
        let played = [
            (Platform::Gb, "Z".to_string()),
            (Platform::Gba, "Gone".to_string()),
            (Platform::Gba, "B".to_string()),
        ];
        let s = shelves(&carts, &Favourites::default(), &played);
        assert_eq!(s[0].0, ShelfKind::Recents);
        let stems: Vec<_> = s[0].1.iter().map(|c| c.stem.as_str()).collect();
        assert_eq!(stems, vec!["Z", "B"], "newest first, missing games skipped");
        assert_eq!(s.len(), 3);
    }
}
