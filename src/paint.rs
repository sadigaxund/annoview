//! Cairo painting of strokes, shapes, text and tool icons.

use gtk::cairo::{self, Context, LineCap, LineJoin, Operator};
use gtk::pango;
use std::f64::consts::PI;

pub type Rgb = (f64, f64, f64);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    Pen,
    Hl,
    Text,
    Select,
    Line,
    Rect,
    Ellipse,
    Tri,
    Pick,
}

impl Tool {
    pub fn from_shape(name: &str) -> Tool {
        match name {
            "line" => Tool::Line,
            "ellipse" => Tool::Ellipse,
            "tri" => Tool::Tri,
            _ => Tool::Rect,
        }
    }

    pub fn shape_name(self) -> Option<&'static str> {
        match self {
            Tool::Line => Some("line"),
            Tool::Rect => Some("rect"),
            Tool::Ellipse => Some("ellipse"),
            Tool::Tri => Some("tri"),
            _ => None,
        }
    }

    pub fn is_shape(self) -> bool {
        self.shape_name().is_some()
    }
}

/// A stroke or shape being drawn, or about to be committed.
#[derive(Clone, Debug)]
pub struct Item {
    pub kind: Tool,
    pub color: Rgb,
    pub fill: bool,
    pub width: f64,
    pub alpha: f64,
    pub pts: Vec<(f64, f64)>,
    pub p0: (f64, f64),
    pub p1: (f64, f64),
}

impl Item {
    pub fn new(kind: Tool, color: Rgb, fill: bool, width: f64, alpha: f64, p: (f64, f64)) -> Item {
        Item { kind, color, fill, width, alpha, pts: vec![p], p0: p, p1: p }
    }

    pub fn is_freehand(&self) -> bool {
        matches!(self.kind, Tool::Pen | Tool::Hl)
    }

    pub fn bbox(&self) -> (f64, f64, f64, f64) {
        let pad = self.width * 1.5 + 2.0;
        let pts: Vec<(f64, f64)> = if self.is_freehand() { self.pts.clone() } else { vec![self.p0, self.p1] };
        let xs = pts.iter().map(|p| p.0);
        let ys = pts.iter().map(|p| p.1);
        (
            xs.clone().fold(f64::MAX, f64::min) - pad,
            ys.clone().fold(f64::MAX, f64::min) - pad,
            xs.fold(f64::MIN, f64::max) + pad,
            ys.fold(f64::MIN, f64::max) + pad,
        )
    }
}

/// Paints `it`. `multiply` blends highlighter strokes with the multiply operator onto the target.
/// With `multiply` false a highlighter stroke is composited normally, for use on its own layer.
pub fn paint_item(cr: &Context, it: &Item, multiply: bool) {
    let (r, g, b) = it.color;
    cr.set_line_cap(LineCap::Round);
    cr.set_line_join(LineJoin::Round);
    cr.set_line_width(it.width);
    if it.is_freehand() {
        let pts = &it.pts;
        let hl = it.kind == Tool::Hl;
        if hl {
            cr.push_group();
        }
        cr.set_source_rgb(r, g, b);
        if pts.len() == 1 {
            cr.arc(pts[0].0, pts[0].1, it.width / 2.0, 0.0, 2.0 * PI);
            let _ = cr.fill();
        } else {
            cr.move_to(pts[0].0, pts[0].1);
            let mut p0 = pts[0];
            for i in 1..pts.len() - 1 {
                let (q, nx) = (pts[i], pts[i + 1]);
                let m = ((q.0 + nx.0) / 2.0, (q.1 + nx.1) / 2.0);
                cr.curve_to(
                    p0.0 + 2.0 / 3.0 * (q.0 - p0.0),
                    p0.1 + 2.0 / 3.0 * (q.1 - p0.1),
                    m.0 + 2.0 / 3.0 * (q.0 - m.0),
                    m.1 + 2.0 / 3.0 * (q.1 - m.1),
                    m.0,
                    m.1,
                );
                p0 = m;
            }
            let last = pts[pts.len() - 1];
            cr.line_to(last.0, last.1);
            let _ = cr.stroke();
        }
        if hl {
            let _ = cr.pop_group_to_source();
            if multiply {
                cr.set_operator(Operator::Multiply);
            }
            let _ = cr.paint_with_alpha(it.alpha);
            cr.set_operator(Operator::Over);
        }
        return;
    }
    let ((x0, y0), (x1, y1)) = (it.p0, it.p1);
    cr.set_source_rgb(r, g, b);
    match it.kind {
        Tool::Line => {
            cr.move_to(x0, y0);
            cr.line_to(x1, y1);
            let _ = cr.stroke();
            return;
        }
        Tool::Rect => cr.rectangle(x0.min(x1), y0.min(y1), (x1 - x0).abs(), (y1 - y0).abs()),
        Tool::Ellipse => {
            let _ = cr.save();
            cr.translate((x0 + x1) / 2.0, (y0 + y1) / 2.0);
            cr.scale(((x1 - x0).abs() / 2.0).max(0.01), ((y1 - y0).abs() / 2.0).max(0.01));
            cr.arc(0.0, 0.0, 1.0, 0.0, 2.0 * PI);
            let _ = cr.restore();
        }
        Tool::Tri => {
            cr.move_to((x0 + x1) / 2.0, y0);
            cr.line_to(x1, y1);
            cr.line_to(x0, y1);
            cr.close_path();
        }
        _ => return,
    }
    if it.fill {
        let _ = cr.fill_preserve();
    }
    let _ = cr.stroke();
}

pub fn make_layout(cr: &Context, text: &str, family: &str, size: f64, bold: bool) -> pango::Layout {
    let layout = pangocairo::functions::create_layout(cr);
    let mut fd = pango::FontDescription::new();
    fd.set_family(family);
    fd.set_weight(if bold { pango::Weight::Bold } else { pango::Weight::Normal });
    fd.set_absolute_size(size * pango::SCALE as f64);
    layout.set_font_description(Some(&fd));
    layout.set_text(text);
    layout
}

/// Pixel size of `text` as it will be committed, with a minimum of one empty line.
pub fn text_extent(text: &str, family: &str, size: i32, bold: bool) -> (i32, i32) {
    let surf = cairo::ImageSurface::create(cairo::Format::ARgb32, 1, 1).expect("surface");
    let cr = Context::new(&surf).expect("context");
    let (w, h) = make_layout(&cr, text, family, size as f64, bold).pixel_size();
    (w.max((size as f64 * 0.6) as i32), h.max((size as f64 * 1.2) as i32))
}

/// Draws a 20x20 tool icon in the given foreground color.
pub fn draw_icon(cr: &Context, name: &str, fg: (f64, f64, f64, f64)) {
    let (r, g, b, a) = fg;
    cr.set_source_rgba(r, g, b, a);
    cr.set_line_width(1.8);
    cr.set_line_cap(LineCap::Round);
    cr.set_line_join(LineJoin::Round);
    let stroke = || {
        let _ = cr.stroke();
    };
    match name {
        "pen" => {
            cr.move_to(3.0, 17.0);
            cr.line_to(4.5, 12.5);
            cr.line_to(13.5, 3.5);
            cr.line_to(16.5, 6.5);
            cr.line_to(7.5, 15.5);
            cr.close_path();
            stroke();
            cr.move_to(4.5, 12.5);
            cr.line_to(7.5, 15.5);
            stroke();
        }
        "hl" => {
            cr.move_to(6.0, 11.0);
            cr.line_to(13.0, 4.0);
            cr.line_to(16.0, 7.0);
            cr.line_to(9.0, 14.0);
            cr.close_path();
            stroke();
            cr.set_line_width(3.2);
            cr.set_source_rgba(r, g, b, a * 0.55);
            cr.move_to(3.5, 17.5);
            cr.line_to(12.0, 17.5);
            stroke();
        }
        "text" => {
            cr.set_line_width(2.2);
            cr.move_to(4.0, 4.5);
            cr.line_to(16.0, 4.5);
            stroke();
            cr.move_to(10.0, 4.5);
            cr.line_to(10.0, 16.5);
            stroke();
        }
        "select" => {
            cr.set_dash(&[2.6, 2.6], 0.0);
            cr.rectangle(3.5, 3.5, 13.0, 13.0);
            stroke();
        }
        "line" => {
            cr.move_to(4.0, 16.0);
            cr.line_to(16.0, 4.0);
            stroke();
        }
        "rect" => {
            cr.rectangle(3.5, 5.0, 13.0, 10.0);
            stroke();
        }
        "ellipse" => {
            let _ = cr.save();
            cr.translate(10.0, 10.0);
            cr.scale(7.0, 5.5);
            cr.arc(0.0, 0.0, 1.0, 0.0, 2.0 * PI);
            let _ = cr.restore();
            stroke();
        }
        "tri" => {
            cr.move_to(10.0, 3.5);
            cr.line_to(16.5, 16.0);
            cr.line_to(3.5, 16.0);
            cr.close_path();
            stroke();
        }
        "pick" => {
            cr.move_to(3.5, 16.5);
            cr.line_to(9.0, 11.0);
            stroke();
            cr.set_line_width(4.5);
            cr.set_line_cap(LineCap::Butt);
            cr.move_to(9.0, 11.0);
            cr.line_to(14.0, 6.0);
            stroke();
            cr.set_line_width(2.2);
            cr.set_line_cap(LineCap::Round);
            cr.move_to(12.0, 3.0);
            cr.line_to(17.0, 8.0);
            stroke();
        }
        _ => {}
    }
}
