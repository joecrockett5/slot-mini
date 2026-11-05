//! Writes package/app/icon.png: a Game Boy cartridge drawn in the
//! style of Onion's own app icons (flat colours, a dark line, a thick white
//! outline), with a blank label. 128x128; Onion scales it down in its list.
//!
//!     cargo run --example icon -p slot-mini

type Rgba = [f32; 4];

const WHITE: Rgba = rgb(0xf2f6fa);
const LINE: Rgba = rgb(0x231d4d);
const BODY: Rgba = rgb(0x6c5ce7);
const BODY_SHADE: Rgba = rgb(0x5446c8);
const BODY_LIGHT: Rgba = rgb(0x8b7ff0);
const RECESS: Rgba = rgb(0x40359c);
const LABEL: Rgba = rgb(0xe8577f);
const LABEL_BAND: Rgba = rgb(0xbf3b66);
const LABEL_GLOSS: Rgba = rgb(0xf27f9e);

const fn rgb(c: u32) -> Rgba {
    [
        ((c >> 16) & 0xff) as f32 / 255.0,
        ((c >> 8) & 0xff) as f32 / 255.0,
        (c & 0xff) as f32 / 255.0,
        1.0,
    ]
}

/// Signed distance from `p` to a polygon: negative inside.
fn polygon(p: (f32, f32), v: &[(f32, f32)]) -> f32 {
    let dot = |a: (f32, f32), b: (f32, f32)| a.0 * b.0 + a.1 * b.1;
    let sub = |a: (f32, f32), b: (f32, f32)| (a.0 - b.0, a.1 - b.1);
    let mut d = dot(sub(p, v[0]), sub(p, v[0]));
    let mut s = 1.0;
    let mut j = v.len() - 1;
    for i in 0..v.len() {
        let e = sub(v[j], v[i]);
        let w = sub(p, v[i]);
        let t = (dot(w, e) / dot(e, e)).clamp(0.0, 1.0);
        let b = (w.0 - e.0 * t, w.1 - e.1 * t);
        d = d.min(dot(b, b));
        let c = [p.1 >= v[i].1, p.1 < v[j].1, e.0 * w.1 > e.1 * w.0];
        if c.iter().all(|x| *x) || c.iter().all(|x| !*x) {
            s = -s;
        }
        j = i;
    }
    s * d.sqrt()
}

/// A rectangle as a polygon.
fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> [(f32, f32); 4] {
    [(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
}

/// Pixel coverage of a shape at signed distance `d`.
fn cover(d: f32) -> f32 {
    (0.5 - d).clamp(0.0, 1.0)
}

fn over(dst: &mut Rgba, c: Rgba, a: f32) {
    let a = a * c[3];
    for k in 0..3 {
        dst[k] = c[k] * a + dst[k] * (1.0 - a);
    }
    dst[3] = a + dst[3] * (1.0 - a);
}

fn pixel(p: (f32, f32)) -> Rgba {
    // A Game Boy cartridge, the shape everyone knows: notched top-right
    // corner, grip grooves along the top, a label, an arrow pointing in.
    // Corners are rounded by `R`.
    const R: f32 = 4.0;
    let body = [
        (24.0, 20.0),
        (82.0, 20.0),
        (82.0, 29.0),
        (104.0, 29.0),
        (104.0, 108.0),
        (24.0, 108.0),
    ];
    let d = polygon(p, &body) - R;
    let recess = polygon(p, &rect(32.0, 47.0, 96.0, 90.0)) - 3.0;
    let label = polygon(p, &rect(35.0, 50.0, 93.0, 87.0)) - 1.5;
    let arrow = polygon(p, &[(57.0, 96.0), (71.0, 96.0), (64.0, 103.0)]) - 1.0;

    let mut out = [0.0; 4];
    over(&mut out, WHITE, cover(d - 8.0));
    over(&mut out, LINE, cover(d - 3.0));
    // Body: flat colour, lit along the top, darker along the bottom.
    let inside = cover(d);
    let fill = if p.1 > 105.0 {
        BODY_SHADE
    } else if d > -4.0 && p.1 < 40.0 {
        BODY_LIGHT
    } else {
        BODY
    };
    over(&mut out, fill, inside);
    // Grip grooves.
    for y in [32.0, 38.0] {
        let g = polygon(p, &rect(32.0, y, 96.0, y + 1.0)) - 1.0;
        over(&mut out, RECESS, cover(g) * inside);
    }
    over(&mut out, RECESS, cover(arrow) * inside);
    over(&mut out, RECESS, cover(recess) * inside);
    over(&mut out, LINE, cover(label - 1.5));
    // The label: blank, with a header band and a soft gloss stripe.
    let mut lab = LABEL;
    if p.1 < 59.0 {
        lab = LABEL_BAND;
    } else {
        let g = p.0 + (p.1 - 59.0) * 0.6;
        if (70.0..78.0).contains(&g) || (82.0..85.0).contains(&g) {
            lab = LABEL_GLOSS;
        }
    }
    over(&mut out, lab, cover(label));
    out
}

fn main() {
    const SIZE: u32 = 128;
    // 4x4 samples a pixel on top of the distance-based edges.
    const SS: u32 = 4;
    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let mut acc = [0.0f32; 4];
            for sy in 0..SS {
                for sx in 0..SS {
                    let p = (
                        x as f32 + (sx as f32 + 0.5) / SS as f32,
                        y as f32 + (sy as f32 + 0.5) / SS as f32,
                    );
                    // `pixel` is premultiplied, so averaging is right at edges.
                    for (a, c) in acc.iter_mut().zip(pixel(p)) {
                        *a += c;
                    }
                }
            }
            let n = (SS * SS) as f32;
            let a = acc[3] / n;
            for c in &acc[..3] {
                let v = if a > 0.0 { c / n / a } else { 0.0 };
                rgba.push((v.min(1.0) * 255.0).round() as u8);
            }
            rgba.push((a * 255.0).round() as u8);
        }
    }
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../package/app/icon.png");
    let mut bytes = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut bytes, SIZE, SIZE);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&rgba).unwrap();
    }
    std::fs::write(&out, bytes).unwrap();
    println!("{}", out.display());
}
