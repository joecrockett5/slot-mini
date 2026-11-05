//! slot's cartridge insert and eject clips.
//!
//! The clips are slot's own (`crates/slot/assets/{insert,eject}.pcm`: 16-bit
//! little-endian mono at 48 kHz). A static binary cannot use Onion's
//! `libpadsp.so` audio path, so on the device the clip is written once as a
//! WAV file and played by Onion's own `ffplay` with `libpadsp.so` preloaded
//! (see docs/PLATFORM.md, Audio). Off unless `sounds = on`.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

const INSERT: &[u8] = include_bytes!("../assets/insert.pcm");
const EJECT: &[u8] = include_bytes!("../assets/eject.pcm");
const RATE: u32 = 48_000;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Clip {
    Insert,
    Eject,
}

impl Clip {
    fn pcm(self) -> &'static [u8] {
        match self {
            Clip::Insert => INSERT,
            Clip::Eject => EJECT,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Clip::Insert => "insert.wav",
            Clip::Eject => "eject.wav",
        }
    }
}

pub trait Sound {
    fn play(&mut self, clip: Clip);
}

/// No sound.
pub struct Silent;

impl Sound for Silent {
    fn play(&mut self, _: Clip) {}
}

/// Remembers what was played, for tests.
#[derive(Default)]
pub struct Recorder(pub std::sync::Arc<std::sync::Mutex<Vec<Clip>>>);

impl Sound for Recorder {
    fn play(&mut self, clip: Clip) {
        self.0.lock().unwrap().push(clip);
    }
}

/// A RIFF/WAVE header and the samples.
pub fn wav(pcm: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(44 + pcm.len());
    let le32 = |v: u32| v.to_le_bytes();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&le32(36 + pcm.len() as u32));
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&le32(16));
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&le32(RATE));
    out.extend_from_slice(&le32(RATE * 2));
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&le32(pcm.len() as u32));
    out.extend_from_slice(pcm);
    out
}

/// Plays a clip by running an external player on a WAV file.
pub struct Player {
    program: PathBuf,
    args: Vec<String>,
    preload: Option<PathBuf>,
    /// Where the player runs (Onion's video player runs it from
    /// `.tmp_update`).
    cwd: Option<PathBuf>,
    dir: PathBuf,
    /// One waiter per clip: it reaps the player and logs a failure.
    playing: Vec<std::thread::JoinHandle<()>>,
}

impl Player {
    /// Onion's ffplay through libpadsp.so, with clips cached in `dir`.
    /// The player's own errors go to slot-mini's log (`-loglevel error`).
    pub fn onion(sd: &Path, dir: PathBuf) -> Player {
        Player {
            program: sd.join(".tmp_update/bin/ffplay"),
            args: ["-nodisp", "-autoexit", "-loglevel", "error"]
                .map(String::from)
                .to_vec(),
            preload: Some(sd.join("miyoo/lib/libpadsp.so")),
            cwd: Some(sd.join(".tmp_update")),
            dir,
            playing: Vec::new(),
        }
    }

    pub fn new(program: PathBuf, args: Vec<String>, dir: PathBuf) -> Player {
        Player {
            program,
            args,
            preload: None,
            cwd: None,
            dir,
            playing: Vec::new(),
        }
    }

    fn file(&self, clip: Clip) -> std::io::Result<PathBuf> {
        let path = self.dir.join(clip.name());
        let want = wav(clip.pcm());
        if std::fs::read(&path).map_or(true, |have| have != want) {
            std::fs::create_dir_all(&self.dir)?;
            slot_mini_store::atomic_write(&path, &want)?;
        }
        Ok(path)
    }
}

impl Sound for Player {
    fn play(&mut self, clip: Clip) {
        self.playing.retain(|w| !w.is_finished());
        if !self.program.is_file() {
            eprintln!("slot-mini: sound: no player at {}", self.program.display());
            return;
        }
        let file = match self.file(clip) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("slot-mini: sound: {e}");
                return;
            }
        };
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.args)
            .arg(&file)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            // The player's errors land in slot-mini's log.
            .stderr(Stdio::inherit());
        if let Some(p) = self.preload.as_ref().filter(|p| p.is_file()) {
            cmd.env("LD_PRELOAD", p);
        }
        if let Some(d) = self.cwd.as_ref().filter(|d| d.is_dir()) {
            cmd.current_dir(d);
        }
        let name = clip.name();
        match cmd.spawn() {
            Ok(child) => self
                .playing
                .push(std::thread::spawn(move || reap(child, name))),
            Err(e) => eprintln!("slot-mini: sound: {}: {e}", self.program.display()),
        }
    }
}

/// Wait for a player and log how it ended if it failed.
fn reap(mut child: Child, name: &str) {
    match child.wait() {
        Ok(s) if s.success() => {}
        Ok(s) => eprintln!("slot-mini: sound: the player failed on {name}: {s}"),
        Err(e) => eprintln!("slot-mini: sound: {name}: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_header_is_well_formed() {
        let w = wav(INSERT);
        assert_eq!(&w[..4], b"RIFF");
        assert_eq!(&w[8..16], b"WAVEfmt ");
        assert_eq!(u32::from_le_bytes(w[24..28].try_into().unwrap()), 48_000);
        assert_eq!(
            u32::from_le_bytes(w[40..44].try_into().unwrap()) as usize,
            INSERT.len()
        );
        assert_eq!(w.len(), 44 + INSERT.len());
    }

    #[test]
    fn the_player_runs_once_per_clip_with_the_wav() {
        let d = tempfile::tempdir().unwrap();
        let log = d.path().join("played");
        let script = d.path().join("player.sh");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\nfor a; do last=$a; done\necho \"$last\" >> {}\n",
                log.display()
            ),
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut p = Player::new(script, vec!["-q".into()], d.path().join("clips"));
        p.play(Clip::Insert);
        p.play(Clip::Eject);
        for w in p.playing.drain(..) {
            w.join().unwrap();
        }
        let played = std::fs::read_to_string(&log).unwrap();
        assert!(
            played.contains("insert.wav") && played.contains("eject.wav"),
            "{played}"
        );
        assert_eq!(
            std::fs::read(d.path().join("clips/insert.wav")).unwrap(),
            wav(INSERT)
        );
    }

    #[test]
    fn a_missing_player_is_silent() {
        let d = tempfile::tempdir().unwrap();
        let mut p = Player::new(d.path().join("nope"), vec![], d.path().join("clips"));
        p.play(Clip::Insert);
        assert!(p.playing.is_empty());
    }
}
