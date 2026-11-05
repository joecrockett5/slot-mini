//! Buttons from the Mini Plus's evdev device, `/dev/input/event0`, with the
//! key codes from Onion's `keymap_hw.h` (docs/PLATFORM.md, "Input").

use std::fs::File;
use std::io::Read;
use std::os::fd::AsRawFd;
use std::path::Path;

use crate::backend::{Button, Event};

const EV_KEY: u16 = 1;

/// Onion's hardware key map: Linux key code to button.
pub const KEYMAP: &[(u16, Button)] = &[
    (103, Button::Up),         // KEY_UP
    (108, Button::Down),       // KEY_DOWN
    (105, Button::Left),       // KEY_LEFT
    (106, Button::Right),      // KEY_RIGHT
    (57, Button::A),           // KEY_SPACE
    (29, Button::B),           // KEY_LEFTCTRL
    (42, Button::X),           // KEY_LEFTSHIFT
    (56, Button::Y),           // KEY_LEFTALT
    (18, Button::L1),          // KEY_E
    (20, Button::R1),          // KEY_T
    (15, Button::L2),          // KEY_TAB
    (14, Button::R2),          // KEY_BACKSPACE
    (97, Button::Select),      // KEY_RIGHTCTRL
    (28, Button::Start),       // KEY_ENTER
    (1, Button::Menu),         // KEY_ESC
    (116, Button::Power),      // KEY_POWER
    (115, Button::VolumeUp),   // KEY_VOLUMEUP
    (114, Button::VolumeDown), // KEY_VOLUMEDOWN
];

pub fn button(code: u16) -> Option<Button> {
    KEYMAP.iter().find(|(c, _)| *c == code).map(|(_, b)| *b)
}

/// Size of `struct input_event` on the device: a 32-bit ARM `timeval` (two
/// 32-bit fields), then type, code and value.
pub const EVENT_SIZE_32: usize = 16;
/// The same struct on a 64-bit host (64-bit `timeval`).
pub const EVENT_SIZE_64: usize = 24;

/// The size the running kernel uses.
pub const EVENT_SIZE: usize = std::mem::size_of::<libc::input_event>();

/// Turn raw event records into button events. Repeats (value 2) and other
/// event types are dropped: the shelf does its own key repeat, as slot does.
/// A trailing partial record is left in `buf` for the next read.
pub fn parse(buf: &mut Vec<u8>, size: usize) -> Vec<Event> {
    let mut out = Vec::new();
    let whole = buf.len() / size * size;
    for rec in buf[..whole].chunks_exact(size) {
        let t = size - 8;
        let kind = u16::from_le_bytes([rec[t], rec[t + 1]]);
        let code = u16::from_le_bytes([rec[t + 2], rec[t + 3]]);
        let value = i32::from_le_bytes(rec[t + 4..t + 8].try_into().unwrap());
        if kind != EV_KEY {
            continue;
        }
        let Some(b) = button(code) else {
            continue;
        };
        match value {
            1 => out.push(Event::Press(b)),
            0 => out.push(Event::Release(b)),
            _ => {}
        }
    }
    buf.drain(..whole);
    out
}

pub struct Evdev {
    file: File,
    buf: Vec<u8>,
    grabbed: bool,
}

/// `EVIOCGRAB`: `_IOW('E', 0x90, int)`.
const EVIOCGRAB: libc::c_ulong = 0x4004_4590;
/// Bytes in the kernel's key bitmap (`KEY_MAX` 0x2ff, one bit per key).
const KEY_BYTES: usize = 0x2ff / 8 + 1;
/// `EVIOCGKEY(len)`: `_IOC(_IOC_READ, 'E', 0x18, len)`.
const EVIOCGKEY: libc::c_ulong = 0x8000_0000 | ((KEY_BYTES as libc::c_ulong) << 16) | 0x4518;

/// The buttons in `KEYMAP` that are down in a key bitmap.
pub fn buttons_down(bits: &[u8]) -> Vec<Button> {
    KEYMAP
        .iter()
        .filter(|(code, _)| {
            let c = *code as usize;
            bits.get(c / 8).is_some_and(|b| b & (1 << (c % 8)) != 0)
        })
        .map(|(_, b)| *b)
        .collect()
}

/// Whether any key in `KEYMAP` is down in a key bitmap.
pub fn any_mapped_down(bits: &[u8]) -> bool {
    KEYMAP.iter().any(|(code, _)| {
        let c = *code as usize;
        bits.get(c / 8).is_some_and(|b| b & (1 << (c % 8)) != 0)
    })
}

impl Evdev {
    pub fn open(path: &Path) -> std::io::Result<Evdev> {
        let file = File::open(path)?;
        // SAFETY: setting O_NONBLOCK on our own descriptor.
        unsafe {
            let fd = file.as_raw_fd();
            let flags = libc::fcntl(fd, libc::F_GETFL);
            libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
        }
        Ok(Evdev {
            file,
            buf: Vec::new(),
            grabbed: false,
        })
    }

    /// Take the device for ourselves, or give it back. While grabbed, no
    /// other reader (the game, Onion's keymon) sees any button. Onion's keymon
    /// grabs the same way. Does nothing if already in that state.
    pub fn grab(&mut self, on: bool) {
        if on == self.grabbed {
            return;
        }
        // SAFETY: EVIOCGRAB takes an int by value. A regular file (tests)
        // cannot be grabbed; the call fails harmlessly there.
        unsafe { libc::ioctl(self.file.as_raw_fd(), EVIOCGRAB as _, on as libc::c_int) };
        self.grabbed = on;
    }

    pub fn grabbed(&self) -> bool {
        self.grabbed
    }

    /// The buttons held right now, from the kernel's own key state.
    pub fn down_buttons(&self) -> Option<Vec<Button>> {
        let mut bits = [0u8; KEY_BYTES];
        // SAFETY: EVIOCGKEY writes at most KEY_BYTES into `bits`.
        let r = unsafe { libc::ioctl(self.file.as_raw_fd(), EVIOCGKEY as _, bits.as_mut_ptr()) };
        (r >= 0).then(|| buttons_down(&bits))
    }

    /// Whether any button is held right now, from the kernel's own key
    /// state (`EVIOCGKEY`), so a lost release can never leave a button
    /// "held". `None` where the device cannot say (a regular file in tests).
    pub fn any_down(&self) -> Option<bool> {
        let mut bits = [0u8; KEY_BYTES];
        // SAFETY: EVIOCGKEY writes at most KEY_BYTES into `bits`.
        let r = unsafe { libc::ioctl(self.file.as_raw_fd(), EVIOCGKEY as _, bits.as_mut_ptr()) };
        (r >= 0).then(|| any_mapped_down(&bits))
    }

    /// Wait up to `timeout_ms` for input (negative waits forever), then
    /// return what arrived. Lets a background process sleep between presses.
    pub fn wait(&mut self, timeout_ms: i32) -> Vec<Event> {
        let mut fds = libc::pollfd {
            fd: self.file.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: one valid pollfd.
        unsafe { libc::poll(&mut fds, 1, timeout_ms) };
        self.poll()
    }

    /// Everything pressed or released since the last call. Never blocks.
    pub fn poll(&mut self) -> Vec<Event> {
        let mut chunk = [0u8; EVENT_SIZE * 32];
        loop {
            match self.file.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => self.buf.extend_from_slice(&chunk[..n]),
                Err(_) => break,
            }
        }
        parse(&mut self.buf, EVENT_SIZE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One record as the 32-bit device writes it.
    fn rec32(kind: u16, code: u16, value: i32) -> Vec<u8> {
        let mut r = vec![0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x08];
        r.extend_from_slice(&kind.to_le_bytes());
        r.extend_from_slice(&code.to_le_bytes());
        r.extend_from_slice(&value.to_le_bytes());
        r
    }

    #[test]
    fn recorded_device_bytes_become_buttons() {
        // A tap of A, a held Right with a repeat, a SYN, and an unknown key.
        let mut bytes = Vec::new();
        for (k, c, v) in [
            (1, 57, 1),
            (0, 0, 0),
            (1, 57, 0),
            (1, 106, 1),
            (1, 106, 2),
            (1, 106, 0),
            (1, 999, 1),
            (1, 1, 1),
        ] {
            bytes.extend(rec32(k, c, v));
        }
        let out = parse(&mut bytes, EVENT_SIZE_32);
        assert_eq!(
            out,
            vec![
                Event::Press(Button::A),
                Event::Release(Button::A),
                Event::Press(Button::Right),
                Event::Release(Button::Right),
                Event::Press(Button::Menu),
            ]
        );
        assert!(bytes.is_empty());
    }

    #[test]
    fn a_partial_record_waits_for_the_rest() {
        let full = rec32(1, 97, 1);
        let mut bytes = full[..10].to_vec();
        assert!(parse(&mut bytes, EVENT_SIZE_32).is_empty());
        bytes.extend_from_slice(&full[10..]);
        assert_eq!(
            parse(&mut bytes, EVENT_SIZE_32),
            vec![Event::Press(Button::Select)]
        );
    }

    #[test]
    fn held_keys_from_the_kernel_bitmap() {
        let mut bits = [0u8; KEY_BYTES];
        assert!(!any_mapped_down(&bits));
        // KEY_SPACE (57) is A.
        bits[57 / 8] |= 1 << (57 % 8);
        assert!(any_mapped_down(&bits));
        assert_eq!(buttons_down(&bits), vec![Button::A]);
        // An unmapped key (KEY_F1, 59) does not count.
        let mut bits = [0u8; KEY_BYTES];
        bits[59 / 8] |= 1 << (59 % 8);
        assert!(!any_mapped_down(&bits));
        assert!(buttons_down(&bits).is_empty());
        assert_eq!(EVIOCGKEY, 0x8060_4518);
    }

    #[test]
    fn every_onion_key_is_mapped_once() {
        let mut codes: Vec<u16> = KEYMAP.iter().map(|k| k.0).collect();
        codes.sort();
        codes.dedup();
        assert_eq!(codes.len(), KEYMAP.len());
        assert_eq!(button(15), Some(Button::L2));
        assert_eq!(button(14), Some(Button::R2));
        assert_eq!(button(116), Some(Button::Power));
    }

    #[test]
    fn reads_from_a_device_file() {
        let path = std::env::temp_dir().join(format!("slot-mini-ev-{}", std::process::id()));
        let mut bytes = Vec::new();
        // Records in the running kernel's layout.
        for (c, v) in [(18u16, 1i32), (18, 0)] {
            let mut r = vec![0u8; EVENT_SIZE - 8];
            r.extend_from_slice(&EV_KEY.to_le_bytes());
            r.extend_from_slice(&c.to_le_bytes());
            r.extend_from_slice(&v.to_le_bytes());
            bytes.extend(r);
        }
        std::fs::write(&path, bytes).unwrap();
        let mut ev = Evdev::open(&path).unwrap();
        assert_eq!(
            ev.poll(),
            vec![Event::Press(Button::L1), Event::Release(Button::L1)]
        );
        assert!(ev.poll().is_empty());
        let _ = std::fs::remove_file(path);
    }
}
