//! slot's insert and eject clips on the PC's speakers. On Windows each
//! clip is handed to the system as a WAV held in memory (`PlaySound`,
//! asynchronous): no device is opened and no thread is kept, so there is
//! nothing to wait for when sound is turned off or the app closes.
//! Elsewhere there is no device and the app is silent.

use std::sync::OnceLock;

use slot_mini::sound::{wav, Clip, Sound};

/// The clip as a WAV file, built once and kept for the life of the
/// program: the system plays from this memory after `play` returns.
pub fn clip_wav(clip: Clip) -> &'static [u8] {
    static INSERT: OnceLock<Vec<u8>> = OnceLock::new();
    static EJECT: OnceLock<Vec<u8>> = OnceLock::new();
    let cell = match clip {
        Clip::Insert => &INSERT,
        Clip::Eject => &EJECT,
    };
    cell.get_or_init(|| wav(clip.pcm()))
}

/// Start `wav` playing, cutting off whatever was. False when the system
/// would not take it.
#[cfg(windows)]
fn start(wav: &'static [u8]) -> bool {
    use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};
    // SAFETY: with SND_MEMORY the first argument is a WAV image, which
    // lives for the whole program; SND_ASYNC returns at once.
    unsafe {
        PlaySoundW(
            wav.as_ptr().cast(),
            std::ptr::null_mut(),
            SND_MEMORY | SND_ASYNC | SND_NODEFAULT,
        ) != 0
    }
}

/// Stop whatever is playing.
#[cfg(windows)]
fn stop() {
    use windows_sys::Win32::Media::Audio::PlaySoundW;
    // SAFETY: a null sound stops the one playing.
    unsafe {
        PlaySoundW(std::ptr::null(), std::ptr::null_mut(), 0);
    }
}

#[cfg(not(windows))]
fn start(_wav: &'static [u8]) -> bool {
    false
}

#[cfg(not(windows))]
fn stop() {}

/// The PC's speakers.
#[derive(Default)]
pub struct Speakers {
    failed: Option<String>,
    played: bool,
}

impl Speakers {
    pub fn new() -> Speakers {
        Speakers::default()
    }
}

impl Sound for Speakers {
    fn play(&mut self, clip: Clip) {
        self.played = true;
        self.failed = (!start(clip_wav(clip))).then(|| {
            if cfg!(windows) {
                "no sound device".to_string()
            } else {
                "no sound on this system".to_string()
            }
        });
    }

    fn prepare(&mut self) {
        // Build the WAVs ahead of the first click.
        clip_wav(Clip::Insert);
        clip_wav(Clip::Eject);
    }

    fn close(&mut self) {
        if self.played {
            stop();
        }
        self.failed = None;
    }

    fn report(&self) -> Option<String> {
        if !self.played {
            return None;
        }
        Some(self.failed.clone().unwrap_or_else(|| "played".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_clip_is_a_whole_wav_built_once() {
        for clip in [Clip::Insert, Clip::Eject] {
            let w = clip_wav(clip);
            assert_eq!(&w[..4], b"RIFF");
            assert_eq!(&w[8..12], b"WAVE");
            assert_eq!(w.len(), 44 + clip.pcm().len());
            let riff = u32::from_le_bytes([w[4], w[5], w[6], w[7]]) as usize;
            assert_eq!(riff, w.len() - 8);
            assert_eq!(w.as_ptr(), clip_wav(clip).as_ptr(), "built once");
        }
    }

    #[test]
    fn closing_never_waits_and_can_be_repeated() {
        let mut s = Speakers::new();
        s.prepare();
        s.close();
        s.play(Clip::Insert);
        s.close();
        s.close();
        assert!(s.report().is_some());
    }
}
