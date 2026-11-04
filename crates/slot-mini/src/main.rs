use slot_mini_gfx::{Backend, Canvas, Headless, SCREEN_H, SCREEN_W};

fn main() {
    let mut frame = Canvas::new(SCREEN_W, SCREEN_H);
    frame.clear(0x202020);
    let mut out = Headless::new(SCREEN_W, SCREEN_H);
    out.present(&frame);
    println!("slot-mini {}", env!("CARGO_PKG_VERSION"));
}
