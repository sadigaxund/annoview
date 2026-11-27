//! The 256-color palette and hex color helpers.

pub type Rgb = (f64, f64, f64);

fn hsv_to_rgb(h: f64, s: f64, v: f64) -> Rgb {
    if s == 0.0 {
        return (v, v, v);
    }
    let i = (h * 6.0).floor();
    let f = h * 6.0 - i;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    match (i as i64).rem_euclid(6) {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    }
}

/// 16 grays, then 16 hue families of 15 shades each.
pub fn build_palette() -> Vec<Rgb> {
    let mut cols: Vec<Rgb> = (0..16)
        .map(|i| {
            let g = i as f64 / 15.0;
            (g, g, g)
        })
        .collect();
    for c in 0..16 {
        for r in 0..15 {
            let (s, v) = if r < 7 {
                (0.2 + 0.8 * r as f64 / 6.0, 1.0)
            } else {
                (1.0, 1.0 - 0.8 * (r - 6) as f64 / 8.0)
            };
            cols.push(hsv_to_rgb(c as f64 / 16.0, s, v));
        }
    }
    cols
}

pub fn parse_hex(h: &str) -> Option<Rgb> {
    let b = h.as_bytes();
    if b.len() != 7 || b[0] != b'#' || !h.is_ascii() {
        return None;
    }
    let c = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok().map(|v| v as f64 / 255.0);
    Some((c(1)?, c(3)?, c(5)?))
}

pub fn hexcol(rgb: Rgb) -> String {
    let c = |v: f64| (v * 255.0).round_ties_even() as i64;
    format!("#{:02x}{:02x}{:02x}", c(rgb.0), c(rgb.1), c(rgb.2))
}

/// Index of the palette entry within 0.003 of `rgb` on every channel.
pub fn find_index(palette: &[Rgb], rgb: Rgb) -> Option<usize> {
    palette.iter().position(|c| {
        (c.0 - rgb.0).abs() < 0.003 && (c.1 - rgb.1).abs() < 0.003 && (c.2 - rgb.2).abs() < 0.003
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Generated from legacy build_palette() in legacy/shotnote.py.
    const LEGACY: &str = include_str!("../../tests/palette_legacy.txt");

    #[test]
    fn matches_legacy_palette() {
        let p = build_palette();
        assert_eq!(p.len(), 256);
        let want: Vec<&str> = LEGACY.lines().collect();
        assert_eq!(want.len(), 256);
        for (i, c) in p.iter().enumerate() {
            assert_eq!(hexcol(*c), want[i], "color {i}");
        }
    }

    #[test]
    fn defaults_come_from_palette_indices() {
        let p = build_palette();
        assert_eq!(hexcol(p[22]), "#ff0000");
        assert_eq!(hexcol(p[67]), "#dfff00");
    }

    #[test]
    fn hex_round_trip() {
        assert_eq!(hexcol(parse_hex("#293d17").unwrap()), "#293d17");
        assert!(parse_hex("293d17").is_none());
        assert!(parse_hex("#zz3d17").is_none());
        assert!(parse_hex("#12345").is_none());
    }

    #[test]
    fn finds_palette_index() {
        let p = build_palette();
        assert_eq!(find_index(&p, p[100]), Some(100));
        assert_eq!(find_index(&p, (0.123, 0.456, 0.789)), None);
    }
}
