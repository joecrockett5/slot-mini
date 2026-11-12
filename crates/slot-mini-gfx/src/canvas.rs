//! A software frame buffer and the sprites drawn into it.
//!
//! Pixels are `u32` in `0xAARRGGBB` order, which is what the Miyoo framebuffer
//! takes. The canvas itself is always opaque (alpha `0xFF`). Sprites hold
//! premultiplied alpha so that blending is one multiply per channel.

/// Fully opaque black.
pub const BLACK: u32 = 0xFF00_0000;

/// Multiply every 8-bit channel of `c` by `f / 255`, rounded.
#[inline(always)]
pub fn scale_px(c: u32, f: u32) -> u32 {
    let mut rb = (c & 0x00FF_00FF) * f + 0x0080_0080;
    rb = ((rb + ((rb >> 8) & 0x00FF_00FF)) >> 8) & 0x00FF_00FF;
    let mut ag = ((c >> 8) & 0x00FF_00FF) * f + 0x0080_0080;
    ag = (ag + ((ag >> 8) & 0x00FF_00FF)) & 0xFF00_FF00;
    rb | ag
}

/// `a` towards `b` by `w / 256`, per 8-bit channel.
#[inline(always)]
fn lerp_px(a: u32, b: u32, w: u32) -> u32 {
    if w == 0 || a == b {
        return a;
    }
    let (ia, ib) = (256 - w, w);
    let rb = ((a & 0x00FF_00FF) * ia + (b & 0x00FF_00FF) * ib) >> 8 & 0x00FF_00FF;
    let ag = ((a >> 8 & 0x00FF_00FF) * ia + (b >> 8 & 0x00FF_00FF) * ib) & 0xFF00_FF00;
    rb | ag
}

/// Premultiplied `src` over opaque `dst`.
#[inline(always)]
pub fn over(dst: u32, src: u32) -> u32 {
    let a = src >> 24;
    match a {
        0 => dst,
        255 => src,
        _ => src.wrapping_add(scale_px(dst, 255 - a)) | 0xFF00_0000,
    }
}

/// Straight-alpha RGBA bytes to one premultiplied ARGB pixel.
#[inline(always)]
pub fn premultiply(r: u8, g: u8, b: u8, a: u8) -> u32 {
    let px = (r as u32) << 16 | (g as u32) << 8 | b as u32;
    match a {
        255 => px | 0xFF00_0000,
        0 => 0,
        _ => scale_px(px, a as u32) & 0x00FF_FFFF | (a as u32) << 24,
    }
}

/// An image to draw: premultiplied ARGB.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Sprite {
    pub w: u32,
    pub h: u32,
    pub px: Vec<u32>,
}

impl Sprite {
    pub fn new(w: u32, h: u32) -> Sprite {
        Sprite {
            w,
            h,
            px: vec![0; (w * h) as usize],
        }
    }

    /// From straight-alpha RGBA bytes, as slot's face builders produce them.
    pub fn from_rgba(w: u32, h: u32, rgba: &[u8]) -> Sprite {
        assert_eq!(rgba.len(), (w * h * 4) as usize, "rgba size");
        let px = rgba
            .chunks_exact(4)
            .map(|p| premultiply(p[0], p[1], p[2], p[3]))
            .collect();
        Sprite { w, h, px }
    }

    /// Back to straight-alpha RGBA bytes.
    pub fn to_rgba(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.px.len() * 4);
        for &p in &self.px {
            let a = p >> 24;
            let un = |c: u32| match a {
                0 => 0,
                _ => ((c * 255 + a / 2) / a).min(255) as u8,
            };
            out.extend_from_slice(&[un(p >> 16 & 0xFF), un(p >> 8 & 0xFF), un(p & 0xFF), a as u8]);
        }
        out
    }

    /// Area-averaged resize, for sprites that will be drawn at one size for
    /// a while (a cart at slot's side scale). Separable box filter in fixed
    /// point; enlarging falls back to nearest.
    pub fn resized(&self, w: u32, h: u32) -> Sprite {
        let (w, h) = (w.max(1), h.max(1));
        if (w, h) == (self.w, self.h) || self.w == 0 || self.h == 0 {
            let mut out = self.clone();
            if self.w == 0 || self.h == 0 {
                out = Sprite::new(w, h);
            }
            return out;
        }
        // Horizontal pass into 4 channels of u32 sums, scaled by 256; the
        // vertical pass scales by 256 again, hence the shift by 16.
        let xw = weights(self.w, w);
        let yw = weights(self.h, h);
        let mut mid = vec![[0u32; 4]; (w * self.h) as usize];
        for y in 0..self.h as usize {
            let row = &self.px[y * self.w as usize..][..self.w as usize];
            for (x, taps) in xw.iter().enumerate() {
                let mut acc = [0u32; 4];
                for &(i, k) in taps {
                    let p = row[i as usize];
                    acc[0] += (p >> 24) * k;
                    acc[1] += (p >> 16 & 0xFF) * k;
                    acc[2] += (p >> 8 & 0xFF) * k;
                    acc[3] += (p & 0xFF) * k;
                }
                mid[y * w as usize + x] = acc;
            }
        }
        let mut out = Sprite::new(w, h);
        for (y, taps) in yw.iter().enumerate() {
            for x in 0..w as usize {
                let mut acc = [0u32; 4];
                for &(i, k) in taps {
                    let m = mid[i as usize * w as usize + x];
                    for c in 0..4 {
                        acc[c] += m[c] * k;
                    }
                }
                let v = acc.map(|v| ((v + 32768) >> 16).min(255));
                let a = v[0];
                out.px[y * w as usize + x] =
                    a << 24 | v[1].min(a) << 16 | v[2].min(a) << 8 | v[3].min(a);
            }
        }
        out
    }

    /// Draw `src` over this sprite at (x, y), both premultiplied, keeping
    /// transparency (for flattening a cart's layers into one image).
    pub fn draw_over(&mut self, src: &Sprite, x: i32, y: i32) {
        for sy in 0..src.h as i32 {
            let dy = y + sy;
            if dy < 0 || dy >= self.h as i32 {
                continue;
            }
            for sx in 0..src.w as i32 {
                let dx = x + sx;
                if dx < 0 || dx >= self.w as i32 {
                    continue;
                }
                let s = src.px[(sy as u32 * src.w + sx as u32) as usize];
                let a = s >> 24;
                if a == 0 {
                    continue;
                }
                let d = &mut self.px[(dy as u32 * self.w + dx as u32) as usize];
                *d = match a {
                    255 => s,
                    _ => s.wrapping_add(scale_px(*d, 255 - a)),
                };
            }
        }
    }

    /// Rotated by `deg` degrees (clockwise on screen) about its centre,
    /// bilinear, on a canvas grown to fit. Done once when a screen opens,
    /// standing in for slot's GL `Draw::Turned`/`Draw::Quad`.
    pub fn rotated(&self, deg: f32) -> Sprite {
        let (sin, cos) = deg.to_radians().sin_cos();
        let (w, h) = (self.w as f32, self.h as f32);
        let nw = (w * cos.abs() + h * sin.abs()).ceil() as u32 + 2;
        let nh = (w * sin.abs() + h * cos.abs()).ceil() as u32 + 2;
        let mut out = Sprite::new(nw, nh);
        let (cx, cy) = (w / 2.0, h / 2.0);
        let (ncx, ncy) = (nw as f32 / 2.0, nh as f32 / 2.0);
        let at = |x: i32, y: i32| -> [f32; 4] {
            if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 {
                return [0.0; 4];
            }
            let p = self.px[(y as u32 * self.w + x as u32) as usize];
            [
                (p >> 24) as f32,
                (p >> 16 & 0xFF) as f32,
                (p >> 8 & 0xFF) as f32,
                (p & 0xFF) as f32,
            ]
        };
        for y in 0..nh {
            for x in 0..nw {
                // Inverse map the destination pixel centre into the source.
                let (dx, dy) = (x as f32 + 0.5 - ncx, y as f32 + 0.5 - ncy);
                let sx = dx * cos + dy * sin + cx - 0.5;
                let sy = -dx * sin + dy * cos + cy - 0.5;
                let (x0, y0) = (sx.floor(), sy.floor());
                let (fx, fy) = (sx - x0, sy - y0);
                let (x0, y0) = (x0 as i32, y0 as i32);
                let mut c = [0f32; 4];
                for (px, py, k) in [
                    (x0, y0, (1.0 - fx) * (1.0 - fy)),
                    (x0 + 1, y0, fx * (1.0 - fy)),
                    (x0, y0 + 1, (1.0 - fx) * fy),
                    (x0 + 1, y0 + 1, fx * fy),
                ] {
                    let v = at(px, py);
                    for i in 0..4 {
                        c[i] += v[i] * k;
                    }
                }
                let a = c[0].round().clamp(0.0, 255.0) as u32;
                let ch = |v: f32| (v.round().clamp(0.0, 255.0) as u32).min(a);
                out.px[(y * nw + x) as usize] = a << 24 | ch(c[1]) << 16 | ch(c[2]) << 8 | ch(c[3]);
            }
        }
        out
    }

    /// Whether every pixel is fully opaque.
    pub fn is_opaque(&self) -> bool {
        self.px.iter().all(|p| p >> 24 == 255)
    }
}

/// For each output position, the source taps and their weights (summing to
/// 256), covering the output pixel's footprint in the source.
fn weights(src: u32, dst: u32) -> Vec<Vec<(u32, u32)>> {
    let scale = src as f64 / dst as f64;
    (0..dst)
        .map(|o| {
            if scale <= 1.0 {
                let i = (((o as f64 + 0.5) * scale) as u32).min(src - 1);
                return vec![(i, 256)];
            }
            let (a, b) = (o as f64 * scale, (o + 1) as f64 * scale);
            let mut taps = Vec::new();
            let mut i = a.floor() as u32;
            while (i as f64) < b && i < src {
                let cover = (b.min(i as f64 + 1.0) - a.max(i as f64)) / scale;
                taps.push((i, (cover * 256.0).round() as u32));
                i += 1;
            }
            // Make the weights sum to exactly 256.
            let sum: u32 = taps.iter().map(|t| t.1).sum();
            if let Some(last) = taps.iter_mut().max_by_key(|t| t.1) {
                last.1 = (last.1 + 256).saturating_sub(sum);
            }
            taps
        })
        .collect()
}

/// An opaque frame being drawn.
#[derive(Clone)]
pub struct Canvas {
    pub w: u32,
    pub h: u32,
    pub px: Vec<u32>,
}

/// An integer rectangle, clipped against something.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub fn new(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
    }

    /// Snap a float rectangle to whole pixels by rounding both edges.
    pub fn snap(x: f32, y: f32, w: f32, h: f32) -> Rect {
        let (x0, y0) = (x.round() as i32, y.round() as i32);
        let (x1, y1) = ((x + w).round() as i32, (y + h).round() as i32);
        Rect::new(x0, y0, x1 - x0, y1 - y0)
    }

    pub fn is_empty(&self) -> bool {
        self.w <= 0 || self.h <= 0
    }

    pub fn intersect(&self, o: &Rect) -> Rect {
        let x0 = self.x.max(o.x);
        let y0 = self.y.max(o.y);
        let x1 = (self.x + self.w).min(o.x + o.w);
        let y1 = (self.y + self.h).min(o.y + o.h);
        Rect::new(x0, y0, x1 - x0, y1 - y0)
    }
}

impl Canvas {
    pub fn new(w: u32, h: u32) -> Canvas {
        Canvas {
            w,
            h,
            px: vec![BLACK; (w * h) as usize],
        }
    }

    pub fn bounds(&self) -> Rect {
        Rect::new(0, 0, self.w as i32, self.h as i32)
    }

    pub fn clear(&mut self, colour: u32) {
        self.px.fill(colour | 0xFF00_0000);
    }

    /// Fill a rectangle with a premultiplied colour.
    pub fn fill(&mut self, r: Rect, colour: u32) {
        let r = r.intersect(&self.bounds());
        if r.is_empty() || colour >> 24 == 0 {
            return;
        }
        let w = self.w as usize;
        for y in r.y..r.y + r.h {
            let row = &mut self.px[y as usize * w + r.x as usize..][..r.w as usize];
            if colour >> 24 == 255 {
                row.fill(colour);
            } else {
                for d in row {
                    *d = over(*d, colour);
                }
            }
        }
    }

    /// Fill with a straight-alpha float colour, as slot's `Draw::Rect` gives.
    pub fn fill_f(&mut self, r: Rect, rgba: [f32; 4]) {
        let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        self.fill(
            r,
            premultiply(b(rgba[0]), b(rgba[1]), b(rgba[2]), b(rgba[3])),
        );
    }

    /// Draw a sprite at its own size with an overall alpha (0..=255).
    pub fn blit(&mut self, s: &Sprite, x: i32, y: i32, alpha: u32) {
        let dst = Rect::new(x, y, s.w as i32, s.h as i32);
        let c = dst.intersect(&self.bounds());
        if c.is_empty() || alpha == 0 {
            return;
        }
        let (sx, sy) = ((c.x - x) as usize, (c.y - y) as usize);
        let w = self.w as usize;
        for row in 0..c.h as usize {
            let src = &s.px[(sy + row) * s.w as usize + sx..][..c.w as usize];
            let dst = &mut self.px[(c.y as usize + row) * w + c.x as usize..][..c.w as usize];
            if alpha >= 255 {
                for (d, &p) in dst.iter_mut().zip(src) {
                    *d = over(*d, p);
                }
            } else {
                for (d, &p) in dst.iter_mut().zip(src) {
                    if p != 0 {
                        *d = over(*d, scale_px(p, alpha));
                    }
                }
            }
        }
    }

    /// Draw the region of a sprite starting at (sx, sy), the size of `dst`,
    /// unscaled.
    pub fn blit_region(&mut self, s: &Sprite, sx: i32, sy: i32, dst: Rect, alpha: u32) {
        let c = dst.intersect(&self.bounds());
        if c.is_empty() || alpha == 0 {
            return;
        }
        // Clip against the sprite too.
        let x0 = (sx + c.x - dst.x).max(0);
        let y0 = (sy + c.y - dst.y).max(0);
        let x1 = (sx + c.x - dst.x + c.w).min(s.w as i32);
        let y1 = (sy + c.y - dst.y + c.h).min(s.h as i32);
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let (dx, dy) = (c.x + x0 - (sx + c.x - dst.x), c.y + y0 - (sy + c.y - dst.y));
        let w = self.w as usize;
        let n = (x1 - x0) as usize;
        for row in 0..(y1 - y0) as usize {
            let src = &s.px[(y0 as usize + row) * s.w as usize + x0 as usize..][..n];
            let dst = &mut self.px[(dy as usize + row) * w + dx as usize..][..n];
            if alpha >= 255 {
                for (d, &p) in dst.iter_mut().zip(src) {
                    *d = over(*d, p);
                }
            } else {
                for (d, &p) in dst.iter_mut().zip(src) {
                    if p != 0 {
                        *d = over(*d, scale_px(p, alpha));
                    }
                }
            }
        }
    }

    /// Draw part of a sprite (`src`, in sprite pixels) stretched to `dst`
    /// with nearest-neighbour sampling. Fast; used while things move.
    pub fn blit_scaled(&mut self, s: &Sprite, src: [f32; 4], dst: Rect, alpha: u32) {
        let c = dst.intersect(&self.bounds());
        if c.is_empty() || alpha == 0 || s.w == 0 || s.h == 0 {
            return;
        }
        let [u0, v0, uw, vh] = src;
        let step_x = uw / dst.w as f32;
        let step_y = vh / dst.h as f32;
        let xs: Vec<usize> = (c.x..c.x + c.w)
            .map(|x| {
                let u = u0 + (x - dst.x) as f32 * step_x + step_x * 0.5;
                (u as i64).clamp(0, s.w as i64 - 1) as usize
            })
            .collect();
        let w = self.w as usize;
        for y in c.y..c.y + c.h {
            let v = v0 + (y - dst.y) as f32 * step_y + step_y * 0.5;
            let sy = (v as i64).clamp(0, s.h as i64 - 1) as usize;
            let src_row = &s.px[sy * s.w as usize..][..s.w as usize];
            let dst_row = &mut self.px[y as usize * w + c.x as usize..][..c.w as usize];
            if alpha >= 255 {
                for (d, &sx) in dst_row.iter_mut().zip(&xs) {
                    *d = over(*d, src_row[sx]);
                }
            } else {
                for (d, &sx) in dst_row.iter_mut().zip(&xs) {
                    let p = src_row[sx];
                    if p != 0 {
                        *d = over(*d, scale_px(p, alpha));
                    }
                }
            }
        }
    }

    /// slot's `Draw::Turned`: the sprite stretched to `(x, y, w, h)` and
    /// turned by `turn` radians about that rectangle's centre (clockwise on
    /// screen for positive turns, as slot's vertex shader does). Bilinear, so
    /// the slanted edges are smooth; samples outside the sprite are clear.
    #[allow(clippy::too_many_arguments)]
    pub fn blit_turned(
        &mut self,
        s: &Sprite,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        turn: f32,
        alpha: u32,
    ) {
        if alpha == 0 || s.w == 0 || s.h == 0 || w <= 0.0 || h <= 0.0 {
            return;
        }
        let (sin, cos) = turn.sin_cos();
        let (cx, cy) = (x + w / 2.0, y + h / 2.0);
        // The turned rectangle's bounds on the canvas.
        let ex = (w * cos.abs() + h * sin.abs()) / 2.0;
        let ey = (w * sin.abs() + h * cos.abs()) / 2.0;
        let box_ = Rect::new(
            (cx - ex).floor() as i32 - 1,
            (cy - ey).floor() as i32 - 1,
            (2.0 * ex).ceil() as i32 + 3,
            (2.0 * ey).ceil() as i32 + 3,
        )
        .intersect(&self.bounds());
        if box_.is_empty() {
            return;
        }
        let (kx, ky) = (s.w as f32 / w, s.h as f32 / h);
        let sample = |sx: i32, sy: i32| -> u32 {
            if sx < 0 || sy < 0 || sx >= s.w as i32 || sy >= s.h as i32 {
                return 0;
            }
            s.px[(sy as u32 * s.w + sx as u32) as usize]
        };
        let width = self.w as usize;
        for py in box_.y..box_.y + box_.h {
            for px in box_.x..box_.x + box_.w {
                let (dx, dy) = (px as f32 + 0.5 - cx, py as f32 + 0.5 - cy);
                // Undo the turn, then into sprite pixels.
                let lx = cos * dx + sin * dy + w / 2.0;
                let ly = -sin * dx + cos * dy + h / 2.0;
                let (u, v) = (lx * kx - 0.5, ly * ky - 0.5);
                if u < -1.0 || v < -1.0 || u > s.w as f32 || v > s.h as f32 {
                    continue;
                }
                let (u0, v0) = (u.floor(), v.floor());
                let (fx, fy) = (u - u0, v - v0);
                let (u0, v0) = (u0 as i32, v0 as i32);
                let mut acc = [0f32; 4];
                for (sx, sy, k) in [
                    (u0, v0, (1.0 - fx) * (1.0 - fy)),
                    (u0 + 1, v0, fx * (1.0 - fy)),
                    (u0, v0 + 1, (1.0 - fx) * fy),
                    (u0 + 1, v0 + 1, fx * fy),
                ] {
                    let p = sample(sx, sy);
                    if p == 0 {
                        continue;
                    }
                    acc[0] += (p >> 24) as f32 * k;
                    acc[1] += (p >> 16 & 0xFF) as f32 * k;
                    acc[2] += (p >> 8 & 0xFF) as f32 * k;
                    acc[3] += (p & 0xFF) as f32 * k;
                }
                let a = acc[0].round() as u32;
                if a == 0 {
                    continue;
                }
                let ch = |c: f32| (c.round() as u32).min(a);
                let mut p = a.min(255) << 24 | ch(acc[1]) << 16 | ch(acc[2]) << 8 | ch(acc[3]);
                if alpha < 255 {
                    p = scale_px(p, alpha);
                }
                let d = &mut self.px[py as usize * width + px as usize];
                *d = over(*d, p);
            }
        }
    }

    /// slot's `Draw::Quad`: the `uv` window of `s` (u, v, width, height in
    /// 0..1) mapped onto the quad `corners` (top-left, top-right,
    /// bottom-right, bottom-left), as slot's GL draws it: two triangles split
    /// along the top-right to bottom-left diagonal, texture coordinates
    /// interpolated linearly in each, sampled bilinearly with edges clamped.
    /// A top-left fill rule means quads that share an edge (the strips of a
    /// curling sticker) meet with no gap and no pixel drawn twice.
    pub fn blit_quad(&mut self, s: &Sprite, corners: [[f32; 2]; 4], uv: [f32; 4], alpha: u32) {
        if alpha == 0 || s.w == 0 || s.h == 0 {
            return;
        }
        // Texture coordinates of each corner, in sprite pixels (centres at
        // .5, as GL samples).
        let tex = |a: f32, b: f32| {
            [
                (uv[0] + a * uv[2]) * s.w as f32 - 0.5,
                (uv[1] + b * uv[3]) * s.h as f32 - 0.5,
            ]
        };
        let t = [tex(0.0, 0.0), tex(1.0, 0.0), tex(1.0, 1.0), tex(0.0, 1.0)];
        for (a, b, c) in [(0, 1, 3), (1, 2, 3)] {
            self.triangle(
                s,
                [corners[a], corners[b], corners[c]],
                [t[a], t[b], t[c]],
                alpha,
            );
        }
    }

    fn triangle(&mut self, s: &Sprite, p: [[f32; 2]; 3], t: [[f32; 2]; 3], alpha: u32) {
        let area =
            (p[1][0] - p[0][0]) * (p[2][1] - p[0][1]) - (p[1][1] - p[0][1]) * (p[2][0] - p[0][0]);
        if !area.is_finite() || area.abs() < 1e-6 {
            return;
        }
        // One winding for every triangle, so the fill rule is consistent.
        let (p, t) = match area > 0.0 {
            true => (p, t),
            false => ([p[0], p[2], p[1]], [t[0], t[2], t[1]]),
        };
        let area = area.abs();
        let min_x = p.iter().map(|q| q[0]).fold(f32::INFINITY, f32::min);
        let max_x = p.iter().map(|q| q[0]).fold(f32::NEG_INFINITY, f32::max);
        let min_y = p.iter().map(|q| q[1]).fold(f32::INFINITY, f32::min);
        let max_y = p.iter().map(|q| q[1]).fold(f32::NEG_INFINITY, f32::max);
        let x0 = ((min_x - 0.5).ceil() as i32).max(0);
        let x1 = ((max_x - 0.5).floor() as i32).min(self.w as i32 - 1);
        let y0 = ((min_y - 0.5).ceil() as i32).max(0);
        let y1 = ((max_y - 0.5).floor() as i32).min(self.h as i32 - 1);
        if x0 > x1 || y0 > y1 {
            return;
        }
        // Edge i runs from p[i+1] to p[i+2] and is 0 on that edge, `area`
        // at p[i]. Its weight is p[i]'s share of the texture coordinate.
        struct Edge {
            dx: f32,
            dy: f32,
            c: f32,
            /// A pixel exactly on this edge is drawn (top or left edges).
            owns: bool,
        }
        let edge = |i: usize| {
            let (a, b) = (p[(i + 1) % 3], p[(i + 2) % 3]);
            // w(x, y) = (b - a) x (q - a), positive inside for this winding.
            let (ex, ey) = (b[0] - a[0], b[1] - a[1]);
            Edge {
                dx: -ey,
                dy: ex,
                c: ey * a[0] - ex * a[1],
                owns: (ey == 0.0 && ex < 0.0) || ey > 0.0,
            }
        };
        let e = [edge(0), edge(1), edge(2)];
        // u and v are linear in x and y: the weights over the area.
        let lin = |k: usize| {
            let f =
                |g: &dyn Fn(&Edge) -> f32| (0..3).map(|i| g(&e[i]) * t[i][k]).sum::<f32>() / area;
            (f(&|e| e.dx), f(&|e| e.dy), f(&|e| e.c))
        };
        let (u, v) = (lin(0), lin(1));
        let (sw, sh) = (s.w as i32, s.h as i32);
        let width = self.w as usize;
        for py in y0..=y1 {
            let fy = py as f32 + 0.5;
            for px in x0..=x1 {
                let fx = px as f32 + 0.5;
                let inside = e.iter().all(|e| {
                    let w = e.dx * fx + e.dy * fy + e.c;
                    w > 0.0 || (w == 0.0 && e.owns)
                });
                if !inside {
                    continue;
                }
                let su = (u.0 * fx + u.1 * fy + u.2).clamp(0.0, (sw - 1) as f32);
                let sv = (v.0 * fx + v.1 * fy + v.2).clamp(0.0, (sh - 1) as f32);
                let (ix, iy) = (su as i32, sv as i32);
                let (wx, wy) = (
                    ((su - ix as f32) * 256.0) as u32,
                    ((sv - iy as f32) * 256.0) as u32,
                );
                let (ix1, iy1) = ((ix + 1).min(sw - 1), (iy + 1).min(sh - 1));
                let at = |x: i32, y: i32| s.px[(y * sw + x) as usize];
                let top = lerp_px(at(ix, iy), at(ix1, iy), wx);
                let bottom = lerp_px(at(ix, iy1), at(ix1, iy1), wx);
                let mut c = lerp_px(top, bottom, wy);
                if c >> 24 == 0 {
                    continue;
                }
                if alpha < 255 {
                    c = scale_px(c, alpha);
                }
                let d = &mut self.px[py as usize * width + px as usize];
                *d = over(*d, c);
            }
        }
    }

    /// Blend an 8-bit coverage mask (text) in one colour.
    pub fn blit_mask(
        &mut self,
        mask: &[u8],
        mw: u32,
        mh: u32,
        x: i32,
        y: i32,
        rgb: u32,
        alpha: u32,
    ) {
        let dst = Rect::new(x, y, mw as i32, mh as i32);
        let c = dst.intersect(&self.bounds());
        if c.is_empty() || alpha == 0 {
            return;
        }
        let solid = rgb & 0x00FF_FFFF | 0xFF00_0000;
        let w = self.w as usize;
        for row in 0..c.h {
            let sy = (c.y - y + row) as usize;
            for col in 0..c.w {
                let sx = (c.x - x + col) as usize;
                let cov = mask[sy * mw as usize + sx] as u32;
                if cov == 0 {
                    continue;
                }
                let a = (cov * alpha + 127) / 255;
                let i = (c.y + row) as usize * w + (c.x + col) as usize;
                self.px[i] = over(self.px[i], scale_px(solid, a));
            }
        }
    }

    /// Straight RGBA bytes, for PNG output and tests.
    pub fn to_rgba(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.px.len() * 4);
        for &p in &self.px {
            out.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, p as u8, 255]);
        }
        out
    }

    /// Copy of a region as an opaque sprite (for baking backgrounds).
    pub fn snapshot(&self) -> Sprite {
        Sprite {
            w: self.w,
            h: self.h,
            px: self.px.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_turned_blit_matches_a_plain_one_unturned_and_turns_clockwise() {
        // A 20x10 white bar.
        let s = Sprite {
            w: 20,
            h: 10,
            px: vec![0xFFFF_FFFF; 200],
        };
        let mut flat = Canvas::new(40, 40);
        flat.blit_turned(&s, 10.0, 15.0, 20.0, 10.0, 0.0, 255);
        let mut plain = Canvas::new(40, 40);
        plain.blit(&s, 10, 15, 255);
        assert_eq!(flat.px, plain.px);
        // A quarter turn stands the bar up: 10 wide, 20 tall, same centre.
        let mut up = Canvas::new(40, 40);
        up.blit_turned(&s, 10.0, 15.0, 20.0, 10.0, std::f32::consts::FRAC_PI_2, 255);
        let lit = |c: &Canvas, x: i32, y: i32| c.px[(y * 40 + x) as usize] == 0xFFFF_FFFF;
        assert!(lit(&up, 20, 12) && lit(&up, 20, 28));
        assert!(!lit(&up, 12, 20) && !lit(&up, 28, 20));
        // Clockwise: a mark at the bar's right end ends up at the bottom.
        let mut marked = s.clone();
        for y in 0..10 {
            marked.px[y * 20 + 19] = 0xFF00_00FF;
        }
        let mut cw = Canvas::new(40, 40);
        cw.blit_turned(
            &marked,
            10.0,
            15.0,
            20.0,
            10.0,
            std::f32::consts::FRAC_PI_2,
            255,
        );
        assert_eq!(cw.px[(29 * 40 + 20) as usize], 0xFF00_00FF);
    }

    #[test]
    fn scale_is_exact_at_the_ends() {
        assert_eq!(scale_px(0xFFFF_FFFF, 255), 0xFFFF_FFFF);
        assert_eq!(scale_px(0xFFFF_FFFF, 0), 0);
        assert_eq!(scale_px(0xFF80_4020, 128), 0x8040_2010);
    }

    #[test]
    fn half_white_over_black_is_grey() {
        let src = premultiply(255, 255, 255, 128);
        assert_eq!(over(BLACK, src), 0xFF80_8080);
    }

    #[test]
    fn rgba_round_trip() {
        let rgba = [10, 20, 30, 255, 200, 100, 50, 128, 0, 0, 0, 0];
        let s = Sprite::from_rgba(3, 1, &rgba);
        let back = s.to_rgba();
        assert_eq!(&back[..4], &rgba[..4]);
        for (a, b) in back[4..8].iter().zip(&rgba[4..8]) {
            assert!((*a as i32 - *b as i32).abs() <= 2);
        }
        assert_eq!(&back[8..], &[0, 0, 0, 0]);
    }

    #[test]
    fn blit_clips_at_every_edge() {
        let mut c = Canvas::new(4, 4);
        let s = Sprite::from_rgba(2, 2, &[255; 16]);
        for (x, y) in [(-1, -1), (3, 3), (-5, 0), (1, 1)] {
            c.blit(&s, x, y, 255);
        }
        assert_eq!(c.px[0], 0xFFFF_FFFF);
        assert_eq!(c.px[15], 0xFFFF_FFFF);
        assert_eq!(c.px[3], BLACK);
    }

    #[test]
    fn scaled_blit_fills_the_destination() {
        let mut c = Canvas::new(8, 8);
        let s = Sprite::from_rgba(2, 2, &[255; 16]);
        c.blit_scaled(&s, [0.0, 0.0, 2.0, 2.0], Rect::new(2, 2, 4, 4), 255);
        let white = c.px.iter().filter(|&&p| p == 0xFFFF_FFFF).count();
        assert_eq!(white, 16);
    }

    #[test]
    fn rotation_keeps_the_middle_and_grows() {
        let s = Sprite::from_rgba(10, 4, &[255; 160]);
        let r = s.rotated(90.0);
        assert!(r.w >= 4 && r.h >= 10);
        assert_eq!(r.px[((r.h / 2) * r.w + r.w / 2) as usize], 0xFFFF_FFFF);
        assert_eq!(r.px[0], 0, "corners are transparent");
    }

    #[test]
    fn draw_over_keeps_transparency() {
        let mut base = Sprite::new(2, 1);
        let red = Sprite::from_rgba(1, 1, &[255, 0, 0, 255]);
        base.draw_over(&red, 1, 0);
        assert_eq!(base.px, vec![0, 0xFFFF_0000]);
    }

    #[test]
    fn resize_averages() {
        // Black and white columns average to grey.
        let s = Sprite::from_rgba(2, 1, &[0, 0, 0, 255, 255, 255, 255, 255]);
        let r = s.resized(1, 1);
        assert_eq!(r.px[0], 0xFF80_8080);
    }

    fn numbered(w: u32, h: u32) -> Sprite {
        let mut s = Sprite::new(w, h);
        for (i, p) in s.px.iter_mut().enumerate() {
            *p = 0xFF00_0000 | (i as u32 * 16) << 8 | 0x40;
        }
        s
    }

    const WHOLE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

    #[test]
    fn a_square_quad_is_a_straight_copy() {
        let s = numbered(4, 4);
        let mut c = Canvas::new(6, 6);
        c.clear(0);
        c.blit_quad(
            &s,
            [[1.0, 1.0], [5.0, 1.0], [5.0, 5.0], [1.0, 5.0]],
            WHOLE,
            255,
        );
        for y in 0..4 {
            for x in 0..4 {
                assert_eq!(
                    c.px[((y + 1) * 6 + x + 1) as usize],
                    s.px[(y * 4 + x) as usize]
                );
            }
        }
        assert_eq!(c.px[0], BLACK, "outside is untouched");
        assert_eq!(c.px[35], BLACK);
    }

    #[test]
    fn a_mirrored_quad_mirrors() {
        let s = numbered(4, 1);
        let mut c = Canvas::new(4, 1);
        c.blit_quad(
            &s,
            [[4.0, 0.0], [0.0, 0.0], [0.0, 1.0], [4.0, 1.0]],
            WHOLE,
            255,
        );
        for x in 0..4 {
            assert_eq!(c.px[x], s.px[3 - x]);
        }
    }

    #[test]
    fn quads_sharing_an_edge_meet_without_gaps_or_overlap() {
        let mut s = Sprite::new(2, 2);
        s.px.fill(0x8080_8080); // white at half alpha, premultiplied
        let mut c = Canvas::new(32, 16);
        c.clear(0);
        // A slanted shared edge, as two strips of a curl.
        let (top, bottom) = ([13.3, 1.2], [17.9, 14.7]);
        c.blit_quad(&s, [[2.0, 1.2], top, bottom, [2.0, 14.7]], WHOLE, 255);
        c.blit_quad(&s, [top, [29.0, 1.2], [29.0, 14.7], bottom], WHOLE, 255);
        let once = over(BLACK, 0x8080_8080);
        for y in 2..14 {
            for x in 2..29 {
                assert_eq!(c.px[y * 32 + x], once, "({x}, {y})");
            }
        }
    }

    #[test]
    fn a_degenerate_quad_draws_nothing() {
        let s = numbered(2, 2);
        let mut c = Canvas::new(4, 4);
        c.clear(0);
        c.blit_quad(
            &s,
            [[1.0, 1.0], [3.0, 1.0], [3.0, 1.0], [1.0, 1.0]],
            WHOLE,
            255,
        );
        assert!(c.px.iter().all(|&p| p == BLACK));
    }
}
