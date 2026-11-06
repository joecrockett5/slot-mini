//! Which Onion is on the card, told by what it can do rather than by its
//! version number, so slot-mini works on the stable release (4.3.1-1) and
//! the development build (the OTA updater's beta channel, the `latest`
//! release built from `main`) alike, and keeps working as either changes.
//! See docs/PLATFORM.md, "Onion versions".

use std::path::Path;

use crate::paths::Paths;

/// What the installed Onion offers that slot-mini can use.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Onion {
    /// The runtime starts a game fresh, without its auto save state, when
    /// `/tmp/reset_game` is there (`override_game_core`, development build).
    /// Stable Onion has no such flag: its Game List Options runs RetroArch
    /// directly with `reset.cfg` instead.
    pub reset_flag: bool,
    /// `gameSwitcher --overlay` opens the Game Switcher over a paused
    /// RetroArch (development build). Stable Onion's Game Switcher only runs
    /// after RetroArch has quit.
    pub switcher_overlay: bool,
    /// `.tmp_update/onionVersion/version.txt`, for the log.
    pub version: Option<String>,
}

fn contains(path: &Path, needle: &[u8]) -> bool {
    std::fs::read(path).is_ok_and(|b| b.windows(needle.len()).any(|w| w == needle))
}

impl Onion {
    pub fn detect(paths: &Paths) -> Onion {
        let sys = paths.sysdir();
        Onion {
            reset_flag: contains(&sys.join("runtime.sh"), b"/tmp/reset_game"),
            switcher_overlay: contains(&sys.join("bin/gameSwitcher"), b"--overlay"),
            version: std::fs::read_to_string(sys.join("onionVersion/version.txt"))
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty()),
        }
    }

    /// One line for the log.
    pub fn describe(&self) -> String {
        format!(
            "Onion {} (reset flag: {}, switcher overlay: {})",
            self.version.as_deref().unwrap_or("?"),
            if self.reset_flag { "yes" } else { "no" },
            if self.switcher_overlay { "yes" } else { "no" },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card() -> (tempfile::TempDir, Paths) {
        let d = tempfile::tempdir().unwrap();
        let paths = Paths::new(d.path().join("sd"), d.path().join("tmp"));
        std::fs::create_dir_all(paths.sysdir().join("bin")).unwrap();
        std::fs::create_dir_all(paths.sysdir().join("onionVersion")).unwrap();
        (d, paths)
    }

    #[test]
    fn stable_onion_offers_neither() {
        let (_d, paths) = card();
        // 4.3.1-1's runtime and Game Switcher, cut down.
        std::fs::write(
            paths.sysdir().join("runtime.sh"),
            "launch_game() {\n    if echo \"$cmd\" | grep -q \"$sysdir/reset.cfg\"; then\n",
        )
        .unwrap();
        std::fs::write(
            paths.sysdir().join("bin/gameSwitcher"),
            b"\x7fELF...gameSwitcher",
        )
        .unwrap();
        std::fs::write(paths.sysdir().join("onionVersion/version.txt"), "4.3.1-1\n").unwrap();
        let o = Onion::detect(&paths);
        assert_eq!(
            o,
            Onion {
                reset_flag: false,
                switcher_overlay: false,
                version: Some("4.3.1-1".into()),
            }
        );
    }

    #[test]
    fn the_development_build_offers_both() {
        let (_d, paths) = card();
        std::fs::write(
            paths.sysdir().join("runtime.sh"),
            "    if [ -f /tmp/reset_game ]; then\n        appendconfig=\"/tmp/reset.cfg\"\n",
        )
        .unwrap();
        std::fs::write(
            paths.sysdir().join("bin/gameSwitcher"),
            b"\x7fELF\0...\0--overlay\0...",
        )
        .unwrap();
        let o = Onion::detect(&paths);
        assert!(o.reset_flag && o.switcher_overlay);
        assert_eq!(o.version, None);
        assert!(o.describe().contains("reset flag: yes"));
    }

    #[test]
    fn no_onion_offers_nothing() {
        let d = tempfile::tempdir().unwrap();
        let paths = Paths::new(d.path().join("sd"), d.path().join("tmp"));
        assert_eq!(Onion::detect(&paths), Onion::default());
    }
}
