//! Pixel buffer helpers on tight or strided premultiplied BGRA data.

use std::collections::HashMap;

/// Copies a rectangle out of `buf` into a tight buffer (stride `w * 4`).
pub fn read_region(buf: &[u8], stride: usize, x: i32, y: i32, w: i32, h: i32) -> Vec<u8> {
    let (x, y, w, h) = (x as usize, y as usize, w as usize, h as usize);
    let mut out = Vec::with_capacity(w * h * 4);
    for row in y..y + h {
        let o = row * stride + x * 4;
        out.extend_from_slice(&buf[o..o + w * 4]);
    }
    out
}

/// Writes a tight buffer into a rectangle of `buf`.
pub fn write_region(buf: &mut [u8], stride: usize, x: i32, y: i32, w: i32, h: i32, src: &[u8]) {
    let (x, y, w, h) = (x as usize, y as usize, w as usize, h as usize);
    for r in 0..h {
        let o = (y + r) * stride + x * 4;
        buf[o..o + w * 4].copy_from_slice(&src[r * w * 4..(r + 1) * w * 4]);
    }
}

/// The color under (x, y), clamped into the image, as un-premultiplied 0..1 RGB.
pub fn pick_pixel(buf: &[u8], stride: usize, w: i32, h: i32, px: f64, py: f64) -> (f64, f64, f64) {
    let x = (px.max(0.0).min((w - 1) as f64)) as usize;
    let y = (py.max(0.0).min((h - 1) as f64)) as usize;
    let o = y * stride + x * 4;
    let (mut b, mut g, mut r, a) = (buf[o] as u32, buf[o + 1] as u32, buf[o + 2] as u32, buf[o + 3] as u32);
    if a > 0 && a < 255 {
        r = (r * 255 / a).min(255);
        g = (g * 255 / a).min(255);
        b = (b * 255 / a).min(255);
    }
    (r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0)
}

/// Most common color in the 1 px ring just outside the rectangle, the background guess.
/// Ties go to the color seen first. White when the ring is outside the image.
#[allow(clippy::too_many_arguments)]
pub fn ring_color(buf: &[u8], stride: usize, iw: i32, ih: i32, x: i32, y: i32, w: i32, h: i32) -> (f64, f64, f64) {
    let mut counts: HashMap<(u8, u8, u8), (u32, u32)> = HashMap::new();
    let mut seq = 0u32;
    let mut px = |ix: i32, iy: i32| {
        if ix >= 0 && ix < iw && iy >= 0 && iy < ih {
            let o = iy as usize * stride + ix as usize * 4;
            let e = counts.entry((buf[o], buf[o + 1], buf[o + 2])).or_insert_with(|| {
                seq += 1;
                (0, seq)
            });
            e.0 += 1;
        }
    };
    let step = (((w + h) / 200).max(1)) as usize;
    for ix in (x..x + w).step_by(step) {
        px(ix, y - 1);
        px(ix, y + h);
    }
    for iy in (y..y + h).step_by(step) {
        px(x - 1, iy);
        px(x + w, iy);
    }
    match counts.into_iter().max_by_key(|(_, (n, first))| (*n, std::cmp::Reverse(*first))) {
        Some(((b, g, r), _)) => (r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0),
        None => (1.0, 1.0, 1.0),
    }
}

/// A whole-image rotation or flip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Xform {
    RotLeft,
    RotRight,
    FlipH,
    FlipV,
}

impl Xform {
    pub fn inverse(self) -> Xform {
        match self {
            Xform::RotLeft => Xform::RotRight,
            Xform::RotRight => Xform::RotLeft,
            other => other,
        }
    }
}

/// Applies `t` to a strided buffer. Returns the tight result and its width and height.
pub fn transform(buf: &[u8], stride: usize, w: usize, h: usize, t: Xform) -> (Vec<u8>, usize, usize) {
    let (nw, nh) = match t {
        Xform::RotLeft | Xform::RotRight => (h, w),
        _ => (w, h),
    };
    let mut out = vec![0u8; nw * nh * 4];
    for y in 0..h {
        for x in 0..w {
            let (nx, ny) = match t {
                Xform::RotRight => (h - 1 - y, x),
                Xform::RotLeft => (y, w - 1 - x),
                Xform::FlipH => (w - 1 - x, y),
                Xform::FlipV => (x, h - 1 - y),
            };
            let s = y * stride + x * 4;
            let d = (ny * nw + nx) * 4;
            out[d..d + 4].copy_from_slice(&buf[s..s + 4]);
        }
    }
    (out, nw, nh)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn img(w: usize, h: usize) -> Vec<u8> {
        (0..w * h).flat_map(|i| [i as u8, 0, 0, 255]).collect()
    }

    #[test]
    fn region_round_trip() {
        let mut buf = img(4, 3);
        let r = read_region(&buf, 16, 1, 1, 2, 2);
        assert_eq!(r.len(), 16);
        assert_eq!(r[0], 5);
        let zeros = vec![0u8; 16];
        write_region(&mut buf, 16, 1, 1, 2, 2, &zeros);
        assert_eq!(buf[20], 0);
        write_region(&mut buf, 16, 1, 1, 2, 2, &r);
        assert_eq!(buf, img(4, 3));
    }

    #[test]
    fn rotations_invert_each_other() {
        let a = img(5, 3);
        for t in [Xform::RotLeft, Xform::RotRight, Xform::FlipH, Xform::FlipV] {
            let (b, w, h) = transform(&a, 20, 5, 3, t);
            let (c, w2, h2) = transform(&b, w * 4, w, h, t.inverse());
            assert_eq!((w2, h2), (5, 3));
            assert_eq!(c, a, "{t:?}");
        }
    }

    #[test]
    fn rotate_right_moves_top_left_to_top_right() {
        let a = img(3, 2);
        let (b, w, h) = transform(&a, 12, 3, 2, Xform::RotRight);
        assert_eq!((w, h), (2, 3));
        assert_eq!(b[4], 0);
    }

    #[test]
    fn pick_unpremultiplies() {
        let buf = vec![50u8, 50, 50, 100];
        let (r, _, _) = pick_pixel(&buf, 4, 1, 1, 0.0, 0.0);
        assert!((r - 127.0 / 255.0).abs() < 0.01);
        let (r2, _, _) = pick_pixel(&buf, 4, 1, 1, 9.0, 9.0);
        assert_eq!(r, r2);
    }

    #[test]
    fn ring_color_finds_background() {
        // 10x10 white image with a black 4x4 square at (3,3).
        let mut buf = vec![255u8; 10 * 10 * 4];
        for y in 3..7 {
            for x in 3..7 {
                let o = (y * 10 + x) * 4;
                buf[o..o + 3].fill(0);
            }
        }
        assert_eq!(ring_color(&buf, 40, 10, 10, 3, 3, 4, 4), (1.0, 1.0, 1.0));
        assert_eq!(ring_color(&buf, 40, 10, 10, 4, 4, 2, 2), (0.0, 0.0, 0.0));
    }

    #[test]
    fn ring_outside_image_is_white() {
        let buf = vec![0u8; 4 * 4 * 4];
        assert_eq!(ring_color(&buf, 16, 4, 4, 0, 0, 4, 4), (1.0, 1.0, 1.0));
    }
}
