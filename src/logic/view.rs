//! Zoom and pan math in widget and image coordinates.

pub const MIN_SCALE: f64 = 0.05;
pub const MAX_SCALE: f64 = 64.0;
pub const PAN_MARGIN: f64 = 60.0;

/// Widget point to image point.
pub fn to_img(ox: f64, oy: f64, scale: f64, wx: f64, wy: f64) -> (f64, f64) {
    ((wx - ox) / scale, (wy - oy) / scale)
}

pub fn clamp_scale(s: f64) -> f64 {
    s.clamp(MIN_SCALE, MAX_SCALE)
}

/// Scale that shows the whole image, never above actual size (`1 / dev`).
pub fn fit_scale(dev: f64, vw: f64, vh: f64, iw: f64, ih: f64) -> f64 {
    (1.0 / dev).min(vw.max(1.0) / iw).min(vh.max(1.0) / ih)
}

fn axis(view: f64, img: f64, off: f64) -> f64 {
    if img <= view {
        (view - img) / 2.0
    } else {
        off.max(view - img - PAN_MARGIN).min(PAN_MARGIN)
    }
}

/// Keeps the image centered when small and within the margin when large.
pub fn constrain(vw: f64, vh: f64, iw: f64, ih: f64, scale: f64, ox: f64, oy: f64) -> (f64, f64) {
    (axis(vw, iw * scale, ox), axis(vh, ih * scale, oy))
}

/// Offset that keeps image point (ix, iy) under widget point (wx, wy) at scale `s`.
pub fn offset_for_anchor(wx: f64, wy: f64, ix: f64, iy: f64, s: f64) -> (f64, f64) {
    (wx - ix * s, wy - iy * s)
}

/// One animation step toward `target`. Returns the new scale and whether it arrived.
pub fn smooth_step(scale: f64, target: f64, dt: f64) -> (f64, bool) {
    let s = scale + (target - scale) * (1.0 - (-dt * 22.0).exp());
    if (s - target).abs() < target * 1e-3 {
        (target, true)
    } else {
        (s, false)
    }
}

/// Frame time step: capped at 50 ms, 1/60 on the first frame.
pub fn frame_dt(now: f64, last: f64) -> f64 {
    if last > 0.0 {
        (now - last).min(0.05)
    } else {
        1.0 / 60.0
    }
}

/// Initial window size in logical pixels: image plus chrome, capped to 96% by 92% of the monitor.
/// Returns (width, height, capped).
pub fn window_size(
    iw: i32, ih: i32, dev: f64, side: i32, chrome_h: i32, min_w: i32, mon_w: i32, mon_h: i32,
) -> (i32, i32, bool) {
    let lw = (iw as f64 / dev).ceil() as i32;
    let lh = (ih as f64 / dev).ceil() as i32;
    let w = (side + lw).max(min_w);
    let h = chrome_h + lh;
    let (cw, ch) = ((mon_w as f64 * 0.96) as i32, (mon_h as f64 * 0.92) as i32);
    (w.min(cw), h.min(ch), w > cw || h > ch)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_img_inverts_the_view() {
        assert_eq!(to_img(10.0, 20.0, 2.0, 30.0, 60.0), (10.0, 20.0));
    }

    #[test]
    fn fit_never_upscales_past_actual() {
        assert_eq!(fit_scale(1.0, 4000.0, 4000.0, 100.0, 100.0), 1.0);
        assert_eq!(fit_scale(2.0, 4000.0, 4000.0, 100.0, 100.0), 0.5);
        assert_eq!(fit_scale(1.0, 500.0, 1000.0, 1000.0, 1000.0), 0.5);
        assert_eq!(fit_scale(1.0, 0.0, 0.0, 10.0, 10.0), 0.1);
    }

    #[test]
    fn small_image_is_centered_large_is_bounded() {
        assert_eq!(constrain(100.0, 100.0, 50.0, 50.0, 1.0, 999.0, -999.0), (25.0, 25.0));
        assert_eq!(constrain(100.0, 100.0, 300.0, 300.0, 1.0, 999.0, -999.0), (60.0, -260.0));
        assert_eq!(constrain(100.0, 100.0, 300.0, 300.0, 1.0, -50.0, -50.0), (-50.0, -50.0));
    }

    #[test]
    fn zoom_around_a_point_keeps_it_fixed() {
        let (ox, oy, s) = (30.0, 40.0, 1.0);
        let (wx, wy) = (200.0, 150.0);
        let (ix, iy) = to_img(ox, oy, s, wx, wy);
        for s2 in [0.5, 2.0, 7.3] {
            let (nx, ny) = offset_for_anchor(wx, wy, ix, iy, s2);
            let (jx, jy) = to_img(nx, ny, s2, wx, wy);
            assert!((jx - ix).abs() < 1e-9 && (jy - iy).abs() < 1e-9);
        }
    }

    #[test]
    fn scale_is_clamped() {
        assert_eq!(clamp_scale(0.0), MIN_SCALE);
        assert_eq!(clamp_scale(1000.0), MAX_SCALE);
        assert_eq!(clamp_scale(2.0), 2.0);
    }

    #[test]
    fn animation_converges_monotonically() {
        let (mut s, target) = (1.0, 4.0);
        let mut last = s;
        for i in 0..400 {
            let (n, done) = smooth_step(s, target, 1.0 / 60.0);
            assert!(n >= last);
            last = n;
            s = n;
            if done {
                assert_eq!(s, target);
                assert!(i < 120);
                return;
            }
        }
        panic!("did not converge");
    }

    #[test]
    fn frame_step_is_capped() {
        assert_eq!(frame_dt(5.0, 0.0), 1.0 / 60.0);
        assert_eq!(frame_dt(5.0, 1.0), 0.05);
        assert!((frame_dt(1.01, 1.0) - 0.01).abs() < 1e-9);
    }

    #[test]
    fn window_fits_image_or_is_capped() {
        // 1000x500 image at scale 2: 500x250 plus 29 side and 90 chrome.
        assert_eq!(window_size(1000, 500, 2.0, 39, 90, 300, 1920, 1080), (539, 340, false));
        let (w, h, capped) = window_size(5000, 4000, 1.0, 39, 90, 300, 1920, 1080);
        assert_eq!((w, h, capped), (1843, 993, true));
        // Narrow image: the toolbar sets the minimum width.
        assert_eq!(window_size(100, 100, 1.0, 39, 90, 400, 1920, 1080).0, 400);
    }
}
