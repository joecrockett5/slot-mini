//! Text drawn straight into the canvas through fontdue, with a glyph cache so
//! that strings which change every frame (the clock) cost a few blits.

use std::collections::HashMap;

use crate::canvas::Canvas;

struct Glyph {
    w: u32,
    h: u32,
    xmin: i32,
    ymin: i32,
    advance: f32,
    mask: Vec<u8>,
}

pub struct Text {
    font: fontdue::Font,
    cache: HashMap<(char, u32), Glyph>,
}

impl Text {
    pub fn new(ttf: &[u8]) -> Option<Text> {
        let font = fontdue::Font::from_bytes(ttf, fontdue::FontSettings::default()).ok()?;
        Some(Text {
            font,
            cache: HashMap::new(),
        })
    }

    fn glyph(&mut self, c: char, px: f32) -> &Glyph {
        let key = (c, (px * 4.0).round() as u32);
        let font = &self.font;
        self.cache.entry(key).or_insert_with(|| {
            let (m, mask) = font.rasterize(c, px);
            Glyph {
                w: m.width as u32,
                h: m.height as u32,
                xmin: m.xmin,
                ymin: m.ymin,
                advance: m.advance_width,
                mask,
            }
        })
    }

    /// Width of `s` at `px` with extra `tracking` between letters.
    pub fn width(&mut self, s: &str, px: f32, tracking: f32) -> f32 {
        let n = s.chars().count();
        let w: f32 = s.chars().map(|c| self.glyph(c, px).advance).sum();
        w + tracking * n.saturating_sub(1) as f32
    }

    /// Ascent above the baseline at `px`.
    pub fn ascent(&self, px: f32) -> f32 {
        self.font
            .horizontal_line_metrics(px)
            .map(|m| m.ascent)
            .unwrap_or(px * 0.8)
    }

    /// Draw `s` with its top-left at (x, y). Returns the pen x after it.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        canvas: &mut Canvas,
        s: &str,
        x: f32,
        y: f32,
        px: f32,
        tracking: f32,
        rgb: u32,
        alpha: u32,
    ) -> f32 {
        let base = y + self.ascent(px);
        let mut pen = x;
        for c in s.chars() {
            self.glyph(c, px);
            let g = &self.cache[&(c, (px * 4.0).round() as u32)];
            let gx = (pen + g.xmin as f32).round() as i32;
            let gy = (base - g.ymin as f32 - g.h as f32).round() as i32;
            if g.w > 0 && g.h > 0 {
                canvas.blit_mask(&g.mask, g.w, g.h, gx, gy, rgb, alpha);
            }
            let adv = g.advance;
            pen += adv + tracking;
        }
        pen
    }

    pub fn cached_glyphs(&self) -> usize {
        self.cache.len()
    }
}
