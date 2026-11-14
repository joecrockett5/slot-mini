//! Putting the 640x480 frame in a window of any size: scaled by a whole
//! number when the window is at least the frame's size (pixels stay
//! square and sharp), shrunk smoothly when it is not, centred, with the
//! borders filled by the wallpaper, blurred and dimmed.

use slot_mini_gfx::{Canvas, Sprite};

/// Where the frame lands in the window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

impl View {
    /// Fit a `fw` x `fh` frame in a `w` x `h` window.
    pub fn fit(fw: u32, fh: u32, w: u32, h: u32) -> View {
        let (w, h) = (w.max(1), h.max(1));
        let whole = (w / fw).min(h / fh);
        let (vw, vh) = match whole {
            // Smaller than the frame: as large as fits, in proportion.
            0 => {
                let s = (w as f32 / fw as f32).min(h as f32 / fh as f32);
                (
                    ((fw as f32 * s).floor() as u32).max(1),
                    ((fh as f32 * s).floor() as u32).max(1),
                )
            }
            k => (fw * k, fh * k),
        };
        View {
            x: (w as i32 - vw as i32) / 2,
            y: (h as i32 - vh as i32) / 2,
            w: vw,
            h: vh,
        }
    }

    /// A point in the window as a point on the frame, if it is on it.
    pub fn to_frame(&self, fw: u32, fh: u32, x: f64, y: f64) -> Option<(f32, f32)> {
        let (dx, dy) = (x - self.x as f64, y - self.y as f64);
        if dx < 0.0 || dy < 0.0 || dx >= self.w as f64 || dy >= self.h as f64 {
            return None;
        }
        Some((
            (dx * fw as f64 / self.w as f64) as f32,
            (dy * fh as f64 / self.h as f64) as f32,
        ))
    }
}

/// How dark the borders are: this much of the wallpaper's light.
const DIM: u32 = 90;
/// The wallpaper is blurred by shrinking it to this width and back.
const BLUR_W: u32 = 40;

/// The window's borders: the wallpaper, blurred, dimmed and scaled to
/// cover the window. Made again only when the window or wallpaper changes.
#[derive(Default)]
pub struct Border {
    key: Option<(u32, u32, u64)>,
    px: Vec<u32>,
}

impl Border {
    fn make(&mut self, w: u32, h: u32, wallpaper: Option<(&Sprite, u64)>) {
        let key = (w, h, wallpaper.map_or(u64::MAX, |(_, id)| id));
        if self.key == Some(key) {
            return;
        }
        self.key = Some(key);
        let Some((art, _)) = wallpaper else {
            self.px = vec![0; (w * h) as usize];
            return;
        };
        // Cover the window: the same crop a wallpaper gets on the shelf.
        let small_h = (BLUR_W * h / w.max(1)).max(2);
        let small = cover(art, BLUR_W, small_h);
        let big = small.resized(w, h);
        self.px = big
            .px
            .iter()
            .map(|&p| {
                let c = |shift: u32| (((p >> shift) & 0xff) * DIM / 255) << shift;
                c(16) | c(8) | c(0)
            })
            .collect();
    }
}

/// `art` scaled and cropped to fill `w` x `h`.
fn cover(art: &Sprite, w: u32, h: u32) -> Sprite {
    let s = (w as f32 / art.w as f32).max(h as f32 / art.h as f32);
    let (sw, sh) = (
        ((art.w as f32 * s).ceil() as u32).max(w),
        ((art.h as f32 * s).ceil() as u32).max(h),
    );
    let scaled = art.resized(sw, sh);
    let (ox, oy) = ((sw - w) / 2, (sh - h) / 2);
    let mut out = Sprite::new(w, h);
    for y in 0..h {
        let from = ((y + oy) * sw + ox) as usize;
        let to = (y * w) as usize;
        out.px[to..to + w as usize].copy_from_slice(&scaled.px[from..from + w as usize]);
    }
    out
}

/// Draw `frame` into a `w` x `h` window buffer of `0x00RRGGBB` pixels.
pub fn present(
    frame: &Canvas,
    border: &mut Border,
    wallpaper: Option<(&Sprite, u64)>,
    out: &mut [u32],
    w: u32,
    h: u32,
) -> View {
    let view = View::fit(frame.w, frame.h, w, h);
    if out.len() < (w * h) as usize {
        return view;
    }
    if view.w < w || view.h < h {
        border.make(w, h, wallpaper);
        out[..border.px.len()].copy_from_slice(&border.px);
    }
    let k = view.w / frame.w;
    if k >= 1 && view.w == frame.w * k {
        // Whole pixels: each frame row is written k times.
        for fy in 0..frame.h {
            let row = &frame.px[(fy * frame.w) as usize..((fy + 1) * frame.w) as usize];
            let first = ((view.y as u32 + fy * k) * w + view.x as u32) as usize;
            if k == 1 {
                out[first..first + row.len()].copy_from_slice(row);
            } else {
                for (fx, &p) in row.iter().enumerate() {
                    let at = first + fx * k as usize;
                    out[at..at + k as usize].fill(p);
                }
                for dup in 1..k {
                    let to = first + (dup * w) as usize;
                    out.copy_within(first..first + view.w as usize, to);
                }
            }
        }
    } else {
        // Shrunk: bilinear through the sprite scaler.
        let small = frame.snapshot().resized(view.w, view.h);
        for y in 0..view.h {
            let from = (y * view.w) as usize;
            let to = ((view.y as u32 + y) * w + view.x as u32) as usize;
            out[to..to + view.w as usize].copy_from_slice(&small.px[from..from + view.w as usize]);
        }
    }
    for p in out.iter_mut() {
        *p &= 0x00ff_ffff;
    }
    view
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_at_least_the_frames_size_scales_by_whole_numbers() {
        assert_eq!(
            View::fit(640, 480, 640, 480),
            View {
                x: 0,
                y: 0,
                w: 640,
                h: 480
            }
        );
        assert_eq!(
            View::fit(640, 480, 1920, 1080),
            View {
                x: 320,
                y: 60,
                w: 1280,
                h: 960
            }
        );
        assert_eq!(
            View::fit(640, 480, 2560, 1440),
            View {
                x: 320,
                y: 0,
                w: 1920,
                h: 1440
            }
        );
        assert_eq!(
            View::fit(640, 480, 1000, 500),
            View {
                x: 180,
                y: 10,
                w: 640,
                h: 480
            }
        );
    }

    #[test]
    fn a_smaller_window_shrinks_the_frame_in_proportion() {
        assert_eq!(
            View::fit(640, 480, 320, 400),
            View {
                x: 0,
                y: 80,
                w: 320,
                h: 240
            }
        );
    }

    #[test]
    fn the_mouse_maps_back_onto_the_frame() {
        let v = View::fit(640, 480, 1920, 1080);
        assert_eq!(v.to_frame(640, 480, 320.0, 60.0), Some((0.0, 0.0)));
        assert_eq!(v.to_frame(640, 480, 960.0, 540.0), Some((320.0, 240.0)));
        assert_eq!(v.to_frame(640, 480, 100.0, 540.0), None);
    }

    #[test]
    fn a_doubled_frame_keeps_its_pixels_and_fills_the_borders() {
        let mut frame = Canvas::new(4, 2);
        frame.px = vec![1, 2, 3, 4, 5, 6, 7, 8];
        let mut art = Sprite::new(8, 8);
        art.px.fill(0xffff_ffff);
        let (w, h) = (10, 6);
        let mut out = vec![0u32; (w * h) as usize];
        let mut border = Border::default();
        let v = present(&frame, &mut border, Some((&art, 1)), &mut out, w, h);
        assert_eq!(
            v,
            View {
                x: 1,
                y: 1,
                w: 8,
                h: 4
            }
        );
        assert_eq!(&out[11..19], &[1, 1, 2, 2, 3, 3, 4, 4]);
        assert_eq!(&out[21..29], &[1, 1, 2, 2, 3, 3, 4, 4]);
        assert_eq!(&out[31..39], &[5, 5, 6, 6, 7, 7, 8, 8]);
        // The border is the white wallpaper, dimmed.
        assert_eq!(out[0], 0x005a_5a5a);
        assert_eq!(out[59], 0x005a_5a5a);
    }
}
