//! Keys and controller buttons as the Miyoo's buttons.
//!
//! Keyboard: the desktop build's keys (arrows, Z X A S, Q W, 1 2,
//! Backspace, Escape) with one change and some additions. Enter is A, as
//! on any PC, so START moves to Space; Tab also toggles the view, Page
//! Up/Down jump a letter, and F1 is MENU beside Escape. F11 and Alt+Enter
//! (fullscreen) belong to the window, not to this table.
//!
//! Controller: by position in the Miyoo's Nintendo layout (right = A,
//! bottom = B, top = X, left = Y), or by Xbox labels with the setting
//! (bottom = A, right = B, left = X, top = Y). MENU is a click of either
//! stick, or the Guide button where the system lets it through (XInput
//! does not).

use gilrs::Button as Pad;
use slot_mini_gfx::Button;
use winit::keyboard::KeyCode;

/// What the player last used: the legend shows its buttons.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Device {
    #[default]
    Keyboard,
    Controller,
}

/// What a key does.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Key {
    Button(Button),
    /// Page Up / Page Down: the letter before or after (L2 + Up / Down on
    /// a controller).
    Letter(bool),
}

pub fn key(code: KeyCode) -> Option<Key> {
    Some(Key::Button(match code {
        KeyCode::ArrowUp => Button::Up,
        KeyCode::ArrowDown => Button::Down,
        KeyCode::ArrowLeft => Button::Left,
        KeyCode::ArrowRight => Button::Right,
        KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::KeyZ => Button::A,
        KeyCode::KeyX => Button::B,
        KeyCode::KeyS => Button::X,
        KeyCode::KeyA => Button::Y,
        KeyCode::KeyQ => Button::L1,
        KeyCode::KeyW => Button::R1,
        KeyCode::Digit1 => Button::L2,
        KeyCode::Digit2 | KeyCode::Tab => Button::R2,
        KeyCode::Backspace => Button::Select,
        KeyCode::Space => Button::Start,
        KeyCode::Escape | KeyCode::F1 => Button::Menu,
        KeyCode::PageUp => return Some(Key::Letter(false)),
        KeyCode::PageDown => return Some(Key::Letter(true)),
        _ => return None,
    }))
}

pub fn pad(button: Pad, xbox: bool) -> Option<Button> {
    Some(match (button, xbox) {
        (Pad::East, false) | (Pad::South, true) => Button::A,
        (Pad::South, false) | (Pad::East, true) => Button::B,
        (Pad::North, false) | (Pad::West, true) => Button::X,
        (Pad::West, false) | (Pad::North, true) => Button::Y,
        (Pad::DPadUp, _) => Button::Up,
        (Pad::DPadDown, _) => Button::Down,
        (Pad::DPadLeft, _) => Button::Left,
        (Pad::DPadRight, _) => Button::Right,
        (Pad::LeftTrigger, _) => Button::L1,
        (Pad::RightTrigger, _) => Button::R1,
        (Pad::LeftTrigger2, _) => Button::L2,
        (Pad::RightTrigger2, _) => Button::R2,
        (Pad::Select, _) => Button::Select,
        (Pad::Start, _) => Button::Start,
        (Pad::Mode | Pad::LeftThumb | Pad::RightThumb, _) => Button::Menu,
        _ => return None,
    })
}

/// The left stick as a D-pad: which directions are held at this position.
/// A direction starts at `ON` and lets go at `OFF`, so a stick resting
/// near the edge of its dead zone does not chatter.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Stick {
    pub x: f32,
    pub y: f32,
    held: [bool; 4],
}

const ON: f32 = 0.6;
const OFF: f32 = 0.4;

impl Stick {
    /// Take the stick's new position (up is positive `y`) and return the
    /// presses and releases it causes.
    pub fn moved(&mut self, x: f32, y: f32) -> Vec<(Button, bool)> {
        (self.x, self.y) = (x, y);
        let level = [-x, x, y, -y];
        let buttons = [Button::Left, Button::Right, Button::Up, Button::Down];
        let mut out = Vec::new();
        for i in 0..4 {
            let now = match self.held[i] {
                true => level[i] > OFF,
                false => level[i] > ON,
            };
            if now != self.held[i] {
                self.held[i] = now;
                out.push((buttons[i], now));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEYS: [KeyCode; 21] = [
        KeyCode::ArrowUp,
        KeyCode::ArrowDown,
        KeyCode::ArrowLeft,
        KeyCode::ArrowRight,
        KeyCode::Enter,
        KeyCode::NumpadEnter,
        KeyCode::KeyZ,
        KeyCode::KeyX,
        KeyCode::KeyS,
        KeyCode::KeyA,
        KeyCode::KeyQ,
        KeyCode::KeyW,
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Tab,
        KeyCode::Backspace,
        KeyCode::Space,
        KeyCode::Escape,
        KeyCode::F1,
        KeyCode::PageUp,
        KeyCode::PageDown,
    ];

    #[test]
    fn every_button_has_a_key_and_the_desktop_keys_are_kept() {
        let reachable: Vec<Button> = KEYS
            .iter()
            .filter_map(|k| match key(*k) {
                Some(Key::Button(b)) => Some(b),
                _ => None,
            })
            .collect();
        for b in [
            Button::Up,
            Button::Down,
            Button::Left,
            Button::Right,
            Button::A,
            Button::B,
            Button::X,
            Button::Y,
            Button::L1,
            Button::R1,
            Button::L2,
            Button::R2,
            Button::Select,
            Button::Start,
            Button::Menu,
        ] {
            assert!(reachable.contains(&b), "{b:?} has no key");
        }
        assert_eq!(key(KeyCode::KeyZ), Some(Key::Button(Button::A)));
        assert_eq!(key(KeyCode::Enter), Some(Key::Button(Button::A)));
        assert_eq!(key(KeyCode::Escape), Some(Key::Button(Button::Menu)));
        assert_eq!(key(KeyCode::F11), None, "fullscreen is the window's");
    }

    #[test]
    fn a_controller_maps_by_position_or_by_xbox_label() {
        assert_eq!(pad(Pad::East, false), Some(Button::A));
        assert_eq!(pad(Pad::South, false), Some(Button::B));
        assert_eq!(pad(Pad::South, true), Some(Button::A));
        assert_eq!(pad(Pad::East, true), Some(Button::B));
        assert_eq!(pad(Pad::North, false), Some(Button::X));
        assert_eq!(pad(Pad::West, true), Some(Button::X));
        assert_eq!(pad(Pad::RightThumb, false), Some(Button::Menu));
        assert_eq!(pad(Pad::RightTrigger2, true), Some(Button::R2));
    }

    #[test]
    fn the_stick_presses_past_its_threshold_and_does_not_chatter() {
        let mut s = Stick::default();
        assert_eq!(s.moved(0.5, 0.0), vec![]);
        assert_eq!(s.moved(0.7, 0.0), vec![(Button::Right, true)]);
        assert_eq!(s.moved(0.5, 0.0), vec![]);
        assert_eq!(
            s.moved(0.1, 0.9),
            vec![(Button::Right, false), (Button::Up, true)]
        );
        assert_eq!(s.moved(0.0, 0.0), vec![(Button::Up, false)]);
    }
}
