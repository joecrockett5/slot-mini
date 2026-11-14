//! slot's insert and eject clips on the PC's speakers. The clips are mono
//! 16-bit PCM at 48 kHz; `Mixer` adds whichever are playing into the
//! device's buffer, and on Windows a tinyaudio output device (WASAPI)
//! pulls from it. Elsewhere there is no device and the app is silent.

use std::sync::{Arc, Mutex};

use slot_mini::sound::{Clip, Sound, RATE};

const CHANNELS: usize = 2;

/// The clips being played and how far each has got, in samples.
#[derive(Clone, Default)]
pub struct Mixer(Arc<Mutex<Vec<(Clip, usize)>>>);

impl Mixer {
    pub fn play(&self, clip: Clip) {
        if let Ok(mut voices) = self.0.lock() {
            // The same clip again starts over instead of doubling up.
            voices.retain(|(c, _)| *c != clip);
            voices.push((clip, 0));
        }
    }

    /// Whether anything is still sounding.
    pub fn busy(&self) -> bool {
        self.0.lock().map(|v| !v.is_empty()).unwrap_or(false)
    }

    /// Fill the device's next buffer: interleaved frames of `channels`
    /// samples at the clips' own rate.
    pub fn fill(&self, out: &mut [f32], channels: usize) {
        out.fill(0.0);
        let Ok(mut voices) = self.0.lock() else {
            return;
        };
        for (clip, at) in voices.iter_mut() {
            let pcm = clip.pcm();
            for frame in out.chunks_exact_mut(channels) {
                let Some(bytes) = pcm.get(*at * 2..*at * 2 + 2) else {
                    break;
                };
                let s = i16::from_le_bytes([bytes[0], bytes[1]]) as f32 / 32768.0;
                for ch in frame {
                    *ch = (*ch + s).clamp(-1.0, 1.0);
                }
                *at += 1;
            }
        }
        voices.retain(|(clip, at)| *at * 2 < clip.pcm().len());
    }
}

/// The PC's speakers, opened the first time they are wanted.
#[derive(Default)]
pub struct Speakers {
    mixer: Mixer,
    #[cfg(windows)]
    device: Option<tinyaudio::OutputDevice>,
    failed: Option<String>,
    played: bool,
}

impl Speakers {
    pub fn new() -> Speakers {
        Speakers::default()
    }

    #[cfg(windows)]
    fn open(&mut self) {
        if self.device.is_some() || self.failed.is_some() {
            return;
        }
        let params = tinyaudio::OutputDeviceParameters {
            channels_count: CHANNELS,
            sample_rate: RATE as usize,
            // 10 ms: the click lands with the cart.
            channel_sample_count: RATE as usize / 100,
        };
        let mixer = self.mixer.clone();
        match tinyaudio::run_output_device(params, move |data| mixer.fill(data, CHANNELS)) {
            Ok(device) => self.device = Some(device),
            Err(e) => {
                eprintln!("slot-mini-pc: sound: {e}");
                self.failed = Some("no sound device".into());
            }
        }
    }

    #[cfg(not(windows))]
    fn open(&mut self) {
        let _ = (CHANNELS, RATE);
        self.failed = Some("no sound on this system".into());
    }
}

impl Sound for Speakers {
    fn play(&mut self, clip: Clip) {
        self.open();
        self.played = true;
        self.mixer.play(clip);
    }

    fn prepare(&mut self) {
        self.open();
    }

    fn close(&mut self) {
        #[cfg(windows)]
        {
            self.device = None;
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
    fn a_clip_is_mixed_onto_both_channels_and_ends() {
        let mixer = Mixer::default();
        let mut buf = vec![1.0f32; 960];
        mixer.fill(&mut buf, 2);
        assert!(buf.iter().all(|s| *s == 0.0), "silence when nothing plays");
        mixer.play(Clip::Insert);
        let samples = Clip::Insert.pcm().len() / 2;
        let mut heard = 0usize;
        let mut loud = false;
        while mixer.busy() {
            mixer.fill(&mut buf, 2);
            assert!(buf.chunks_exact(2).all(|f| f[0] == f[1]));
            loud |= buf.iter().any(|s| s.abs() > 0.05);
            heard += buf.len() / 2;
            assert!(heard < samples + 960, "the clip never ended");
        }
        assert!(loud && heard >= samples);
    }

    #[test]
    fn playing_a_clip_again_starts_it_over() {
        let mixer = Mixer::default();
        mixer.play(Clip::Eject);
        mixer.fill(&mut [0.0; 64], 2);
        mixer.play(Clip::Eject);
        assert_eq!(mixer.0.lock().unwrap().as_slice(), &[(Clip::Eject, 0)]);
    }
}
