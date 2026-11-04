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
    fn resize_averages() {
        // Black and white columns average to grey.
        let s = Sprite::from_rgba(2, 1, &[0, 0, 0, 255, 255, 255, 255, 255]);
        let r = s.resized(1, 1);
        assert_eq!(r.px[0], 0xFF80_8080);
    }
}
