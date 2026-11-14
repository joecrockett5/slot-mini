//! What the PC app (crates/slot-mini-pc) asks of the shared app: it runs
//! games itself and stays alive around them, where the Miyoo's launcher
//! exits and is started again.

use std::path::PathBuf;

use slot_mini_gfx::Sprite;
use slot_mini_store::Cart;

use super::{App, Phase, NOT_STARTED};
use crate::library;
use crate::settings::{Recents, State};

impl App {
    /// A game is about to run: remember it the way `launch::hand_off` does
    /// on the Miyoo (the place on the shelf, and the PC's own Recents).
    pub fn game_started(&mut self, cart: &Cart) {
        let state = State {
            shelf: Some(self.shelf_key()),
            cart: Some((cart.platform, cart.stem.clone())),
            launched: false,
        };
        if let Err(e) = state.write(&self.paths) {
            eprintln!("slot-mini: state: {e}");
        }
        let mut played = Recents::read(&self.paths);
        played.played(cart.platform, &cart.stem);
        if let Err(e) = played.write(&self.paths) {
            eprintln!("slot-mini: recents.txt: {e}");
        }
    }

    /// The game has ended (or never started): the cart comes back out.
    pub fn game_ended(&mut self) {
        self.exit = None;
        let Phase::Inserting { stem, .. } = &self.phase else {
            return;
        };
        let stem = stem.clone();
        let keep = self.current().map(|(k, c)| (k, c.platform, c.stem.clone()));
        self.recents = Recents::read(&self.paths);
        self.restock();
        if let Some((kind, platform, stem)) = keep {
            // Recents now leads with this cart; stay on the shelf it was
            // played from.
            self.select(kind, platform, &stem);
        }
        self.phase = Phase::Ejecting {
            at: NOT_STARTED,
            stem,
        };
    }

    /// Look at the library again (the window came back to the front): new
    /// ROMs, labels and art show up without a restart. Only while the shelf
    /// is at rest.
    pub fn rescan(&mut self) -> bool {
        let quiet = self.phase == Phase::Shelf
            && self.studio.is_none()
            && self.about.is_none()
            && self.core.is_none();
        if !quiet {
            return false;
        }
        let carts = library::scan(&self.paths);
        if carts == self.carts {
            return false;
        }
        self.carts = carts;
        self.restock();
        self.refresh_view();
        true
    }

    /// RetroArch was found, moved or lost: where its cores are now.
    pub fn set_cores(&mut self, cores: Option<PathBuf>) {
        if let Some(pc) = self.paths.pc.as_mut() {
            pc.cores = cores;
        }
    }

    /// The wallpaper behind the shelf on screen, for the window's borders.
    pub fn wallpaper(&self) -> Option<(&Sprite, u64)> {
        let (tex, _) = self.ground()?;
        Some((self.gfx.get(tex)?, tex.raw() as u64))
    }
}
