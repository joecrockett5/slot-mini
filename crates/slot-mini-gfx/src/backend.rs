//! Where finished frames go, and where button presses come from.

use std::path::Path;

use crate::canvas::Canvas;

/// Buttons, named for the Mini Plus.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Button {
    Up,
    Down,
    Left,
    Right,
    A,
    B,
    X,
    Y,
    L1,
    R1,
    L2,
    R2,
    Select,
    Start,
    Menu,
    Power,
    /// The side volume keys: Onion handles them; slot-mini only notes them
    /// (a MENU combination with one is not a MENU tap).
    VolumeUp,
    VolumeDown,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Event {
    Press(Button),
    Release(Button),
    /// The window was closed or the process was asked to stop.
    Quit,
}

/// A display and its input. The app polls events, draws, then presents.
pub trait Backend {
    fn size(&self) -> (u32, u32);
    /// Show a finished frame. Blocks for vsync where the backend has one.
    fn present(&mut self, frame: &Canvas);
    /// Events since the last call, oldest first. Never blocks.
    fn poll(&mut self) -> Vec<Event>;
    /// Wait up to `ms` for input, then return what arrived. Lets an idle
    /// screen sleep instead of drawing at 60 fps; a button press ends the
    /// wait at once where the backend can block on its input.
    fn wait(&mut self, ms: u32) -> Vec<Event> {
        std::thread::sleep(std::time::Duration::from_millis(ms as u64));
        self.poll()
    }
    /// Whether a button is held now, if the backend can tell from the
    /// device itself (the caller otherwise counts presses and releases).
    fn held(&mut self) -> Option<bool> {
        None
    }
    /// Called once before the process exits, with the last frame shown.
    fn close(&mut self, _last: &Canvas) {}
}

/// Encode a canvas as PNG.
pub fn png_bytes(frame: &Canvas) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, frame.w, frame.h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().expect("png header");
        w.write_image_data(&frame.to_rgba()).expect("png data");
    }
    out
}

pub fn save_png(frame: &Canvas, path: &Path) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, png_bytes(frame))
}

/// No window: keeps the last frame and replays scripted input. Used by tests.
pub struct Headless {
    pub w: u32,
    pub h: u32,
    pub last: Option<Canvas>,
    pub frames: u64,
    script: std::collections::VecDeque<Vec<Event>>,
}

impl Headless {
    pub fn new(w: u32, h: u32) -> Headless {
        Headless {
            w,
            h,
            last: None,
            frames: 0,
            script: Default::default(),
        }
    }

    /// Queue the events to hand out on one future `poll` call.
    pub fn push(&mut self, events: Vec<Event>) {
        self.script.push_back(events);
    }
}

impl Backend for Headless {
    fn size(&self) -> (u32, u32) {
        (self.w, self.h)
    }

    fn present(&mut self, frame: &Canvas) {
        self.frames += 1;
        match &mut self.last {
            Some(c) => c.px.copy_from_slice(&frame.px),
            None => self.last = Some(frame.clone()),
        }
    }

    fn poll(&mut self) -> Vec<Event> {
        self.script.pop_front().unwrap_or_default()
    }
}

/// The device: the framebuffer and the button device.
#[cfg(target_os = "linux")]
pub struct Device {
    fb: crate::fbdev::Fbdev,
    input: Option<crate::evdev::Evdev>,
}

#[cfg(target_os = "linux")]
impl Device {
    /// A regular file in place of the framebuffer is mapped with the Mini
    /// Plus's geometry (for testing the device path off the device).
    pub fn open(fb: &Path, input: &Path) -> std::io::Result<Device> {
        let fb = crate::fbdev::Fbdev::open_any(fb)?;
        let input = match crate::evdev::Evdev::open(input) {
            Ok(i) => Some(i),
            Err(e) => {
                let _ = writeln_err(&format!("slot-mini: {}: {e}", input.display()));
                None
            }
        };
        Ok(Device { fb, input })
    }
}

#[cfg(target_os = "linux")]
fn writeln_err(s: &str) -> std::io::Result<()> {
    use std::io::Write as _;
    writeln!(std::io::stderr(), "{s}")
}

#[cfg(target_os = "linux")]
impl Backend for Device {
    fn size(&self) -> (u32, u32) {
        (self.fb.geometry.width, self.fb.geometry.height)
    }

    fn present(&mut self, frame: &Canvas) {
        self.fb.present(frame);
    }

    fn poll(&mut self) -> Vec<Event> {
        self.input.as_mut().map(|i| i.poll()).unwrap_or_default()
    }

    fn held(&mut self) -> Option<bool> {
        self.input.as_ref()?.any_down()
    }

    fn wait(&mut self, ms: u32) -> Vec<Event> {
        match self.input.as_mut() {
            Some(i) => i.wait(ms as i32),
            None => {
                std::thread::sleep(std::time::Duration::from_millis(ms as u64));
                Vec::new()
            }
        }
    }

    fn close(&mut self, last: &Canvas) {
        self.fb.hand_back(last);
    }
}

#[cfg(feature = "desktop")]
pub mod desktop {
    //! A window for development. Keys: arrows, Z = A, X = B, A = Y, S = X,
    //! Q/W = L1/R1, 1/2 = L2/R2, Backspace = SELECT, Enter = START,
    //! Escape = MENU.
    use super::*;
    use minifb::{Key, Window, WindowOptions};

    pub struct Desktop {
        window: Window,
        w: u32,
        h: u32,
        down: Vec<Key>,
    }

    const KEYS: &[(Key, Button)] = &[
        (Key::Up, Button::Up),
        (Key::Down, Button::Down),
        (Key::Left, Button::Left),
        (Key::Right, Button::Right),
        (Key::Z, Button::A),
        (Key::X, Button::B),
        (Key::S, Button::X),
        (Key::A, Button::Y),
        (Key::Q, Button::L1),
        (Key::W, Button::R1),
        (Key::Key1, Button::L2),
        (Key::Key2, Button::R2),
        (Key::Backspace, Button::Select),
        (Key::Enter, Button::Start),
        (Key::Escape, Button::Menu),
    ];

    impl Desktop {
        pub fn new(title: &str, w: u32, h: u32) -> Result<Desktop, String> {
            let mut window = Window::new(title, w as usize, h as usize, WindowOptions::default())
                .map_err(|e| e.to_string())?;
            window.set_target_fps(60);
            Ok(Desktop {
                window,
                w,
                h,
                down: Vec::new(),
            })
        }
    }

    impl Backend for Desktop {
        fn size(&self) -> (u32, u32) {
            (self.w, self.h)
        }

        fn present(&mut self, frame: &Canvas) {
            let _ = self
                .window
                .update_with_buffer(&frame.px, frame.w as usize, frame.h as usize);
        }

        fn poll(&mut self) -> Vec<Event> {
            if !self.window.is_open() {
                return vec![Event::Quit];
            }
            let now = self.window.get_keys();
            let mut out = Vec::new();
            for &(k, b) in KEYS {
                match (self.down.contains(&k), now.contains(&k)) {
                    (false, true) => out.push(Event::Press(b)),
                    (true, false) => out.push(Event::Release(b)),
                    _ => {}
                }
            }
            self.down = now;
            out
        }
    }
}
