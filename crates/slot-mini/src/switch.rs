//! "Use slot-mini as the boot launcher": swap Onion's MainUI binaries for
//! the boot shim (package/shim.sh), and put them back.
//!
//! The device must never be left without a working launcher, whatever
//! crashes or power cuts happen part-way. So every step is ordered such that
//! stopping after it leaves either the original or the shim in place, and the
//! shim never without its original:
//!
//! 1. Back the original up to `.tmp_update/slot-mini/stock/<name>/MainUI`
//!    (written to a temporary name, flushed, renamed), and read it back.
//! 2. Only then write the shim beside the original and rename it over.
//!
//! Restoring copies the backup the same way. A file that is not the shim
//! (Onion was updated and put its own back) is never replaced by a restore,
//! and an install backs up whatever genuine binary is there now.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::paths::Paths;

pub const SHIM: &str = include_str!("../../../package/shim.sh");
/// How a shim is recognised.
pub const MARKER: &str = "# slot-mini boot shim";
/// Onion's MainUI binaries: Mini (283), Mini Plus (354), Mini Flip (285),
/// each in clean and Expert mode (runtime.sh `mount_main_ui`).
pub const VARIANTS: [&str; 6] = [
    "MainUI-283-clean",
    "MainUI-283-expert",
    "MainUI-354-clean",
    "MainUI-354-expert",
    "MainUI-285-clean",
    "MainUI-285-expert",
];

#[derive(Debug)]
pub enum SwitchError {
    /// No MainUI binary found in `.tmp_update/bin`: not an Onion card.
    NoOnion,
    /// The backup could not be made or did not read back the same.
    Backup(String),
    /// The shim is installed for this file but its original is gone.
    NoBackup(String),
    Io(String),
}

impl std::fmt::Display for SwitchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SwitchError::NoOnion => write!(f, "no Onion launcher found"),
            SwitchError::Backup(n) => write!(f, "could not back up {n}"),
            SwitchError::NoBackup(n) => write!(f, "the original {n} is missing"),
            SwitchError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl From<std::io::Error> for SwitchError {
    fn from(e: std::io::Error) -> Self {
        SwitchError::Io(e.to_string())
    }
}

fn bin(paths: &Paths) -> PathBuf {
    paths.sysdir().join("bin")
}

pub fn stock_dir(paths: &Paths) -> PathBuf {
    paths.sysdir().join("slot-mini/stock")
}

fn backup_of(paths: &Paths, name: &str) -> PathBuf {
    stock_dir(paths).join(name).join("MainUI")
}

/// Whether a file is the shim (by its marker line, not its exact bytes, so
/// an older shim still counts).
pub fn is_shim(path: &Path) -> bool {
    let Ok(mut f) = std::fs::File::open(path) else {
        return false;
    };
    let mut head = [0u8; 512];
    let n = std::io::Read::read(&mut f, &mut head).unwrap_or(0);
    String::from_utf8_lossy(&head[..n]).contains(MARKER)
}

/// MainUI binaries present on this card.
fn present(paths: &Paths) -> Vec<&'static str> {
    VARIANTS
        .into_iter()
        .filter(|n| bin(paths).join(n).is_file())
        .collect()
}

pub fn installed(paths: &Paths) -> bool {
    present(paths).iter().any(|n| is_shim(&bin(paths).join(n)))
}

fn sync() {
    // SAFETY: sync(2) takes no arguments and cannot fail.
    unsafe { libc::sync() };
}

fn set_exec(p: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755))
}

/// Write `bytes` to `path` without ever leaving a half-written `path`:
/// write a sibling, flush it, make it executable, rename it over.
fn put(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("slot-mini-new");
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    set_exec(&tmp)?;
    let done = std::fs::rename(&tmp, path);
    if done.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    done?;
    if let Some(dir) = path.parent() {
        if let Ok(d) = std::fs::File::open(dir) {
            let _ = d.sync_all();
        }
    }
    Ok(())
}

/// Replace every MainUI binary with the shim, backing each original up
/// first. Running it twice changes nothing the second time (the shim is
/// rewritten only if it differs).
pub fn install(paths: &Paths) -> Result<(), SwitchError> {
    let names = present(paths);
    if names.is_empty() {
        return Err(SwitchError::NoOnion);
    }
    for name in names {
        let target = bin(paths).join(name);
        let backup = backup_of(paths, name);
        if !is_shim(&target) {
            let original = std::fs::read(&target)?;
            if original.is_empty() {
                return Err(SwitchError::Backup(format!("{name} is empty")));
            }
            std::fs::create_dir_all(backup.parent().unwrap())?;
            put(&backup, &original).map_err(|e| SwitchError::Backup(format!("{name}: {e}")))?;
            if std::fs::read(&backup).ok().as_deref() != Some(&original[..]) {
                return Err(SwitchError::Backup(name.into()));
            }
        } else if !backup.is_file() {
            // A shim without its original: leave it be, and say so.
            return Err(SwitchError::NoBackup(name.into()));
        }
        if std::fs::read(&target).ok().as_deref() != Some(SHIM.as_bytes()) {
            put(&target, SHIM.as_bytes())?;
        }
    }
    sync();
    Ok(())
}

/// Put Onion's originals back wherever the shim is. Files that are not the
/// shim are left alone.
pub fn restore(paths: &Paths) -> Result<(), SwitchError> {
    let mut missing = None;
    for name in present(paths) {
        let target = bin(paths).join(name);
        if !is_shim(&target) {
            continue;
        }
        let backup = backup_of(paths, name);
        match std::fs::read(&backup) {
            Ok(bytes)
                if !bytes.is_empty()
                    && !String::from_utf8_lossy(&bytes[..bytes.len().min(512)])
                        .contains(MARKER) =>
            {
                put(&target, &bytes)?;
            }
            _ => missing = Some(name),
        }
    }
    sync();
    match missing {
        Some(name) => Err(SwitchError::NoBackup(name.into())),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card() -> (tempfile::TempDir, Paths) {
        let d = tempfile::tempdir().unwrap();
        let p = Paths::new(d.path().join("sd"), d.path().join("tmp"));
        std::fs::create_dir_all(bin(&p)).unwrap();
        for n in [
            "MainUI-354-clean",
            "MainUI-354-expert",
            "MainUI-283-clean",
            "MainUI-283-expert",
        ] {
            std::fs::write(bin(&p).join(n), format!("\x7fELF stock {n}")).unwrap();
        }
        (d, p)
    }

    #[test]
    fn install_backs_up_then_swaps_and_restore_puts_back() {
        let (_d, p) = card();
        assert!(!installed(&p));
        install(&p).unwrap();
        assert!(installed(&p));
        for n in ["MainUI-354-clean", "MainUI-283-expert"] {
            assert!(is_shim(&bin(&p).join(n)));
            assert_eq!(
                std::fs::read_to_string(backup_of(&p, n)).unwrap(),
                format!("\x7fELF stock {n}")
            );
        }
        assert!(
            !bin(&p).join("MainUI-285-clean").exists(),
            "absent variants are not created"
        );
        restore(&p).unwrap();
        assert!(!installed(&p));
        assert_eq!(
            std::fs::read_to_string(bin(&p).join("MainUI-354-clean")).unwrap(),
            "\x7fELF stock MainUI-354-clean"
        );
    }

    #[test]
    fn installing_twice_never_backs_up_the_shim() {
        let (_d, p) = card();
        install(&p).unwrap();
        install(&p).unwrap();
        let b = std::fs::read_to_string(backup_of(&p, "MainUI-354-clean")).unwrap();
        assert!(!b.contains(MARKER));
        restore(&p).unwrap();
        assert!(!installed(&p));
    }

    #[test]
    fn restore_leaves_a_binary_onion_put_back() {
        let (_d, p) = card();
        install(&p).unwrap();
        // An Onion update restores its own (newer) MainUI for one variant.
        std::fs::write(bin(&p).join("MainUI-354-clean"), "\x7fELF newer").unwrap();
        restore(&p).unwrap();
        assert_eq!(
            std::fs::read_to_string(bin(&p).join("MainUI-354-clean")).unwrap(),
            "\x7fELF newer"
        );
        // Installing again backs up the newer one.
        install(&p).unwrap();
        assert_eq!(
            std::fs::read_to_string(backup_of(&p, "MainUI-354-clean")).unwrap(),
            "\x7fELF newer"
        );
    }

    #[test]
    fn a_shim_whose_backup_vanished_is_reported_not_overwritten() {
        let (_d, p) = card();
        install(&p).unwrap();
        std::fs::remove_dir_all(stock_dir(&p).join("MainUI-354-clean")).unwrap();
        assert!(matches!(restore(&p), Err(SwitchError::NoBackup(_))));
        assert!(
            is_shim(&bin(&p).join("MainUI-354-clean")),
            "left the shim, which still falls back"
        );
        assert!(matches!(install(&p), Err(SwitchError::NoBackup(_))));
    }

    #[test]
    fn not_an_onion_card() {
        let d = tempfile::tempdir().unwrap();
        let p = Paths::new(d.path().join("sd"), d.path().join("tmp"));
        assert!(matches!(install(&p), Err(SwitchError::NoOnion)));
    }

    #[test]
    fn a_stale_temporary_from_a_crash_does_not_matter() {
        let (_d, p) = card();
        // A power cut left a half-written temporary beside the original.
        std::fs::write(bin(&p).join("MainUI-354-clean.slot-mini-new"), "half").unwrap();
        install(&p).unwrap();
        assert!(is_shim(&bin(&p).join("MainUI-354-clean")));
        restore(&p).unwrap();
        assert_eq!(
            std::fs::read_to_string(bin(&p).join("MainUI-354-clean")).unwrap(),
            "\x7fELF stock MainUI-354-clean"
        );
    }
}
