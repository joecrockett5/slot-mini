// Copied from slot (https://github.com/BrandonKowalski/slot) at e66715d:
//   crates/slot-store/src/atomic.rs
// Changes: `AtomicFile` added, for files too large to hold in memory
//   (slot-mini's sync streams ROMs through it).
// Copyright (C) Brandon Kowalski and slot contributors. GPL-3.0-or-later.

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static SEQ: AtomicU64 = AtomicU64::new(0);

pub fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = temp_path(path);
    match write_then_rename(&tmp, path, bytes) {
        Ok(()) => {}
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            return Err(e);
        }
    }
    sync_dir(path);
    Ok(())
}

pub(crate) fn sync_dir(path: &Path) {
    if let Some(dir) = path.parent() {
        if let Ok(d) = File::open(dir) {
            let _ = d.sync_all();
            #[cfg(target_os = "linux")]
            unsafe {
                libc::syncfs(std::os::fd::AsRawFd::as_raw_fd(&d));
            }
        }
    }
}

fn write_then_rename(tmp: &Path, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut f = File::create(tmp)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    drop(f);
    std::fs::rename(tmp, path)
}

const NAME_MAX: usize = 255;

fn temp_path(path: &Path) -> PathBuf {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let tail = format!(".{}.{seq}.tmp", std::process::id());
    let mut room = NAME_MAX.saturating_sub(tail.len() + 1).min(name.len());
    while room > 0 && !name.is_char_boundary(room) {
        room -= 1;
    }
    let tmp = format!(".{}{tail}", &name[..room]);
    match path.parent() {
        Some(dir) => dir.join(tmp),
        None => PathBuf::from(tmp),
    }
}

pub fn sweep_temps(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        match entry.file_type() {
            Ok(t) if t.is_dir() => sweep_temps(&path),
            Ok(t) if t.is_file() && left_by_another_run(&entry.file_name().to_string_lossy()) => {
                if let Err(e) = std::fs::remove_file(&path) {
                    eprintln!("slot: temp sweep: {}: {e}", path.display());
                }
            }
            _ => {}
        }
    }
}

fn left_by_another_run(name: &str) -> bool {
    let Some(rest) = name.strip_prefix('.').and_then(|n| n.strip_suffix(".tmp")) else {
        return false;
    };
    let mut parts = rest.rsplitn(3, '.');
    let (Some(seq), Some(pid), Some(_)) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    digits(seq) && digits(pid) && pid != std::process::id().to_string()
}

/// A file written piece by piece beside its destination and renamed into
/// place by [`AtomicFile::commit`]: [`atomic_write`] for content that is
/// streamed. Dropped without a commit, the temporary file is removed.
pub struct AtomicFile {
    tmp: PathBuf,
    path: PathBuf,
    file: Option<File>,
}

impl AtomicFile {
    pub fn create(path: &Path) -> std::io::Result<AtomicFile> {
        let tmp = temp_path(path);
        let file = File::create(&tmp)?;
        Ok(AtomicFile {
            tmp,
            path: path.to_path_buf(),
            file: Some(file),
        })
    }

    /// Where the file will be once committed.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// fsync, rename over the destination, and sync the folder.
    pub fn commit(mut self) -> std::io::Result<()> {
        if let Some(f) = self.file.take() {
            f.sync_all()?;
        }
        std::fs::rename(&self.tmp, &self.path)?;
        sync_dir(&self.path);
        Ok(())
    }
}

impl Write for AtomicFile {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match &mut self.file {
            Some(f) => f.write(buf),
            None => Ok(0),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match &mut self.file {
            Some(f) => f.flush(),
            None => Ok(()),
        }
    }
}

impl Drop for AtomicFile {
    fn drop(&mut self) {
        // Still set: never committed (or the commit failed before renaming).
        if self.file.take().is_some() || self.tmp.exists() {
            let _ = std::fs::remove_file(&self.tmp);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_atomic_file_appears_only_when_committed() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("rom.gba");
        std::fs::write(&path, b"old").unwrap();
        let mut f = AtomicFile::create(&path).unwrap();
        f.write_all(b"new ").unwrap();
        f.write_all(b"content").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"old");
        f.commit().unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"new content");
        assert_eq!(std::fs::read_dir(d.path()).unwrap().count(), 1);
    }

    #[test]
    fn a_dropped_atomic_file_leaves_nothing() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("rom.gba");
        let mut f = AtomicFile::create(&path).unwrap();
        f.write_all(b"half").unwrap();
        drop(f);
        assert_eq!(std::fs::read_dir(d.path()).unwrap().count(), 0);
    }
}
