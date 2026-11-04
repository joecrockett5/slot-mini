//! slot-mini's drawing layer: a software `Canvas`, sprites, an executor for
//! slot's `Draw` lists, text, and display backends (headless PNG, a desktop
//! window, and the Linux framebuffer on the device).

pub mod backend;
pub mod canvas;
pub mod draw;
pub mod text;

pub use backend::{png_bytes, save_png, Backend, Button, Event, Headless};
pub use canvas::{over, premultiply, scale_px, Canvas, Rect, Sprite, BLACK};
pub use draw::{framed, Draw, Renderer, Stage, TexId, OUT_H, OUT_W};
pub use text::Text;

/// The Mini Plus screen.
pub const SCREEN_W: u32 = 640;
pub const SCREEN_H: u32 = 480;
