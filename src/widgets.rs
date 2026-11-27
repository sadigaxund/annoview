//! Small widgets: the square icon button and the 256-color column.

use crate::logic::palette::{self, Rgb};
use crate::paint::draw_icon;
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

struct IconState {
    icon: String,
    dropdown: bool,
    rgb: Rgb,
}

/// Square flat button with a cairo-drawn icon or a color swatch.
#[derive(Clone)]
pub struct IconButton {
    pub btn: gtk::Button,
    da: gtk::DrawingArea,
    st: Rc<RefCell<IconState>>,
}

impl IconButton {
    pub const SIZE: i32 = 38;

    pub fn new(icon: &str, tip: &str) -> IconButton {
        let btn = gtk::Button::new();
        btn.add_css_class("flat");
        btn.set_size_request(Self::SIZE, Self::SIZE);
        btn.set_valign(gtk::Align::Center);
        btn.set_halign(gtk::Align::Center);
        btn.set_tooltip_text(Some(tip));
        let da = gtk::DrawingArea::new();
        da.set_content_width(20);
        da.set_content_height(20);
        let st = Rc::new(RefCell::new(IconState { icon: icon.into(), dropdown: false, rgb: (1.0, 0.0, 0.0) }));
        let s2 = st.clone();
        da.set_draw_func(move |a, cr, w, h| {
            let fg = a.color();
            let s = s2.borrow();
            cr.translate((w as f64 - 20.0) / 2.0, (h as f64 - 20.0) / 2.0);
            if s.icon == "swatch" {
                cr.set_source_rgb(s.rgb.0, s.rgb.1, s.rgb.2);
                cr.rectangle(2.0, 2.0, 16.0, 16.0);
                let _ = cr.fill_preserve();
                cr.set_source_rgba(fg.red() as f64, fg.green() as f64, fg.blue() as f64, 0.6);
                cr.set_line_width(1.2);
                let _ = cr.stroke();
                return;
            }
            let c = (fg.red() as f64, fg.green() as f64, fg.blue() as f64, fg.alpha() as f64);
            draw_icon(cr, &s.icon, c);
            if s.dropdown {
                cr.set_source_rgba(c.0, c.1, c.2, c.3);
                cr.move_to(14.5, 15.5);
                cr.line_to(19.5, 15.5);
                cr.line_to(17.0, 18.5);
                cr.close_path();
                let _ = cr.fill();
            }
        });
        btn.set_child(Some(&da));
        IconButton { btn, da, st }
    }

    pub fn set_dropdown(&self, on: bool) {
        self.st.borrow_mut().dropdown = on;
        self.da.queue_draw();
    }

    pub fn set_icon(&self, name: &str) {
        self.st.borrow_mut().icon = name.into();
        self.da.queue_draw();
    }

    pub fn set_rgb(&self, rgb: Rgb) {
        self.st.borrow_mut().rgb = rgb;
        self.da.queue_draw();
    }
}

/// A 20x20 icon as a plain widget, for menus.
pub fn icon_widget(name: &'static str) -> gtk::DrawingArea {
    let da = gtk::DrawingArea::new();
    da.set_content_width(20);
    da.set_content_height(20);
    da.set_draw_func(move |a, cr, w, h| {
        let fg = a.color();
        cr.translate((w as f64 - 20.0) / 2.0, (h as f64 - 20.0) / 2.0);
        draw_icon(cr, name, (fg.red() as f64, fg.green() as f64, fg.blue() as f64, fg.alpha() as f64));
    });
    da
}

pub const PAL_W: i32 = 28;
pub const PAL_H: i32 = 18;

/// One vertical column of all 256 colors, scrolled by its container.
#[derive(Clone)]
pub struct Palette {
    pub da: gtk::DrawingArea,
    colors: Rc<Vec<Rgb>>,
    idx: Rc<Cell<i32>>,
}

impl Palette {
    pub fn new(on_pick: impl Fn(Rgb) + 'static) -> Palette {
        let colors = Rc::new(palette::build_palette());
        let idx = Rc::new(Cell::new(-1));
        let da = gtk::DrawingArea::new();
        da.set_content_width(PAL_W);
        da.set_content_height(colors.len() as i32 * PAL_H);
        da.set_halign(gtk::Align::Center);
        let (c2, i2) = (colors.clone(), idx.clone());
        da.set_draw_func(move |_, cr, _, _| {
            for (i, (r, g, b)) in c2.iter().enumerate() {
                cr.set_source_rgb(*r, *g, *b);
                cr.rectangle(0.0, (i as i32 * PAL_H) as f64, PAL_W as f64, (PAL_H - 1) as f64);
                let _ = cr.fill();
            }
            let sel = i2.get();
            if sel >= 0 {
                let y = (sel * PAL_H) as f64;
                let w = PAL_W as f64;
                cr.set_line_width(2.0);
                cr.set_source_rgb(0.0, 0.0, 0.0);
                cr.rectangle(1.0, y + 1.0, w - 2.0, PAL_H as f64 - 3.0);
                let _ = cr.stroke();
                cr.set_line_width(1.0);
                cr.set_source_rgb(1.0, 1.0, 1.0);
                cr.rectangle(2.0, y + 2.0, w - 4.0, PAL_H as f64 - 5.0);
                let _ = cr.stroke();
            }
        });
        let g = gtk::GestureClick::new();
        let c3 = colors.clone();
        g.connect_pressed(move |_, _, _, y| {
            let i = (y / PAL_H as f64).floor() as i64;
            if i >= 0 && (i as usize) < c3.len() {
                on_pick(c3[i as usize]);
            }
        });
        da.add_controller(g);
        Palette { da, colors, idx }
    }

    pub fn set_color(&self, rgb: Rgb) {
        self.idx.set(palette::find_index(&self.colors, rgb).map_or(-1, |i| i as i32));
        self.da.queue_draw();
    }

    pub fn index(&self) -> i32 {
        self.idx.get()
    }
}
