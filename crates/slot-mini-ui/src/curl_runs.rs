//! Fewer quads for a curling sticker. slot's `sticker_curl::strips` cuts a
//! sticker into 8 px strips, which a GPU draws for free. Software pays per
//! strip and per row, so strips that lie in one plane (the flat part of the
//! paper) are joined back into one quad. They share the plane and the
//! perspective, so the picture is the same.

use crate::sticker_curl::Strip;

/// Join neighbouring strips that continue each other in one plane. `strips`
/// is in slot's drawing order (by depth); only strips next to each other
/// in that order are joined, so the order still holds.
pub fn joined(strips: Vec<Strip>) -> Vec<Strip> {
    let mut out: Vec<Strip> = Vec::with_capacity(strips.len());
    for s in strips {
        if let Some(last) = out.last_mut() {
            if continues(last, &s) {
                last.corners[1] = s.corners[1];
                last.corners[2] = s.corners[2];
                last.uv[2] += s.uv[2];
                continue;
            }
        }
        out.push(s);
    }
    out
}

/// slot's strips `n` at a time: each group becomes one quad from its first
/// strip's left edge to its last strip's right edge, a chord across the
/// curl. For a sticker drawn large, where 8 px strips cost too much in
/// software (the maintainer's choice: wider strips before no curl).
pub fn wider(mut strips: Vec<Strip>, n: usize) -> Vec<Strip> {
    if n <= 1 {
        return strips;
    }
    strips.sort_by(|a, b| a.uv[0].total_cmp(&b.uv[0]));
    let mut out: Vec<Strip> = Vec::with_capacity(strips.len() / n + 1);
    let mut count = 0;
    for s in strips {
        match out.last_mut() {
            Some(last) if count < n && last.back == s.back => {
                last.corners[1] = s.corners[1];
                last.corners[2] = s.corners[2];
                last.uv[2] += s.uv[2];
                last.z = (last.z * count as f32 + s.z) / (count + 1) as f32;
                count += 1;
            }
            _ => {
                out.push(s);
                count = 1;
            }
        }
    }
    out.sort_by(|a, b| a.z.total_cmp(&b.z));
    out
}

fn continues(a: &Strip, b: &Strip) -> bool {
    let near = |p: [f32; 2], q: [f32; 2]| (p[0] - q[0]).abs() < 1e-3 && (p[1] - q[1]).abs() < 1e-3;
    let dir = |p: [f32; 2], q: [f32; 2]| [q[0] - p[0], q[1] - p[1]];
    let parallel = |d: [f32; 2], e: [f32; 2]| {
        let cross = d[0] * e[1] - d[1] * e[0];
        let dot = d[0] * e[0] + d[1] * e[1];
        dot > 0.0 && cross.abs() <= 1e-4 * dot
    };
    a.back == b.back
        && a.z == b.z
        && near(a.corners[1], b.corners[0])
        && near(a.corners[2], b.corners[3])
        && (a.uv[0] + a.uv[2] - b.uv[0]).abs() < 1e-5
        && parallel(
            dir(a.corners[0], a.corners[1]),
            dir(b.corners[0], b.corners[1]),
        )
        && parallel(
            dir(a.corners[3], a.corners[2]),
            dir(b.corners[3], b.corners[2]),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sticker_curl::{strips, Pose};

    #[test]
    fn a_flat_sticker_is_one_quad() {
        let s = joined(strips(
            [10.0, 20.0, 205.0, 106.0],
            1.0,
            0.0,
            &Pose::REST,
            [320.0, 240.0],
        ));
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].corners[0], [10.0, 20.0]);
        assert!((s[0].corners[2][0] - 215.0).abs() < 1e-3);
        assert!((s[0].uv[2] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn the_curled_end_keeps_its_strips() {
        let all = strips(
            [10.0, 20.0, 205.0, 106.0],
            0.5,
            200.0,
            &Pose::REST,
            [320.0, 240.0],
        );
        let n = all.len();
        let s = joined(all);
        assert!(s.len() > 1 && s.len() < n, "{} of {n}", s.len());
    }
}
