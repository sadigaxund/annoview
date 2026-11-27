//! Settings stored in `settings.json`, keeping the keys of the Python version.

use super::palette::{build_palette, hexcol, parse_hex};
use serde_json::{Map, Value};

pub const SHAPES: [&str; 4] = ["line", "rect", "ellipse", "tri"];

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub pen_rgb: String,
    pub hl_rgb: String,
    pub pen_size: i64,
    pub hl_size: i64,
    pub hl_alpha: i64,
    pub shape_size: i64,
    pub shape: String,
    pub fill: bool,
    pub font_family: String,
    pub font_size: i64,
    pub bold: bool,
}

impl Default for Settings {
    fn default() -> Self {
        let p = build_palette();
        Settings {
            pen_rgb: hexcol(p[22]),
            hl_rgb: hexcol(p[67]),
            pen_size: 5,
            hl_size: 24,
            hl_alpha: 80,
            shape_size: 4,
            shape: "rect".into(),
            fill: false,
            font_family: "Sans".into(),
            font_size: 28,
            bold: false,
        }
    }
}

fn int(v: &Value, lo: i64, hi: i64) -> Option<i64> {
    v.as_f64().map(|f| (f.round() as i64).clamp(lo, hi))
}

impl Settings {
    /// Reads a settings document. Each bad or missing key keeps its default.
    pub fn from_json(text: &str) -> Settings {
        let mut s = Settings::default();
        let Ok(Value::Object(m)) = serde_json::from_str::<Value>(text) else {
            return s;
        };
        let color = |k: &str, dst: &mut String| {
            if let Some(Value::String(c)) = m.get(k) {
                if parse_hex(c).is_some() {
                    *dst = c.clone();
                }
            }
        };
        color("pen_rgb", &mut s.pen_rgb);
        color("hl_rgb", &mut s.hl_rgb);
        let num = |k: &str, dst: &mut i64, lo: i64, hi: i64| {
            if let Some(n) = m.get(k).and_then(|v| int(v, lo, hi)) {
                *dst = n;
            }
        };
        num("pen_size", &mut s.pen_size, 1, 40);
        num("hl_size", &mut s.hl_size, 4, 80);
        num("hl_alpha", &mut s.hl_alpha, 10, 100);
        num("shape_size", &mut s.shape_size, 1, 40);
        num("font_size", &mut s.font_size, 8, 400);
        if let Some(Value::String(sh)) = m.get("shape") {
            if SHAPES.contains(&sh.as_str()) {
                s.shape = sh.clone();
            }
        }
        if let Some(Value::Bool(b)) = m.get("fill") {
            s.fill = *b;
        }
        if let Some(Value::Bool(b)) = m.get("bold") {
            s.bold = *b;
        }
        if let Some(Value::String(f)) = m.get("font_family") {
            if !f.is_empty() {
                s.font_family = f.clone();
            }
        }
        s
    }

    pub fn to_json(&self) -> String {
        let mut m = Map::new();
        m.insert("pen_rgb".into(), self.pen_rgb.clone().into());
        m.insert("hl_rgb".into(), self.hl_rgb.clone().into());
        m.insert("pen_size".into(), self.pen_size.into());
        m.insert("hl_size".into(), self.hl_size.into());
        m.insert("hl_alpha".into(), self.hl_alpha.into());
        m.insert("shape_size".into(), self.shape_size.into());
        m.insert("shape".into(), self.shape.clone().into());
        m.insert("fill".into(), self.fill.into());
        m.insert("font_family".into(), self.font_family.clone().into());
        m.insert("font_size".into(), self.font_size.into());
        m.insert("bold".into(), self.bold.into());
        let mut out = Vec::new();
        let fmt = serde_json::ser::PrettyFormatter::with_indent(b" ");
        let mut ser = serde_json::Serializer::with_formatter(&mut out, fmt);
        serde::Serialize::serialize(&Value::Object(m), &mut ser).expect("serialize settings");
        String::from_utf8(out).expect("utf8")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWNER: &str = r##"{
 "pen_rgb": "#293d17",
 "hl_rgb": "#dfff00",
 "pen_size": 29,
 "hl_size": 24,
 "hl_alpha": 80,
 "shape_size": 18,
 "shape": "rect",
 "fill": false,
 "font_family": "Sans",
 "font_size": 66,
 "bold": false
}"##;

    #[test]
    fn loads_legacy_file() {
        let s = Settings::from_json(OWNER);
        assert_eq!(s.pen_rgb, "#293d17");
        assert_eq!(s.pen_size, 29);
        assert_eq!(s.shape_size, 18);
        assert_eq!(s.font_size, 66);
    }

    #[test]
    fn round_trip_is_identical_to_legacy_layout() {
        let s = Settings::from_json(OWNER);
        assert_eq!(s.to_json(), OWNER);
        assert_eq!(Settings::from_json(&s.to_json()), s);
    }

    #[test]
    fn defaults_match_legacy() {
        let d = Settings::default();
        assert_eq!((d.pen_rgb.as_str(), d.hl_rgb.as_str()), ("#ff0000", "#dfff00"));
        assert_eq!((d.pen_size, d.hl_size, d.hl_alpha, d.shape_size), (5, 24, 80, 4));
        assert_eq!((d.shape.as_str(), d.font_size, d.fill, d.bold), ("rect", 28, false, false));
    }

    #[test]
    fn bad_values_fall_back() {
        let s = Settings::from_json(
            r##"{"pen_rgb":"red","shape":"star","pen_size":"x","unknown":1,"hl_alpha":500,"fill":1}"##,
        );
        let d = Settings::default();
        assert_eq!(s.pen_rgb, d.pen_rgb);
        assert_eq!(s.shape, "rect");
        assert_eq!(s.pen_size, d.pen_size);
        assert_eq!(s.hl_alpha, 100);
        assert!(!s.fill);
    }

    #[test]
    fn broken_file_gives_defaults() {
        assert_eq!(Settings::from_json("{not json"), Settings::default());
        assert_eq!(Settings::from_json("[]"), Settings::default());
        assert_eq!(Settings::from_json(""), Settings::default());
    }
}
