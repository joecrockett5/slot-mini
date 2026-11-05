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

    /// slot's `Sfx::lead`: seconds of the clip before its click, so the
    /// click can land on the moment it belongs to (the insert's lands as
    /// the cart seats).
    pub fn lead(self) -> f32 {
        match self {
            Clip::Insert => 0.097,
            Clip::Eject => 0.021,
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
    /// Get ready to play with little delay (see `Player::prepare`). Called
    /// often; cheap when already ready.
    fn prepare(&mut self) {}
    /// Let go of anything kept ready (before exiting, or sounds off).
    fn close(&mut self) {}
    /// How the last clip went, once known, in a few words for the screen
    /// (MENU shows it after turning sounds on).
    fn report(&self) -> Option<String> {
        None
    }
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

/// The library path Onion's runtime exports for every app and game
/// (`runtime.sh`, 4.3.1-1). `ffplay`'s ffmpeg libraries are in
/// `.tmp_update/lib/parasyte`, which the narrower path Onion starts MainUI
/// with (`miyoo/lib:/config/lib:/lib`) leaves out: as the boot launcher,
/// slot-mini runs as MainUI, and `ffplay` then failed to load (exit 127,
/// seen on a device with 0.4.4).
pub fn onion_library_path(sd: &Path) -> String {
    let sd = sd.display();
    format!("/lib:/config/lib:{sd}/miyoo/lib:{sd}/.tmp_update/lib:{sd}/.tmp_update/lib/parasyte")
}

/// Plays a clip by running an external player on a WAV file.
pub struct Player {
    program: PathBuf,
    args: Vec<String>,
    preload: Option<PathBuf>,
    /// The library path the player needs (see `onion_library_path`).
    libraries: Option<String>,
    /// Where the player runs (Onion's video player runs it from
    /// `.tmp_update`).
    cwd: Option<PathBuf>,
    dir: PathBuf,
    /// One waiter per clip: it reaps the player and logs a failure.
    playing: Vec<std::thread::JoinHandle<()>>,
    /// How the last clip went (see `report`).
    last: std::sync::Arc<std::sync::Mutex<Option<String>>>,
    /// Where a player kept ready waits for its clip (a FIFO per player),
    /// if this player does that (see `prepare`).
    warm: Option<PathBuf>,
    armed: Option<Armed>,
    seq: u32,
}

/// A player started ahead of time, waiting on its FIFO for a clip.
struct Armed {
    fifo: PathBuf,
    pid: u32,
    /// When its clip was handed over (its report times from then).
    handed: std::sync::Arc<std::sync::Mutex<Option<std::time::Instant>>>,
}

/// How long a clip waits for a player still starting before giving up.
const READY_WAIT: std::time::Duration = std::time::Duration::from_secs(2);

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
            libraries: Some(onion_library_path(sd)),
            dir,
            playing: Vec::new(),
            last: Default::default(),
            warm: None,
            armed: None,
            seq: 0,
        }
    }

    /// Keep a player ready in `fifo_dir` (see `prepare`). On the device this
    /// is `/tmp`, a tmpfs, where FIFOs can be made.
    pub fn warm(mut self, fifo_dir: PathBuf) -> Player {
        self.warm = Some(fifo_dir);
        self
    }

    pub fn new(program: PathBuf, args: Vec<String>, dir: PathBuf) -> Player {
        Player {
            program,
            args,
            preload: None,
            cwd: None,
            libraries: None,
            dir,
            playing: Vec::new(),
            last: Default::default(),
            warm: None,
            armed: None,
            seq: 0,
        }
    }

    fn command(&self, input: &Path, extra: &[&str]) -> Command {
        use std::os::unix::process::CommandExt;
        let mut cmd = Command::new(&self.program);
        // Its own process group, so stopping it stops anything it started.
        cmd.process_group(0)
            .args(&self.args)
            .args(extra)
            .arg(input)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            // Read by the waiter: copied to slot-mini's log, and its first
            // line reported on screen if the player fails.
            .stderr(Stdio::piped());
        if let Some(p) = self.preload.as_ref().filter(|p| p.is_file()) {
            cmd.env("LD_PRELOAD", p);
        }
        if let Some(l) = &self.libraries {
            cmd.env("LD_LIBRARY_PATH", l);
        }
        if let Some(d) = self.cwd.as_ref().filter(|d| d.is_dir()) {
            cmd.current_dir(d);
        }
        cmd
    }

    fn set_last(&self, s: Option<String>) {
        if let Ok(mut l) = self.last.lock() {
            *l = s;
        }
    }

    /// Players left waiting by an earlier slot-mini that did not get to
    /// close them (a crash): stop them and remove their FIFOs.
    fn clear_stale(dir: &Path) {
        let mine = std::process::id();
        let needle = dir.to_string_lossy().into_owned();
        if let Ok(procs) = std::fs::read_dir("/proc") {
            for p in procs.flatten() {
                let Some(pid) = p.file_name().to_str().and_then(|n| n.parse::<i32>().ok()) else {
                    continue;
                };
                if pid as u32 == mine {
                    continue;
                }
                let cmdline = std::fs::read(p.path().join("cmdline")).unwrap_or_default();
                if String::from_utf8_lossy(&cmdline).contains(&needle) {
                    // SAFETY: signalling another process.
                    unsafe { libc::kill(pid, libc::SIGTERM) };
                }
            }
        }
        if let Ok(entries) = std::fs::read_dir(dir) {
            for e in entries.flatten() {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }

    /// Start a player now, waiting on a FIFO, so that a clip later starts
    /// at once: Onion's `ffplay` takes a while to load its libraries from
    /// the SD card, which made the sounds late.
    fn arm(&mut self) {
        let Some(dir) = self.warm.clone() else {
            return;
        };
        if self.armed.is_some() || !self.program.is_file() {
            return;
        }
        if self.seq == 0 {
            Player::clear_stale(&dir);
        }
        if let Err(e) = std::fs::create_dir_all(&dir) {
            eprintln!("slot-mini: sound: {}: {e}", dir.display());
            self.warm = None;
            return;
        }
        let fifo = dir.join(format!("{}-{}.wav", std::process::id(), self.seq));
        self.seq += 1;
        let _ = std::fs::remove_file(&fifo);
        let Ok(c) = std::ffi::CString::new(fifo.to_string_lossy().into_owned()) else {
            return;
        };
        // SAFETY: a NUL-terminated path.
        if unsafe { libc::mkfifo(c.as_ptr(), 0o600) } != 0 {
            let e = std::io::Error::last_os_error();
            eprintln!(
                "slot-mini: sound: no FIFO at {} ({e}); clips start cold",
                fifo.display()
            );
            self.warm = None;
            return;
        }
        match self.command(&fifo, &["-f", "wav"]).spawn() {
            Ok(child) => {
                let pid = child.id();
                let handed = std::sync::Arc::new(std::sync::Mutex::new(None));
                let (last, h) = (self.last.clone(), handed.clone());
                self.playing
                    .push(std::thread::spawn(move || reap(child, "clip", &last, &h)));
                self.armed = Some(Armed { fifo, pid, handed });
            }
            Err(e) => {
                let _ = std::fs::remove_file(&fifo);
                self.set_last(Some(format!("could not start the player: {e}")));
            }
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
    fn report(&self) -> Option<String> {
        self.last.lock().ok()?.clone()
    }

    fn prepare(&mut self) {
        self.arm();
    }

    fn close(&mut self) {
        if let Some(a) = self.armed.take() {
            // SAFETY: signalling our own child's process group.
            unsafe { libc::kill(-(a.pid as i32), libc::SIGTERM) };
            let _ = std::fs::remove_file(&a.fifo);
        }
    }

    fn play(&mut self, clip: Clip) {
        self.playing.retain(|w| !w.is_finished());
        if let Some(a) = self.armed.take() {
            self.set_last(None);
            if let Ok(mut h) = a.handed.lock() {
                *h = Some(std::time::Instant::now());
            }
            let bytes = wav(clip.pcm());
            let last = self.last.clone();
            self.playing
                .push(std::thread::spawn(move || hand_over(a, &bytes, &last)));
            return;
        }
        let say = |s: String| {
            eprintln!("slot-mini: sound: {s}");
            if let Ok(mut l) = self.last.lock() {
                *l = Some(s);
            }
        };
        if let Ok(mut l) = self.last.lock() {
            *l = None;
        }
        if !self.program.is_file() {
            eprintln!("slot-mini: sound: no player at {}", self.program.display());
            if let Ok(mut l) = self.last.lock() {
                *l = Some("ffplay not found".into());
            }
            return;
        }
        let file = match self.file(clip) {
            Ok(f) => f,
            Err(e) => return say(format!("could not write the clip: {e}")),
        };
        let name = clip.name();
        match self.command(&file, &[]).spawn() {
            Ok(child) => {
                let last = self.last.clone();
                let handed = std::sync::Arc::new(std::sync::Mutex::new(None));
                self.playing.push(std::thread::spawn(move || {
                    reap(child, name, &last, &handed)
                }));
            }
            Err(e) => say(format!("could not start the player: {e}")),
        }
    }
}

/// Write a clip into a waiting player's FIFO. The player may still be
/// starting (not yet reading): wait for it a little, then give up and stop
/// it, so nothing is left waiting for ever.
fn hand_over(a: Armed, bytes: &[u8], last: &std::sync::Mutex<Option<String>>) {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let started = std::time::Instant::now();
    let file = loop {
        // Non-blocking: fails at once (ENXIO) while nobody reads yet.
        match std::fs::OpenOptions::new()
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(&a.fifo)
        {
            Ok(f) => break Some(f),
            Err(_) if started.elapsed() < READY_WAIT => {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(e) => {
                eprintln!("slot-mini: sound: the player never got ready: {e}");
                break None;
            }
        }
    };
    let _ = std::fs::remove_file(&a.fifo);
    let Some(mut file) = file else {
        // SAFETY: signalling our own child.
        unsafe { libc::kill(-(a.pid as i32), libc::SIGTERM) };
        if let Ok(mut l) = last.lock() {
            l.get_or_insert_with(|| "the player never got ready".into());
        }
        return;
    };
    // Back to blocking writes: the clip is bigger than a pipe holds.
    // SAFETY: changing flags on our own descriptor.
    unsafe {
        use std::os::fd::AsRawFd;
        let fd = file.as_raw_fd();
        let flags = libc::fcntl(fd, libc::F_GETFL);
        libc::fcntl(fd, libc::F_SETFL, flags & !libc::O_NONBLOCK);
    }
    if let Err(e) = file.write_all(bytes) {
        eprintln!("slot-mini: sound: handing the clip over: {e}");
    }
}

/// Wait for a player, copy what it printed to the log, and record how it
/// ended: the exit status and its first line of output if it failed. A
/// player kept ready times from when its clip was handed over.
fn reap(
    mut child: Child,
    name: &str,
    last: &std::sync::Mutex<Option<String>>,
    handed: &std::sync::Mutex<Option<std::time::Instant>>,
) {
    use std::io::Read;
    let spawned = std::time::Instant::now();
    let mut said = String::new();
    if let Some(mut err) = child.stderr.take() {
        let _ = err.read_to_string(&mut said);
    }
    for line in said.lines().filter(|l| !l.trim().is_empty()) {
        eprintln!("slot-mini: sound: player: {line}");
    }
    let first = said
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    let started = handed.lock().ok().and_then(|h| *h).unwrap_or(spawned);
    let secs = started.elapsed().as_secs_f32();
    let report = match child.wait() {
        Ok(s) if s.success() => format!("player finished ({secs:.1} s)"),
        Ok(s) => {
            let how = match s.code() {
                Some(c) => format!("exit {c}"),
                None => "killed".to_string(),
            };
            eprintln!("slot-mini: sound: the player failed on {name}: {s}");
            match first.is_empty() {
                true => format!("player failed, {how}"),
                false => format!("{how}: {first}"),
            }
        }
        Err(e) => format!("{name}: {e}"),
    };
    if let Ok(mut l) = last.lock() {
        *l = Some(report);
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
        assert!(p.report().unwrap().starts_with("player finished ("));
    }

    /// A stand-in player: copies its input (the last argument) to `out`.
    fn copier(dir: &Path, out: &Path) -> PathBuf {
        let script = dir.join("player.sh");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\nfor a; do last=$a; done\ncat \"$last\" > {}\n",
                out.display()
            ),
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        script
    }

    fn alive(pid: u32) -> bool {
        // A reaped child is gone; /proc/<pid> may linger briefly as a zombie.
        std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .is_ok_and(|s| s.split_whitespace().nth(2).is_none_or(|st| st != "Z"))
    }

    #[test]
    fn a_player_kept_ready_gets_its_clip_through_a_fifo() {
        let d = tempfile::tempdir().unwrap();
        let out = d.path().join("heard");
        let fifos = d.path().join("fifos");
        let mut p =
            Player::new(copier(d.path(), &out), vec![], d.path().join("clips")).warm(fifos.clone());
        p.prepare();
        p.prepare();
        let armed = p.armed.as_ref().expect("a player is waiting");
        assert!(armed.fifo.starts_with(&fifos));
        p.play(Clip::Insert);
        assert!(p.armed.is_none(), "used");
        for w in p.playing.drain(..) {
            w.join().unwrap();
        }
        assert_eq!(std::fs::read(&out).unwrap(), wav(INSERT));
        assert!(p.report().unwrap().starts_with("player finished ("));
        assert_eq!(
            std::fs::read_dir(&fifos).unwrap().count(),
            0,
            "FIFO removed"
        );
        // And again with the next one.
        p.prepare();
        p.play(Clip::Eject);
        for w in p.playing.drain(..) {
            w.join().unwrap();
        }
        assert_eq!(std::fs::read(&out).unwrap(), wav(EJECT));
    }

    #[test]
    fn closing_stops_a_waiting_player() {
        let d = tempfile::tempdir().unwrap();
        let fifos = d.path().join("fifos");
        let mut p = Player::new(
            copier(d.path(), &d.path().join("heard")),
            vec![],
            d.path().join("c"),
        )
        .warm(fifos.clone());
        p.prepare();
        let pid = p.armed.as_ref().unwrap().pid;
        assert!(alive(pid));
        p.close();
        for w in p.playing.drain(..) {
            w.join().unwrap();
        }
        assert!(!alive(pid));
        assert_eq!(std::fs::read_dir(&fifos).unwrap().count(), 0);
    }

    #[test]
    fn players_left_by_a_crash_are_stopped() {
        let d = tempfile::tempdir().unwrap();
        let fifos = d.path().join("fifos");
        std::fs::create_dir_all(&fifos).unwrap();
        std::fs::write(fifos.join("1-0.wav"), "").unwrap();
        // A process whose command line names the FIFO folder.
        let mut stale = std::process::Command::new("sh")
            .args(["-c", "sleep 30", "x"])
            .arg(fifos.join("1-0.wav"))
            .spawn()
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));
        let mut p = Player::new(
            copier(d.path(), &d.path().join("heard")),
            vec![],
            d.path().join("c"),
        )
        .warm(fifos.clone());
        p.prepare();
        let status = stale.wait().unwrap();
        assert!(!status.success(), "stopped");
        assert!(!fifos.join("1-0.wav").exists());
        p.close();
    }

    #[test]
    fn onions_player_gets_onions_library_path() {
        assert_eq!(
            onion_library_path(Path::new("/mnt/SDCARD")),
            "/lib:/config/lib:/mnt/SDCARD/miyoo/lib:/mnt/SDCARD/.tmp_update/lib:/mnt/SDCARD/.tmp_update/lib/parasyte"
        );
        // The player is started with it, whatever slot-mini was given.
        let d = tempfile::tempdir().unwrap();
        let sd = d.path().join("sd");
        let bin = sd.join(".tmp_update/bin");
        std::fs::create_dir_all(&bin).unwrap();
        let log = d.path().join("env");
        let script = bin.join("ffplay");
        std::fs::write(
            &script,
            format!("#!/bin/sh\necho \"$LD_LIBRARY_PATH\" > {}\n", log.display()),
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut p = Player::onion(&sd, d.path().join("clips"));
        p.play(Clip::Insert);
        for w in p.playing.drain(..) {
            w.join().unwrap();
        }
        assert_eq!(
            std::fs::read_to_string(&log).unwrap().trim(),
            onion_library_path(&sd)
        );
    }

    #[test]
    fn a_missing_player_is_silent_and_says_so() {
        let d = tempfile::tempdir().unwrap();
        let mut p = Player::new(d.path().join("nope"), vec![], d.path().join("clips"));
        p.play(Clip::Insert);
        assert!(p.playing.is_empty());
        assert_eq!(p.report().as_deref(), Some("ffplay not found"));
    }

    #[test]
    fn a_failing_player_is_reported_with_its_first_line() {
        let d = tempfile::tempdir().unwrap();
        let script = d.path().join("player.sh");
        std::fs::write(
            &script,
            "#!/bin/sh\necho 'SDL_OpenAudio: No available audio device' >&2\necho more >&2\nexit 1\n",
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut p = Player::new(script, vec![], d.path().join("clips"));
        p.play(Clip::Insert);
        for w in p.playing.drain(..) {
            w.join().unwrap();
        }
        assert_eq!(
            p.report().as_deref(),
            Some("exit 1: SDL_OpenAudio: No available audio device")
        );
    }
}
