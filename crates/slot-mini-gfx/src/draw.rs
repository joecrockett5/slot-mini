// Adapted from slot (https://github.com/BrandonKowalski/slot) at e66715d:
//   crates/slot-gfx/src/draw.rs  -> `Draw`, `TexId` (GL passes removed; the
//                                   Paper/Game/Shot/Capture variants kept so
//                                   copied layout code compiles, never drawn)
//   crates/slot-gfx/src/layout.rs -> `Stage` (slot's `Canvas`, renamed so the
//                                   pixel buffer can be called `Canvas`)
//   crates/slot-gfx/src/bezel.rs  -> `framed`
//   crates/slot-gfx/src/surface.rs -> `OUT_W`, `OUT_H`
// Copyright (C) Brandon Kowalski and slot contributors. GPL-3.0-or-later.
// The `Renderer` below is new: a software stand-in for slot's GL compositor.

use std::collections::HashMap;

use crate::canvas::{Canvas, Rect, Sprite};

pub const OUT_W: u32 = 720;
pub const OUT_H: u32 = 480;

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct TexId(usize);

impl TexId {
    pub const fn from_raw(id: usize) -> Self {
        TexId(id)
    }
    pub const fn raw(self) -> usize {
        self.0
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum Draw {
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        colour: [f32; 4],
    },
    Tex {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        tex: TexId,
        alpha: f32,
    },
    Part {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        tex: TexId,
        alpha: f32,
        uv: [f32; 4],
    },
    Turned {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        tex: TexId,
        alpha: f32,
        turn: f32,
    },
    Quad {
        corners: [[f32; 2]; 4],
        tex: TexId,
        uv: [f32; 4],
        alpha: f32,
    },
    Paper {
        tex: TexId,
        pivot: [f32; 2],
        scale: f32,
        lift: f32,
        fold: f32,
        tear: f32,
        bow: bool,
        flap: bool,
    },
    Game,
    Shot {
        tex: TexId,
    },
    Capture,
}

pub fn framed(panel: (u32, u32)) -> bool {
    panel.0 > 0 && panel.0 * 3 == panel.1 * 4
}

/// slot's layout `Canvas`: the logical size frames are laid out at.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Stage {
    pub w: f32,
    pub h: f32,
}

impl Stage {
    pub const FRAMED: Stage = Stage {
        w: OUT_W as f32,
        h: OUT_H as f32,
    };

    /// The Mini Plus: 640x480, laid out 1:1.
    pub const MINI: Stage = Stage { w: 640.0, h: 480.0 };

    pub fn of(panel: (u32, u32)) -> Stage {
        match framed(panel) {
            true => Stage {
                w: (OUT_H * panel.0 / panel.1) as f32,
                h: OUT_H as f32,
            },
            false => Stage::FRAMED,
        }
    }

    pub fn centred(self, w: f32) -> f32 {
        (self.w - w) / 2.0
    }

    pub fn window(self, tex_w: f32) -> [f32; 4] {
        if tex_w <= self.w {
            return [0.0, 0.0, 1.0, 1.0];
        }
        [(tex_w - self.w) / 2.0 / tex_w, 0.0, self.w / tex_w, 1.0]
    }

    pub fn cover(self, tex_w: f32) -> [f32; 4] {
        if tex_w >= self.w {
            return self.window(tex_w);
        }
        let shown = tex_w / self.w;
        [0.0, (1.0 - shown) / 2.0, 1.0, shown]
    }
}

/// Two sizes count as the same pinned variant when both edges are within
/// this many pixels (slot's layout rounds each edge separately).
const PIN_SLACK: i32 = 1;

/// Textures by id, and a software executor for slot's `Draw` lists.
#[derive(Default)]
pub struct Renderer {
    textures: Vec<Option<Sprite>>,
    free: Vec<usize>,
    /// Pre-scaled copies of a texture, drawn when a draw asks for about
    /// their size. Built off the main thread by whoever made the texture.
    pinned: HashMap<usize, Vec<Sprite>>,
    /// Bumped by every change to any texture, so a caller can tell that the
    /// same draw list would now look different.
    generation: u64,
}

impl Renderer {
    pub fn new() -> Renderer {
        Renderer::default()
    }

    /// Changes whenever any texture is uploaded, replaced, pinned or freed.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn upload(&mut self, sprite: Sprite) -> TexId {
        self.generation += 1;
        match self.free.pop() {
            Some(i) => {
                self.textures[i] = Some(sprite);
                TexId(i)
            }
            None => {
                self.textures.push(Some(sprite));
                TexId(self.textures.len() - 1)
            }
        }
    }

    pub fn upload_rgba(&mut self, w: u32, h: u32, rgba: &[u8]) -> TexId {
        self.upload(Sprite::from_rgba(w, h, rgba))
    }

    /// Replace a texture's pixels, keeping its id. Pinned copies are dropped.
    pub fn replace(&mut self, id: TexId, sprite: Sprite) {
        self.generation += 1;
        if let Some(slot) = self.textures.get_mut(id.0) {
            *slot = Some(sprite);
        }
        self.pinned.remove(&id.0);
    }

    /// Attach a pre-scaled copy of `id`, used for draws of about its size.
    pub fn pin_scaled(&mut self, id: TexId, sprite: Sprite) {
        self.generation += 1;
        if self.get(id).is_some() {
            self.pinned.entry(id.0).or_default().push(sprite);
        }
    }

    pub fn free(&mut self, id: TexId) {
        self.generation += 1;
        if let Some(slot) = self.textures.get_mut(id.0) {
            if slot.take().is_some() {
                self.free.push(id.0);
            }
        }
        self.pinned.remove(&id.0);
    }

    pub fn get(&self, id: TexId) -> Option<&Sprite> {
        self.textures.get(id.0).and_then(Option::as_ref)
    }

    /// Bytes held by textures and their pinned copies.
    pub fn bytes(&self) -> usize {
        let t: usize = self.textures.iter().flatten().map(|s| s.px.len() * 4).sum();
        let p: usize = self.pinned.values().flatten().map(|s| s.px.len() * 4).sum();
        t + p
    }

    pub fn live_textures(&self) -> usize {
        self.textures.iter().flatten().count()
    }

    /// Draw a list into the canvas. Operations slot only uses for in-game
    /// and GL-only effects (`Game`, `Shot`, `Capture`, `Paper`, `Quad`) are
    /// skipped; screens that need them pre-render instead (see
    /// docs/DESIGN-PARITY.md). `Turned` is drawn in software (the core
    /// picker's lid and chip).
    pub fn render(&self, draws: &[Draw], canvas: &mut Canvas) {
        for d in draws {
            match *d {
                Draw::Rect { x, y, w, h, colour } => canvas.fill_f(Rect::snap(x, y, w, h), colour),
                Draw::Tex {
                    x,
                    y,
                    w,
                    h,
                    tex,
                    alpha,
                } => self.tex(canvas, tex, None, Rect::snap(x, y, w, h), alpha),
                Draw::Part {
                    x,
                    y,
                    w,
                    h,
                    tex,
                    alpha,
                    uv,
                } => self.tex(canvas, tex, Some(uv), Rect::snap(x, y, w, h), alpha),
                Draw::Turned {
                    x,
                    y,
                    w,
                    h,
                    tex,
                    alpha,
                    turn,
                } => {
                    let a = (alpha.clamp(0.0, 1.0) * 255.0).round() as u32;
                    if let Some(sprite) = self.get(tex) {
                        match turn == 0.0 {
                            true => self.tex(canvas, tex, None, Rect::snap(x, y, w, h), alpha),
                            false => canvas.blit_turned(sprite, x, y, w, h, turn, a),
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn tex(&self, canvas: &mut Canvas, tex: TexId, uv: Option<[f32; 4]>, dst: Rect, alpha: f32) {
        let a = (alpha.clamp(0.0, 1.0) * 255.0).round() as u32;
        if a == 0 || dst.is_empty() || dst.intersect(&canvas.bounds()).is_empty() {
            return;
        }
        let Some(sprite) = self.get(tex) else {
            return;
        };
        let whole = uv.is_none_or(|uv| uv == [0.0, 0.0, 1.0, 1.0]);
        if whole && dst.w as u32 == sprite.w && dst.h as u32 == sprite.h {
            return canvas.blit(sprite, dst.x, dst.y, a);
        }
        let src = match uv {
            Some([u, v, uw, vh]) => [
                u * sprite.w as f32,
                v * sprite.h as f32,
                uw * sprite.w as f32,
                vh * sprite.h as f32,
            ],
            None => [0.0, 0.0, sprite.w as f32, sprite.h as f32],
        };
        // A 1:1 window onto the sprite (slot's 720-wide art on a 640 screen):
        // copy that region straight.
        if (src[2] - dst.w as f32).abs() < 0.5 && (src[3] - dst.h as f32).abs() < 0.5 {
            let (sx, sy) = (src[0].round() as i32, src[1].round() as i32);
            return canvas.blit_region(sprite, sx, sy, dst, a);
        }
        if whole {
            let near = |s: &&Sprite| {
                (s.w as i32 - dst.w).abs() <= PIN_SLACK && (s.h as i32 - dst.h).abs() <= PIN_SLACK
            };
            if let Some(s) = self.pinned.get(&tex.0).and_then(|v| v.iter().find(near)) {
                return canvas.blit(s, dst.x, dst.y, a);
            }
        }
        canvas.blit_scaled(sprite, src, dst, a)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mini_stage_is_640() {
        assert_eq!(Stage::of((640, 480)), Stage::MINI);
    }

    #[test]
    fn freed_ids_are_reused() {
        let mut r = Renderer::new();
        let a = r.upload(Sprite::new(1, 1));
        r.free(a);
        let b = r.upload(Sprite::new(2, 2));
        assert_eq!(a, b);
        assert_eq!(r.get(b).unwrap().w, 2);
    }

    #[test]
    fn pinned_copies_are_used_near_their_size() {
        let mut r = Renderer::new();
        let t = r.upload(Sprite::from_rgba(4, 4, &[255; 64]));
        r.pin_scaled(t, Sprite::from_rgba(3, 3, &[0, 0, 0, 255].repeat(9)));
        let mut c = Canvas::new(8, 8);
        let d = |w: f32| Draw::Tex {
            x: 0.0,
            y: 0.0,
            w,
            h: w,
            tex: t,
            alpha: 1.0,
        };
        c.clear(0xFFFFFF);
        r.render(&[d(3.0)], &mut c);
        assert_eq!(
            c.px[0],
            crate::BLACK,
            "3x3 comes from the black pinned copy"
        );
        c.clear(0xFFFFFF);
        r.render(&[d(6.0)], &mut c);
        assert_eq!(c.px[0], 0xFFFF_FFFF, "6x6 is scaled from the white texture");
    }

    #[test]
    fn a_one_to_one_window_is_copied_straight() {
        let mut r = Renderer::new();
        let mut rgba = vec![0u8; 4 * 2 * 4];
        rgba[4..8].copy_from_slice(&[255, 0, 0, 255]); // (1, 0) is red
        let t = r.upload_rgba(4, 2, &rgba);
        let mut c = Canvas::new(2, 2);
        r.render(
            &[Draw::Part {
                x: 0.0,
                y: 0.0,
                w: 2.0,
                h: 2.0,
                tex: t,
                alpha: 1.0,
                uv: [0.25, 0.0, 0.5, 1.0],
            }],
            &mut c,
        );
        assert_eq!(c.px[0], 0xFFFF_0000);
    }
}
