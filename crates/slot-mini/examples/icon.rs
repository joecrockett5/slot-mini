//! Writes package/app/icon.png: a cart face from slot's own face builder,
//! labelled "slot mini", fitted into 128x128.
//!
//!     cargo run --example icon -p slot-mini

fn main() {
    let cart = slot_mini_store::Cart {
        platform: slot_mini_store::Platform::Gba,
        stem: "slot mini".into(),
        rom: "slot mini.gba".into(),
        label: None,
        title: String::new(),
        code: String::new(),
        shell: slot_mini_store::ShellChoice::parse("auto 4a3f8f solid"),
    };
    let f = slot_mini_ui::cart_face(&cart);
    let sprite = slot_mini_gfx::Sprite::from_rgba(f.w, f.h, &f.rgba);
    let s = 124.0 / f.w.max(f.h) as f32;
    let small = sprite.resized(
        (f.w as f32 * s).round() as u32,
        (f.h as f32 * s).round() as u32,
    );
    let mut icon = slot_mini_gfx::Sprite::new(128, 128);
    icon.draw_over(
        &small,
        ((128 - small.w) / 2) as i32,
        ((128 - small.h) / 2) as i32,
    );
    let rgba = icon.to_rgba();
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../package/app/icon.png");
    let mut bytes = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut bytes, 128, 128);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&rgba).unwrap();
    }
    std::fs::write(&out, bytes).unwrap();
    println!("{}", out.display());
}
