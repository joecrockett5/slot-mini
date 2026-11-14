//! The window: winit for events, softbuffer for pixels, gilrs for
//! controllers. Everything it shows and decides is `PcApp`'s.

#![cfg_attr(windows, windows_subsystem = "windows")]

use std::num::NonZeroU32;
use std::rc::Rc;
use std::time::{Duration, Instant};

use slot_mini::status::Status;
use slot_mini::Pointer;
use slot_mini_gfx::{SCREEN_H, SCREEN_W};
use slot_mini_pc::input::{self, Device};
use slot_mini_pc::pcapp::PcOptions;
use slot_mini_pc::present::{present, Border, View};
use slot_mini_pc::{config, desktop, retroarch, PcApp, Request};
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, ModifiersState, PhysicalKey};
use winit::window::{Fullscreen, Window, WindowId};

/// One frame at 60 a second.
const FRAME: Duration = Duration::from_micros(16_667);
/// How often to look whether the game has ended.
const IN_GAME: Duration = Duration::from_millis(100);

/// Two clicks this close together, in time and in frame pixels, are a
/// double click.
const DOUBLE_MS: u64 = 400;
const DOUBLE_PX: f32 = 8.0;
/// A touchpad's pixels to a wheel's notch.
const NOTCH_PX: f64 = 40.0;

/// The footer's status on a PC: the clock, and no battery to show.
struct PcStatus;

impl Status for PcStatus {
    fn battery(&mut self) -> Option<u8> {
        None
    }
    fn wifi(&mut self) -> bool {
        true
    }
    fn clock(&mut self) -> (u32, u32) {
        slot_mini::status::local_hm()
    }
}

type Surface = softbuffer::Surface<Rc<Window>, Rc<Window>>;

struct Shell {
    pc: PcApp,
    window: Option<Rc<Window>>,
    surface: Option<Surface>,
    gilrs: Option<gilrs::Gilrs>,
    border: Border,
    start: Instant,
    next: Instant,
    mods: ModifiersState,
    /// The window needs drawing again though the frame did not change.
    stale: bool,
    stick: (f32, f32),
    /// Where the frame is in the window, as last drawn.
    view: View,
    /// The pointer, in the frame's pixels, when it is over the frame.
    pointer: Option<(f32, f32)>,
    last_click: Option<(u64, f32, f32)>,
    /// Wheel movement not yet a whole notch (touchpads scroll in pixels).
    wheel: f64,
}

impl Shell {
    fn now(&self) -> u64 {
        self.start.elapsed().as_millis() as u64
    }

    /// A window as it was left, or one the frame fits a whole number of
    /// times into most of the screen.
    fn open(&mut self, el: &ActiveEventLoop) {
        let screens: Vec<_> = el.available_monitors().collect();
        let mut attrs = Window::default_attributes()
            .with_title("slot-mini")
            .with_min_inner_size(PhysicalSize::new(SCREEN_W / 2, SCREEN_H / 2))
            .with_maximized(self.pc.config.maximized);
        let remembered = self.pc.config.window.filter(|&[x, y, w, h]| {
            // Only where a screen still is: a monitor may have gone.
            w > 0
                && h > 0
                && screens.iter().any(|m| {
                    let (p, s) = (m.position(), m.size());
                    x + w > p.x + 64
                        && x < p.x + s.width as i32 - 64
                        && y >= p.y - 16
                        && y < p.y + s.height as i32 - 64
                })
        });
        match remembered {
            Some([x, y, w, h]) => {
                attrs = attrs
                    .with_position(PhysicalPosition::new(x, y))
                    .with_inner_size(PhysicalSize::new(w as u32, h as u32));
            }
            None => {
                let screen = el
                    .primary_monitor()
                    .or_else(|| screens.first().cloned())
                    .map(|m| m.size())
                    .unwrap_or(PhysicalSize::new(SCREEN_W, SCREEN_H));
                let k = (screen.width * 4 / 5 / SCREEN_W)
                    .min(screen.height * 4 / 5 / SCREEN_H)
                    .max(1);
                attrs = attrs.with_inner_size(PhysicalSize::new(SCREEN_W * k, SCREEN_H * k));
            }
        }
        if self.pc.config.fullscreen {
            attrs = attrs.with_fullscreen(Some(Fullscreen::Borderless(None)));
        }
        let window = match el.create_window(attrs) {
            Ok(w) => Rc::new(w),
            Err(e) => {
                eprintln!("slot-mini-pc: no window: {e}");
                return el.exit();
            }
        };
        let surface = softbuffer::Context::new(window.clone())
            .and_then(|context| softbuffer::Surface::new(&context, window.clone()));
        match surface {
            Ok(s) => self.surface = Some(s),
            Err(e) => {
                eprintln!("slot-mini-pc: no surface: {e}");
                return el.exit();
            }
        }
        self.window = Some(window);
    }

    fn draw(&mut self) {
        let (Some(window), Some(surface)) = (&self.window, &mut self.surface) else {
            return;
        };
        let size = window.inner_size();
        let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else {
            return;
        };
        if surface.resize(w, h).is_err() {
            return;
        }
        let Ok(mut buffer) = surface.buffer_mut() else {
            return;
        };
        self.view = present(
            &self.pc.frame,
            &mut self.border,
            self.pc.app.wallpaper(),
            &mut buffer,
            size.width,
            size.height,
        );
        if let Err(e) = buffer.present() {
            eprintln!("slot-mini-pc: present: {e}");
        }
    }

    fn pads(&mut self) {
        let Some(gilrs) = &mut self.gilrs else {
            return;
        };
        let xbox = self.pc.config.xbox_layout;
        while let Some(ev) = gilrs.next_event() {
            use gilrs::{Axis, EventType};
            match ev.event {
                EventType::ButtonPressed(b, _) => {
                    if let Some(b) = input::pad(b, xbox) {
                        self.pc.button(b, true, Device::Controller);
                    }
                }
                EventType::ButtonReleased(b, _) => {
                    if let Some(b) = input::pad(b, xbox) {
                        self.pc.button(b, false, Device::Controller);
                    }
                }
                EventType::AxisChanged(axis, v, _) => {
                    match axis {
                        Axis::LeftStickX => self.stick.0 = v,
                        Axis::LeftStickY => self.stick.1 = v,
                        _ => continue,
                    }
                    self.pc.stick(self.stick.0, self.stick.1);
                }
                EventType::Disconnected => {
                    self.stick = (0.0, 0.0);
                    self.pc.stick(0.0, 0.0);
                }
                _ => {}
            }
        }
    }

    fn requests(&mut self, el: &ActiveEventLoop) {
        for request in std::mem::take(&mut self.pc.requests) {
            let Some(window) = &self.window else {
                continue;
            };
            match request {
                Request::Hide => window.set_visible(false),
                Request::Show => {
                    window.set_visible(true);
                    window.focus_window();
                    self.stale = true;
                }
                Request::Fullscreen(on) => {
                    window.set_fullscreen(on.then_some(Fullscreen::Borderless(None)));
                }
                Request::Quit => self.quit(el),
                Request::PickLibrary => {
                    let start = self.pc.library().to_path_buf();
                    let folder = desktop::pick_folder("Choose the library folder", &start);
                    self.pc.chose_library(folder);
                }
                Request::PickRetroArch => {
                    let exe = desktop::pick_file("Where is RetroArch?", retroarch::EXE);
                    self.pc.chose_retroarch(exe);
                }
                Request::PickCard => {
                    let start = std::path::PathBuf::new();
                    let card = desktop::pick_folder("Choose the Onion SD card", &start);
                    self.pc.chose_card(card);
                }
                Request::OpenUrl(url) => desktop::open_url(&url),
                Request::Reopen => {
                    self.pc.close(None);
                    self.pc = open_pc(self.start.elapsed().as_millis() as u64);
                    self.stale = true;
                }
            }
        }
    }

    fn quit(&mut self, el: &ActiveEventLoop) {
        let rect = self.window.as_ref().and_then(|w| {
            let (p, s) = (w.outer_position().ok()?, w.inner_size());
            Some((
                [p.x, p.y, s.width as i32, s.height as i32],
                w.is_maximized(),
            ))
        });
        self.pc.close(rect);
        el.exit();
    }
}

impl ApplicationHandler for Shell {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_none() {
            self.open(el);
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => self.quit(el),
            WindowEvent::RedrawRequested => self.draw(),
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => self.stale = true,
            WindowEvent::Focused(true) => self.pc.focused(),
            WindowEvent::ModifiersChanged(m) => self.mods = m.state(),
            WindowEvent::KeyboardInput { event, .. } => {
                let PhysicalKey::Code(code) = event.physical_key else {
                    return;
                };
                let down = event.state == ElementState::Pressed;
                let enter = matches!(code, KeyCode::Enter | KeyCode::NumpadEnter);
                if code == KeyCode::F11 || (enter && self.mods.alt_key()) {
                    if down && !event.repeat {
                        self.pc.toggle_fullscreen();
                    }
                    return;
                }
                // The app repeats a held button itself.
                if event.repeat {
                    return;
                }
                if let Some(key) = input::key(code) {
                    self.pc.key(key, down);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let at = self
                    .view
                    .to_frame(SCREEN_W, SCREEN_H, position.x, position.y);
                if at != self.pointer {
                    self.pointer = at;
                    if let Some((x, y)) = at {
                        self.pc.pointer(Pointer::Move(x, y));
                    }
                }
            }
            WindowEvent::CursorLeft { .. } => self.pointer = None,
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button,
                ..
            } => match (button, self.pointer) {
                (MouseButton::Left, Some((x, y))) => {
                    let now = self.now();
                    let double = self.last_click.is_some_and(|(at, lx, ly)| {
                        now - at <= DOUBLE_MS
                            && (x - lx).abs() <= DOUBLE_PX
                            && (y - ly).abs() <= DOUBLE_PX
                    });
                    // A third click starts over.
                    self.last_click = (!double).then_some((now, x, y));
                    self.pc.pointer(match double {
                        true => Pointer::Double(x, y),
                        false => Pointer::Click(x, y),
                    });
                }
                (MouseButton::Right | MouseButton::Back, _) => self.pc.pointer(Pointer::Back),
                _ => {}
            },
            WindowEvent::MouseWheel { delta, .. } => {
                // Wheel up (positive) is towards earlier carts.
                self.wheel -= match delta {
                    MouseScrollDelta::LineDelta(_, y) => y as f64,
                    MouseScrollDelta::PixelDelta(p) => p.y / NOTCH_PX,
                };
                let lines = self.wheel.trunc();
                if lines != 0.0 {
                    self.wheel -= lines;
                    let (x, y) = self.pointer.unwrap_or((0.0, 0.0));
                    self.pc.pointer(Pointer::Wheel(x, y, lines as i32));
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        let now = Instant::now();
        if now >= self.next {
            self.pads();
            self.pc.tick(self.now());
            self.requests(el);
            let changed = self.pc.render();
            if changed || std::mem::take(&mut self.stale) {
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            let step = if self.pc.in_game() { IN_GAME } else { FRAME };
            self.next = (self.next + step).max(now);
        }
        el.set_control_flow(ControlFlow::WaitUntil(self.next));
    }
}

/// The app on the library the config names (or the default one).
fn open_pc(now: u64) -> PcApp {
    let config_dir = config::dir();
    let library = config::Config::read(&config_dir)
        .library
        .unwrap_or_else(config::default_library);
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64);
    let opts = PcOptions {
        config_dir,
        places: retroarch::usual_places(),
        status: Box::new(PcStatus),
        sound: Some(Box::new(slot_mini_pc::audio::Speakers::new())),
        face_wait: slot_mini::Options::default().face_wait,
        seed,
        releases_url: slot_mini_pc::net::releases_url(slot_mini_pc::net::REPO),
        buildbot: retroarch::BUILDBOT.to_string(),
    };
    PcApp::new(&library, opts, now)
}

fn main() {
    let start = Instant::now();
    let mut shell = Shell {
        pc: open_pc(0),
        window: None,
        surface: None,
        gilrs: gilrs::Gilrs::new()
            .map_err(|e| eprintln!("slot-mini-pc: controllers: {e}"))
            .ok(),
        border: Border::default(),
        start,
        next: start,
        mods: ModifiersState::empty(),
        stale: true,
        stick: (0.0, 0.0),
        view: View::fit(SCREEN_W, SCREEN_H, SCREEN_W, SCREEN_H),
        pointer: None,
        last_click: None,
        wheel: 0.0,
    };
    let el = match EventLoop::new() {
        Ok(el) => el,
        Err(e) => {
            eprintln!("slot-mini-pc: {e}");
            std::process::exit(1);
        }
    };
    if let Err(e) = el.run_app(&mut shell) {
        eprintln!("slot-mini-pc: {e}");
        std::process::exit(1);
    }
}
