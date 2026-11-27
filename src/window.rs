//! The editor window: header, toolbar, color column, popovers, keys and file operations.

use crate::app::{surface_from_file, Ctx};
use crate::canvas::{Callbacks, Canvas, Params};
use crate::capture;
use crate::logic::folder;
use crate::logic::naming;
use crate::logic::palette::{hexcol, parse_hex, Rgb};
use crate::logic::pixels::Xform;
use crate::logic::settings::Settings;
use crate::logic::view;
use crate::paint::Tool;
use crate::widgets::{icon_widget, IconButton, Palette, PAL_H, PAL_W};
use adw::prelude::*;
use gtk::cairo::ImageSurface;
use gtk::{gdk, gio, glib, pango};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

pub struct Win {
    ctx: Rc<Ctx>,
    pub window: adw::ApplicationWindow,
    pub(crate) canvas: Canvas,
    header: adw::HeaderBar,
    toolbar: gtk::Box,
    wtitle: adw::WindowTitle,
    file_btn: gtk::MenuButton,
    undo_btn: gtk::Button,
    redo_btn: gtk::Button,
    zoom_btn: gtk::Button,
    palette: Palette,
    pal_sw: gtk::ScrolledWindow,
    tool_buttons: HashMap<&'static str, IconButton>,
    color_btn: IconButton,
    popovers: RefCell<HashMap<&'static str, gtk::Popover>>,
    toast_label: gtk::Label,
    toast_id: RefCell<Option<glib::SourceId>>,
    path: RefCell<Option<PathBuf>>,
    owned: Cell<bool>,
    prev_tool: Cell<Tool>,
    corrected: Cell<bool>,
    centered: Cell<bool>,
    capped: Cell<bool>,
    size: Cell<(i32, i32)>,
    dev: f64,
    mon: (i32, i32),
}

fn tool_key(t: Tool) -> &'static str {
    match t {
        Tool::Pen => "pen",
        Tool::Hl => "hl",
        Tool::Text => "text",
        Tool::Select => "select",
        Tool::Pick => "pick",
        _ => "shape",
    }
}

fn monitor_info() -> (f64, i32, i32) {
    let mon = gdk::Display::default()
        .and_then(|d| d.monitors().item(0))
        .and_then(|o| o.downcast::<gdk::Monitor>().ok());
    match mon {
        Some(m) => {
            let g = m.geometry();
            (m.scale(), g.width(), g.height())
        }
        None => (1.0, 1920, 1080),
    }
}

fn key_of(k: gdk::Key) -> String {
    k.name().map(|n| n.to_lowercase()).unwrap_or_default()
}

impl Win {
    pub fn new(ctx: &Rc<Ctx>, surface: ImageSurface, path: Option<PathBuf>) -> Rc<Win> {
        let (dev, mw, mh) = monitor_info();
        let window = adw::ApplicationWindow::new(&ctx.app);

        let header = adw::HeaderBar::new();
        let wtitle = adw::WindowTitle::new("", "");
        header.set_title_widget(Some(&wtitle));
        let file_btn = gtk::MenuButton::builder().label("File").build();
        header.pack_start(&file_btn);
        let undo_btn = gtk::Button::from_icon_name("edit-undo-symbolic");
        let redo_btn = gtk::Button::from_icon_name("edit-redo-symbolic");
        undo_btn.set_tooltip_text(Some("Undo (Ctrl+Z)"));
        redo_btn.set_tooltip_text(Some("Redo (Ctrl+Y)"));
        header.pack_start(&undo_btn);
        header.pack_start(&redo_btn);
        let zoom_btn = gtk::Button::with_label("100%");
        zoom_btn.add_css_class("flat");
        zoom_btn.set_tooltip_text(Some("Click: actual size  |  Ctrl+0: fit"));
        header.pack_end(&zoom_btn);

        let overlay = gtk::Overlay::new();
        let canvas = Canvas::new(dev, surface);
        canvas.set_overlay(&overlay);
        overlay.set_child(Some(&canvas));
        overlay.set_hexpand(true);
        let toast_label = gtk::Label::new(None);
        toast_label.add_css_class("shotnote-toast");
        toast_label.set_halign(gtk::Align::Center);
        toast_label.set_valign(gtk::Align::End);
        toast_label.set_margin_bottom(18);
        toast_label.set_can_target(false);
        toast_label.set_visible(false);
        overlay.add_overlay(&toast_label);

        let palette_ref: Rc<RefCell<Option<Rc<Win>>>> = Rc::new(RefCell::new(None));
        let pr = palette_ref.clone();
        let palette = Palette::new(move |rgb| {
            if let Some(w) = pr.borrow().as_ref() {
                w.set_color(rgb);
            }
        });
        let pal_sw = gtk::ScrolledWindow::new();
        pal_sw.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        pal_sw.set_size_request(PAL_W + 10, -1);
        pal_sw.set_vexpand(true);
        pal_sw.set_child(Some(&palette.da));

        let toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        toolbar.add_css_class("toolbar");
        let shape = ctx.settings.borrow().shape.clone();
        let mut tool_buttons = HashMap::new();
        let specs: [(&'static str, String, &str); 5] = [
            ("pen", "pen".into(), "Pencil (P)  -  right-click: size"),
            ("hl", "hl".into(), "Highlighter (H)  -  right-click: thickness / opacity"),
            ("text", "text".into(), "Text (T)  -  right-click: font"),
            ("select", "select".into(), "Select area (S)  -  drag to move, Ctrl+drag to copy, Del to remove"),
            ("shape", shape, "Shapes  -  click for list, right-click: stroke / fill"),
        ];
        for (key, icon, tip) in specs {
            let b = IconButton::new(&icon, tip);
            if key == "shape" {
                b.set_dropdown(true);
            }
            toolbar.append(&b.btn);
            tool_buttons.insert(key, b);
        }
        let sep = gtk::Separator::new(gtk::Orientation::Vertical);
        sep.set_margin_top(6);
        sep.set_margin_bottom(6);
        toolbar.append(&sep);
        let color_btn = IconButton::new("swatch", "Current color  -  click for a custom color");
        toolbar.append(&color_btn.btn);
        let pick = IconButton::new("pick", "Pick color from the image (I)");
        toolbar.append(&pick.btn);
        tool_buttons.insert("pick", pick);

        let body = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        body.append(&pal_sw);
        body.append(&gtk::Separator::new(gtk::Orientation::Vertical));
        body.append(&overlay);
        let tv = adw::ToolbarView::new();
        tv.add_top_bar(&header);
        tv.add_top_bar(&toolbar);
        tv.set_content(Some(&body));
        window.set_content(Some(&tv));

        let win = Rc::new(Win {
            ctx: ctx.clone(),
            window,
            canvas,
            header,
            toolbar,
            wtitle,
            file_btn,
            undo_btn,
            redo_btn,
            zoom_btn,
            palette,
            pal_sw,
            tool_buttons,
            color_btn,
            popovers: RefCell::new(HashMap::new()),
            toast_label,
            toast_id: RefCell::new(None),
            path: RefCell::new(path),
            owned: Cell::new(false),
            prev_tool: Cell::new(Tool::Pen),
            corrected: Cell::new(false),
            centered: Cell::new(false),
            capped: Cell::new(false),
            size: Cell::new((0, 0)),
            dev,
            mon: (mw, mh),
        });
        *palette_ref.borrow_mut() = Some(win.clone());
        win.wire();
        win.fit_window_to_image();
        win
    }

    pub fn present(self: &Rc<Self>) {
        if self.ctx.timing {
            self.window.connect_map(|w| {
                if let Some(clock) = w.frame_clock() {
                    clock.connect_after_paint(|_| {
                        crate::lap("first paint");
                        let ms = crate::START.get().map_or(0.0, |s| s.elapsed().as_secs_f64() * 1000.0);
                        println!("{ms:.1}");
                        std::process::exit(0);
                    });
                }
            });
        }
        if let Ok(out) = std::env::var("SHOTNOTE_DUMP") {
            let w = self.window.clone();
            glib::timeout_add_local_once(Duration::from_millis(1500), move || {
                dump_window(&w, &out);
                std::process::exit(0);
            });
        }
        crate::lap("present");
        self.window.present();
        #[cfg(feature = "selftest")]
        if let Ok(f) = std::env::var("SHOTNOTE_SCRIPT") {
            let script = std::fs::read_to_string(f).unwrap_or_default();
            let w = self.clone();
            glib::timeout_add_local_once(Duration::from_millis(1200), move || crate::selftest::run(w, script));
        }
    }

    #[cfg(feature = "selftest")]
    pub(crate) fn test_popover(self: &Rc<Self>, which: &str) {
        for p in self.popovers.borrow().values() {
            p.popdown();
        }
        match which {
            "shape_menu" => self.popover_for("shape_menu").popup(),
            "pen" => self.show_options("pen"),
            "hl" => self.show_options("hl"),
            "text" => self.show_options("text"),
            _ => self.show_options("shape"),
        }
    }

    #[cfg(feature = "selftest")]
    pub(crate) fn set_size(self: &Rc<Self>, which: &str, v: i64) {
        self.cfg(|c| match which {
            "pen" => c.pen_size = v,
            "hl" => c.hl_size = v,
            "shape" => c.shape_size = v,
            _ => c.font_size = v,
        });
        self.apply_params();
    }

    fn wire(self: &Rc<Self>) {
        let w = self.clone();
        self.canvas.set_callbacks(Callbacks {
            on_zoom: Some(Rc::new({
                let w = w.clone();
                move |s| w.zoom_changed(s)
            })),
            on_history: Some(Rc::new({
                let w = w.clone();
                move |u, r| {
                    w.undo_btn.set_sensitive(u);
                    w.redo_btn.set_sensitive(r);
                }
            })),
            on_dirty: Some(Rc::new({
                let w = w.clone();
                move || w.update_title()
            })),
            on_size: Some(Rc::new({
                let w = w.clone();
                move |cw, ch| w.on_canvas_size(cw, ch)
            })),
            on_pick: Some(Rc::new({
                let w = w.clone();
                move |rgb| w.set_color(rgb)
            })),
            on_finish_pick: Some(Rc::new({
                let w = w.clone();
                move || w.finish_pick()
            })),
        });

        let c = self.clone();
        self.undo_btn.connect_clicked(move |_| c.canvas.do_undo());
        let c = self.clone();
        self.redo_btn.connect_clicked(move |_| c.canvas.do_redo());
        let c = self.clone();
        self.zoom_btn.connect_clicked(move |_| c.canvas.zoom_actual());
        self.undo_btn.set_sensitive(false);
        self.redo_btn.set_sensitive(false);

        self.file_btn.set_menu_model(Some(&self.build_menu()));

        for (key, b) in &self.tool_buttons {
            let key: &'static str = key;
            let c = self.clone();
            if key == "shape" {
                b.btn.connect_clicked(move |_| c.popover_for("shape_menu").popup());
            } else if key == "pick" {
                b.btn.connect_clicked(move |_| c.choose_tool(Tool::Pick));
            } else {
                let tool = match key {
                    "pen" => Tool::Pen,
                    "hl" => Tool::Hl,
                    "text" => Tool::Text,
                    _ => Tool::Select,
                };
                b.btn.connect_clicked(move |_| c.choose_tool(tool));
            }
            if matches!(key, "pen" | "hl" | "text" | "shape") {
                let rc = gtk::GestureClick::new();
                rc.set_button(3);
                let c = self.clone();
                rc.connect_pressed(move |g, _, _, _| {
                    g.set_state(gtk::EventSequenceState::Claimed);
                    c.show_options(key);
                });
                b.btn.add_controller(rc);
                let lp = gtk::GestureLongPress::new();
                let c = self.clone();
                lp.connect_pressed(move |_, _, _| c.show_options(key));
                b.btn.add_controller(lp);
            }
        }
        let c = self.clone();
        self.color_btn.btn.connect_clicked(move |_| c.custom_color());

        self.update_title();
        self.zoom_changed(self.canvas.scale());
        self.choose_tool(Tool::Pen);

        let kc = gtk::EventControllerKey::new();
        kc.set_propagation_phase(gtk::PropagationPhase::Capture);
        let c = self.clone();
        kc.connect_key_pressed(move |_, key, _, state| c.on_key(key, state));
        let c = self.clone();
        kc.connect_key_released(move |_, key, _, _| {
            if key_of(key) == "space" {
                c.canvas.set_space(false);
            }
        });
        self.window.add_controller(kc);
        let c = self.clone();
        self.window.connect_close_request(move |_| c.on_close());
        let c = self.clone();
        self.window.connect_map(move |_| c.on_map());
    }

    // ---- sizing
    fn fit_window_to_image(&self) {
        let (iw, ih) = self.canvas.img_size();
        let v = gtk::Orientation::Vertical;
        let hz = gtk::Orientation::Horizontal;
        let chrome_h = self.header.measure(v, -1).1 + self.toolbar.measure(v, -1).1;
        let min_w = self.toolbar.measure(hz, -1).1.max(self.header.measure(hz, -1).1.min(520));
        let (w, h, capped) = view::window_size(iw, ih, self.dev, PAL_W + 10 + 1, chrome_h, min_w, self.mon.0, self.mon.1);
        self.capped.set(capped);
        self.corrected.set(false);
        self.size.set((w, h));
        self.window.set_default_size(w, h);
    }

    fn on_canvas_size(&self, w: i32, h: i32) {
        if self.corrected.replace(true) || self.capped.get() {
            return;
        }
        let (iw, ih) = self.canvas.img_size();
        let nw = (iw as f64 / self.dev).ceil() as i32;
        let nh = (ih as f64 / self.dev).ceil() as i32;
        let dw = if w < nw { nw - w } else { 0 };
        let dh = if h < nh { nh - h } else { 0 };
        if dw > 0 || dh > 0 {
            let (sw, sh) = self.size.get();
            self.window.set_default_size(sw + dw, sh + dh);
        }
    }

    fn on_map(self: &Rc<Self>) {
        if !self.centered.replace(true) {
            let c = self.clone();
            glib::timeout_add_local_once(Duration::from_millis(80), move || {
                c.scroll_palette(c.palette.index(), true);
            });
        }
    }

    // ---- menu
    fn build_menu(self: &Rc<Self>) -> gio::Menu {
        fn mi(label: &str, action: &str, accel: &str) -> gio::MenuItem {
            let it = gio::MenuItem::new(Some(label), Some(action));
            it.set_attribute_value("accel", Some(&accel.to_variant()));
            it
        }
        let menu = gio::Menu::new();
        menu.append_item(&mi("New screenshot", "win.new", "<Primary>n"));
        menu.append_item(&mi("Open…", "win.open", "<Primary>o"));
        let sections: Vec<Vec<gio::MenuItem>> = vec![
            vec![mi("Save", "win.save", "<Primary>s"), mi("Save As…", "win.saveas", "<Primary><Shift>s")],
            vec![mi("Copy image / selection", "win.copy", "<Primary>c")],
            vec![mi("Zoom to fit", "win.fit", "<Primary>0"), mi("Actual size", "win.actual", "<Primary>1")],
            vec![
                mi("Next image", "win.next", "Right"),
                mi("Previous image", "win.prev", "Left"),
                mi("Fullscreen", "win.fullscreen", "F11"),
            ],
            vec![
                mi("Rotate left", "win.rotl", "bracketleft"),
                mi("Rotate right", "win.rotr", "bracketright"),
                mi("Flip horizontal", "win.fliph", "f"),
                mi("Flip vertical", "win.flipv", "v"),
            ],
            vec![mi("Details…", "win.details", "<Primary>i"), mi("Move to Trash", "win.trash", "")],
        ];
        for sec in sections {
            let m = gio::Menu::new();
            for it in sec {
                m.append_item(&it);
            }
            menu.append_section(None, &m);
        }
        type Act = Box<dyn Fn(&Rc<Win>)>;
        let actions: Vec<(&str, Act)> = vec![
            ("new", Box::new(|w| w.ctx.capture(Some(w.clone())))),
            ("open", Box::new(|w| w.open_dialog())),
            ("save", Box::new(|w| {
                w.save(None);
            })),
            ("saveas", Box::new(|w| w.save_as())),
            ("copy", Box::new(|w| w.copy_image())),
            ("fit", Box::new(|w| w.canvas.zoom_fit())),
            ("actual", Box::new(|w| w.canvas.zoom_actual())),
            ("next", Box::new(|w| w.navigate(1))),
            ("prev", Box::new(|w| w.navigate(-1))),
            ("fullscreen", Box::new(|w| w.toggle_fullscreen())),
            ("rotl", Box::new(|w| w.canvas.transform(Xform::RotLeft))),
            ("rotr", Box::new(|w| w.canvas.transform(Xform::RotRight))),
            ("fliph", Box::new(|w| w.canvas.transform(Xform::FlipH))),
            ("flipv", Box::new(|w| w.canvas.transform(Xform::FlipV))),
            ("details", Box::new(|w| w.show_details())),
            ("trash", Box::new(|w| w.trash())),
        ];
        for (name, f) in actions {
            let a = gio::SimpleAction::new(name, None);
            let w = self.clone();
            a.connect_activate(move |_, _| f(&w));
            self.window.add_action(&a);
        }
        menu
    }

    // ---- popovers
    fn cfg<R>(&self, f: impl FnOnce(&mut Settings) -> R) -> R {
        let r = f(&mut self.ctx.settings.borrow_mut());
        self.ctx.save_soon();
        r
    }

    fn slider_row(self: &Rc<Self>, label: &str, lo: f64, hi: f64, init: i64, set: impl Fn(&mut Settings, i64) + 'static) -> gtk::Box {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        let lbl = gtk::Label::new(Some(label));
        lbl.set_xalign(0.0);
        lbl.set_size_request(70, -1);
        row.append(&lbl);
        let sc = gtk::Scale::with_range(gtk::Orientation::Horizontal, lo, hi, 1.0);
        sc.set_size_request(190, -1);
        sc.set_draw_value(true);
        sc.set_value_pos(gtk::PositionType::Right);
        sc.set_value(init as f64);
        let w = self.clone();
        sc.connect_value_changed(move |s| {
            let v = s.value() as i64;
            w.cfg(|c| set(c, v));
            w.apply_params();
        });
        row.append(&sc);
        row
    }

    fn font_rows(self: &Rc<Self>) -> Vec<gtk::Widget> {
        let mut names: Vec<String> = pangocairo::FontMap::default()
            .list_families()
            .iter()
            .map(|f| f.name().to_string())
            .collect();
        names.sort_by_key(|n| n.to_lowercase());
        names.dedup();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let dd = gtk::DropDown::from_strings(&refs);
        dd.set_enable_search(true);
        dd.set_expression(Some(gtk::PropertyExpression::new(
            gtk::StringObject::static_type(),
            None::<gtk::Expression>,
            "string",
        )));
        dd.set_size_request(250, -1);
        let fac = gtk::SignalListItemFactory::new();
        fac.connect_setup(|_, li| {
            let li = li.downcast_ref::<gtk::ListItem>().expect("list item");
            let l = gtk::Label::builder()
                .xalign(0.0)
                .ellipsize(pango::EllipsizeMode::End)
                .max_width_chars(26)
                .build();
            li.set_child(Some(&l));
        });
        fac.connect_bind(|_, li| {
            let li = li.downcast_ref::<gtk::ListItem>().expect("list item");
            if let (Some(l), Some(item)) = (li.child().and_downcast::<gtk::Label>(), li.item().and_downcast::<gtk::StringObject>()) {
                l.set_text(&item.string());
            }
        });
        dd.set_factory(Some(&fac));
        let cur = self.ctx.settings.borrow().font_family.clone();
        if let Some(i) = names.iter().position(|n| *n == cur) {
            dd.set_selected(i as u32);
        }
        let w = self.clone();
        dd.connect_selected_notify(move |d| {
            if let Some(it) = d.selected_item().and_downcast::<gtk::StringObject>() {
                w.cfg(|c| c.font_family = it.string().to_string());
                w.apply_params();
            }
        });

        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let l = gtk::Label::new(Some("Size"));
        l.set_xalign(0.0);
        row.append(&l);
        let sp = gtk::SpinButton::with_range(8.0, 400.0, 1.0);
        sp.set_value(self.ctx.settings.borrow().font_size as f64);
        let w = self.clone();
        sp.connect_value_changed(move |s| {
            w.cfg(|c| c.font_size = s.value() as i64);
            w.apply_params();
        });
        row.append(&sp);
        let bold = gtk::ToggleButton::new();
        let bl = gtk::Label::new(None);
        bl.set_markup("<b>B</b>");
        bold.set_child(Some(&bl));
        bold.set_size_request(36, 36);
        bold.set_tooltip_text(Some("Bold"));
        bold.set_active(self.ctx.settings.borrow().bold);
        let w = self.clone();
        bold.connect_toggled(move |b| {
            w.cfg(|c| c.bold = b.is_active());
            w.apply_params();
        });
        row.append(&bold);
        vec![dd.upcast(), row.upcast()]
    }

    fn popover_for(self: &Rc<Self>, key: &'static str) -> gtk::Popover {
        if let Some(p) = self.popovers.borrow().get(key) {
            return p.clone();
        }
        let parent = &self.tool_buttons[if key.starts_with("shape") { "shape" } else { key }];
        let menu = key == "shape_menu";
        let s = self.ctx.settings.borrow().clone();
        let kids: Vec<gtk::Widget> = match key {
            "pen" => vec![self.slider_row("Size", 1.0, 40.0, s.pen_size, |c, v| c.pen_size = v).upcast()],
            "hl" => vec![
                self.slider_row("Thickness", 4.0, 80.0, s.hl_size, |c, v| c.hl_size = v).upcast(),
                self.slider_row("Opacity %", 10.0, 100.0, s.hl_alpha, |c, v| c.hl_alpha = v).upcast(),
            ],
            "text" => self.font_rows(),
            "shape" => {
                let fill = gtk::CheckButton::with_label("Fill shapes");
                fill.set_active(s.fill);
                let w = self.clone();
                fill.connect_toggled(move |b| {
                    w.cfg(|c| c.fill = b.is_active());
                    w.apply_params();
                });
                vec![self.slider_row("Stroke", 1.0, 40.0, s.shape_size, |c, v| c.shape_size = v).upcast(), fill.upcast()]
            }
            _ => {
                let mut v = Vec::new();
                for (name, label) in [
                    ("line", "Line   (L)"),
                    ("rect", "Rectangle   (R)"),
                    ("ellipse", "Ellipse / circle   (E)"),
                    ("tri", "Triangle   (G)"),
                ] {
                    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
                    row.append(&icon_widget(name));
                    let l = gtk::Label::new(Some(label));
                    l.set_xalign(0.0);
                    row.append(&l);
                    let b = gtk::Button::new();
                    b.add_css_class("flat");
                    b.set_child(Some(&row));
                    let w = self.clone();
                    b.connect_clicked(move |_| {
                        w.popover_for("shape_menu").popdown();
                        w.choose_tool(Tool::from_shape(name));
                    });
                    v.push(b.upcast());
                }
                v
            }
        };
        let pop = gtk::Popover::new();
        pop.set_parent(&parent.btn);
        pop.set_position(gtk::PositionType::Bottom);
        let bx = gtk::Box::new(gtk::Orientation::Vertical, if menu { 2 } else { 8 });
        let m = if menu { 4 } else { 10 };
        bx.set_margin_start(m);
        bx.set_margin_end(m);
        bx.set_margin_top(m);
        bx.set_margin_bottom(m);
        for k in kids {
            bx.append(&k);
        }
        pop.set_child(Some(&bx));
        let w = self.clone();
        pop.connect_closed(move |_| match w.canvas.text_view() {
            Some(tv) => {
                tv.grab_focus();
            }
            None => {
                w.canvas.grab_focus();
            }
        });
        self.popovers.borrow_mut().insert(key, pop.clone());
        pop
    }

    fn show_options(self: &Rc<Self>, key: &'static str) {
        let tool = match key {
            "pen" => Tool::Pen,
            "hl" => Tool::Hl,
            "text" => Tool::Text,
            _ => Tool::from_shape(&self.ctx.settings.borrow().shape),
        };
        self.choose_tool(tool);
        self.popover_for(key).popup();
    }

    // ---- tool, color and parameter state
    pub(crate) fn choose_tool(self: &Rc<Self>, t: Tool) {
        if t == Tool::Pick && self.canvas.tool() != Tool::Pick {
            self.prev_tool.set(self.canvas.tool());
        }
        if let Some(name) = t.shape_name() {
            self.cfg(|c| c.shape = name.into());
            self.tool_buttons["shape"].set_icon(name);
        }
        self.canvas.set_tool(t);
        for (key, b) in &self.tool_buttons {
            let active = *key == tool_key(t);
            if active {
                b.btn.add_css_class("tool-active");
            } else {
                b.btn.remove_css_class("tool-active");
            }
        }
        self.apply_color();
        self.apply_params();
    }

    fn finish_pick(self: &Rc<Self>) {
        let p = self.prev_tool.get();
        self.choose_tool(if p == Tool::Pick { Tool::Pen } else { p });
    }

    fn cur_hl(&self) -> bool {
        let mut t = self.canvas.tool();
        if t == Tool::Pick {
            t = self.prev_tool.get();
        }
        t == Tool::Hl
    }

    fn cur_color(&self) -> Rgb {
        let s = self.ctx.settings.borrow();
        let hex = if self.cur_hl() { &s.hl_rgb } else { &s.pen_rgb };
        parse_hex(hex).unwrap_or((1.0, 0.0, 0.0))
    }

    fn apply_color(self: &Rc<Self>) {
        let rgb = self.cur_color();
        self.palette.set_color(rgb);
        self.scroll_palette(self.palette.index(), false);
        self.color_btn.set_rgb(rgb);
        self.apply_params();
    }

    pub(crate) fn set_color(self: &Rc<Self>, rgb: Rgb) {
        let hex = hexcol(rgb);
        let hl = self.cur_hl();
        self.cfg(|c| if hl { c.hl_rgb = hex } else { c.pen_rgb = hex });
        self.apply_color();
    }

    fn custom_color(self: &Rc<Self>) {
        let dlg = gtk::ColorDialog::new();
        dlg.set_with_alpha(false);
        let (r, g, b) = self.cur_color();
        let cur = gdk::RGBA::new(r as f32, g as f32, b as f32, 1.0);
        let w = self.clone();
        glib::spawn_future_local(async move {
            if let Ok(rgba) = dlg.choose_rgba_future(Some(&w.window), Some(&cur)).await {
                w.set_color((rgba.red() as f64, rgba.green() as f64, rgba.blue() as f64));
            }
        });
    }

    fn apply_params(self: &Rc<Self>) {
        let s = self.ctx.settings.borrow();
        let t = self.canvas.tool();
        let size = if t.is_shape() {
            s.shape_size
        } else if t == Tool::Hl {
            s.hl_size
        } else {
            s.pen_size
        };
        let color = {
            let hex = if self.cur_hl() { &s.hl_rgb } else { &s.pen_rgb };
            parse_hex(hex).unwrap_or((1.0, 0.0, 0.0))
        };
        self.canvas.set_params(Params {
            color,
            size: size as f64,
            hl_alpha: s.hl_alpha as f64 / 100.0,
            fill: s.fill,
            font_family: s.font_family.clone(),
            font_size: s.font_size as i32,
            bold: s.bold,
        });
    }

    fn scroll_palette(&self, idx: i32, center: bool) {
        if idx < 0 {
            return;
        }
        let adj = self.pal_sw.vadjustment();
        let y = (idx * PAL_H) as f64;
        let page = adj.page_size();
        if center || y < adj.value() || y + PAL_H as f64 > adj.value() + page {
            adj.set_value((y - page / 2.0).max(0.0));
        }
    }

    fn zoom_changed(&self, s: f64) {
        self.zoom_btn.set_label(&format!("{:.0}%", s * self.dev * 100.0));
    }

    fn name(&self) -> String {
        self.path
            .borrow()
            .as_ref()
            .and_then(|p| p.file_name())
            .map_or_else(|| "Untitled".into(), |n| n.to_string_lossy().into_owned())
    }

    fn update_title(&self) {
        let name = self.name();
        let dot = if self.canvas.dirty() { "• " } else { "" };
        self.wtitle.set_title(&format!("{dot}{name}"));
        self.window.set_title(Some(&name));
    }

    fn toast(self: &Rc<Self>, msg: &str) {
        self.toast_label.set_text(msg);
        self.toast_label.set_visible(true);
        if let Some(id) = self.toast_id.borrow_mut().take() {
            id.remove();
        }
        let w = self.clone();
        let id = glib::timeout_add_local_once(Duration::from_millis(1300), move || {
            w.toast_id.borrow_mut().take();
            w.toast_label.set_visible(false);
        });
        *self.toast_id.borrow_mut() = Some(id);
    }

    // ---- keyboard
    pub(crate) fn on_key(self: &Rc<Self>, key: gdk::Key, state: gdk::ModifierType) -> glib::Propagation {
        use glib::Propagation::{Proceed, Stop};
        let c = &self.canvas;
        let ctrl = state.contains(gdk::ModifierType::CONTROL_MASK);
        let shift = state.contains(gdk::ModifierType::SHIFT_MASK);
        let alt = state.contains(gdk::ModifierType::ALT_MASK);
        let k = key_of(key);
        let in_editable = gtk::prelude::GtkWindowExt::focus(&self.window).is_some_and(|f| f.dynamic_cast_ref::<gtk::Editable>().is_some());
        let editing = c.text_active() || in_editable;
        if editing && (!ctrl || matches!(k.as_str(), "z" | "y" | "a" | "c" | "v" | "x")) {
            return Proceed;
        }
        if ctrl {
            match k.as_str() {
                "z" => {
                    if shift {
                        c.do_redo()
                    } else {
                        c.do_undo()
                    }
                }
                "y" => c.do_redo(),
                "s" => {
                    if shift {
                        self.save_as()
                    } else {
                        self.save(None);
                    }
                }
                "c" => self.copy_image(),
                "o" => self.open_dialog(),
                "n" => self.ctx.capture(Some(self.clone())),
                "w" | "q" => self.window.close(),
                "0" => c.zoom_fit(),
                "1" => c.zoom_actual(),
                "plus" | "equal" | "kp_add" => c.zoom_by(1.25),
                "minus" | "underscore" | "kp_subtract" => c.zoom_by(1.0 / 1.25),
                "a" => {
                    self.choose_tool(Tool::Select);
                    c.select_all();
                }
                "i" => self.show_details(),
                _ => return Proceed,
            }
            return Stop;
        }
        if alt {
            return Proceed;
        }
        match k.as_str() {
            "space" => {
                c.set_space(true);
                return Stop;
            }
            "delete" | "backspace" => {
                c.delete_selection();
                return Stop;
            }
            "escape" => {
                if self.window.is_fullscreen() && !c.has_selection() && c.tool() != Tool::Pick {
                    self.window.unfullscreen();
                } else {
                    c.escape();
                }
                return Stop;
            }
            "f11" => {
                self.toggle_fullscreen();
                return Stop;
            }
            "page_down" => {
                self.navigate(1);
                return Stop;
            }
            "page_up" => {
                self.navigate(-1);
                return Stop;
            }
            "right" | "left" if !c.has_selection() => {
                self.navigate(if k == "right" { 1 } else { -1 });
                return Stop;
            }
            _ => {}
        }
        if shift {
            return Proceed;
        }
        let xf = match k.as_str() {
            "bracketleft" => Some(Xform::RotLeft),
            "bracketright" => Some(Xform::RotRight),
            "f" => Some(Xform::FlipH),
            "v" => Some(Xform::FlipV),
            _ => None,
        };
        if let Some(x) = xf {
            c.transform(x);
            return Stop;
        }
        let tool = match k.as_str() {
            "i" => Tool::Pick,
            "p" => Tool::Pen,
            "h" => Tool::Hl,
            "t" => Tool::Text,
            "s" => Tool::Select,
            "l" => Tool::Line,
            "r" => Tool::Rect,
            "e" | "o" => Tool::Ellipse,
            "g" => Tool::Tri,
            _ => return Proceed,
        };
        self.choose_tool(tool);
        Stop
    }

    fn toggle_fullscreen(&self) {
        self.window.set_fullscreened(!self.window.is_fullscreen());
    }

    // ---- file operations
    pub(crate) fn copy_image(self: &Rc<Self>) {
        self.canvas.commit_text();
        let surf = self.canvas.copy_surface();
        let mut png = Vec::new();
        let ok = surf.write_to_png(&mut png).is_ok();
        let w = self.clone();
        glib::spawn_future_local(async move {
            if ok {
                let sent = gio::spawn_blocking(move || -> std::io::Result<bool> {
                    use std::io::Write;
                    let mut child = std::process::Command::new("wl-copy")
                        .args(["-t", "image/png"])
                        .stdin(std::process::Stdio::piped())
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .spawn()?;
                    child.stdin.take().expect("stdin").write_all(&png)?;
                    Ok(child.wait()?.success())
                })
                .await;
                if matches!(sent, Ok(Ok(true))) {
                    w.toast("Copied");
                    return;
                }
            }
            let tex = texture_from_surface(&surf);
            w.window.clipboard().set_texture(&tex);
            w.toast("Copied");
        });
    }

    /// Writes the image to the copy path. Returns false when the write failed.
    pub(crate) fn save(self: &Rc<Self>, after: Option<Box<dyn FnOnce()>>) -> bool {
        self.canvas.commit_text();
        let target = naming::save_target(
            self.path.borrow().as_deref(),
            self.owned.get(),
            &capture::screenshots_dir(),
            &capture::stamp(),
            &|p| p.exists(),
        );
        if let Some(dir) = target.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = self.canvas.write_png(&target) {
            self.toast(&format!("Save failed: {e}"));
            return false;
        }
        self.finish_save(target);
        if let Some(f) = after {
            f();
        }
        true
    }

    fn finish_save(self: &Rc<Self>, target: PathBuf) {
        *self.path.borrow_mut() = Some(target);
        self.owned.set(true);
        self.canvas.set_dirty(false);
        self.toast("Saved");
    }

    fn save_as(self: &Rc<Self>) {
        self.canvas.commit_text();
        let suggested = naming::save_target(
            self.path.borrow().as_deref(),
            self.owned.get(),
            &capture::screenshots_dir(),
            &capture::stamp(),
            &|p| p.exists(),
        );
        let dlg = gtk::FileDialog::new();
        dlg.set_title("Save As");
        dlg.set_initial_name(suggested.file_name().and_then(|n| n.to_str()));
        if let Some(dir) = suggested.parent() {
            dlg.set_initial_folder(Some(&gio::File::for_path(dir)));
        }
        let w = self.clone();
        glib::spawn_future_local(async move {
            let Ok(f) = dlg.save_future(Some(&w.window)).await else { return };
            let Some(mut p) = f.path() else { return };
            if p.extension().and_then(|e| e.to_str()).map(str::to_lowercase).as_deref() != Some("png") {
                let mut s = p.into_os_string();
                s.push(".png");
                p = s.into();
            }
            match w.canvas.write_png(&p) {
                Ok(()) => w.finish_save(p),
                Err(e) => w.toast(&format!("Save failed: {e}")),
            }
        });
    }

    fn open_dialog(self: &Rc<Self>) {
        let dlg = gtk::FileDialog::new();
        let flt = gtk::FileFilter::new();
        flt.set_name(Some("Images"));
        flt.add_mime_type("image/*");
        dlg.set_default_filter(Some(&flt));
        dlg.set_initial_folder(Some(&gio::File::for_path(capture::screenshots_dir())));
        let w = self.clone();
        glib::spawn_future_local(async move {
            if let Ok(f) = dlg.open_future(Some(&w.window)).await {
                if let Some(p) = f.path() {
                    w.ctx.open_path(&p);
                }
            }
        });
    }

    // ---- viewer
    /// Runs `then` now when nothing is unsaved, otherwise after the user decides.
    fn guard_unsaved(self: &Rc<Self>, then: Box<dyn Fn()>) {
        self.canvas.commit_text();
        if !self.canvas.dirty() {
            then();
            return;
        }
        let d = adw::AlertDialog::new(Some("Save changes?"), Some("Your annotations haven't been saved."));
        d.add_responses(&[("cancel", "Cancel"), ("discard", "Discard"), ("save", "Save")]);
        d.set_response_appearance("discard", adw::ResponseAppearance::Destructive);
        d.set_response_appearance("save", adw::ResponseAppearance::Suggested);
        d.set_default_response(Some("save"));
        d.set_close_response("cancel");
        let w = self.clone();
        d.connect_response(None, move |_, r| match r {
            "save" => {
                if w.save(None) {
                    then();
                }
            }
            "discard" => {
                w.canvas.set_dirty(false);
                then();
            }
            _ => {}
        });
        d.present(Some(&self.window));
    }

    pub(crate) fn navigate(self: &Rc<Self>, step: i64) {
        let Some(cur) = self.path.borrow().clone() else { return };
        let list = folder::list_images(&cur);
        let Some(next) = folder::neighbor(&list, &cur, step) else { return };
        let w = self.clone();
        self.guard_unsaved(Box::new(move || w.load_path(&next)));
    }

    pub(crate) fn load_path(self: &Rc<Self>, p: &Path) {
        match surface_from_file(p) {
            Ok(s) => {
                self.canvas.load_surface(s);
                *self.path.borrow_mut() = Some(p.to_path_buf());
                self.owned.set(false);
                self.update_title();
                if !self.window.is_fullscreen() && !self.window.is_maximized() {
                    self.fit_window_to_image();
                }
            }
            Err(e) => self.toast(&format!("Could not open {}: {e}", p.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned()))),
        }
    }

    fn show_details(self: &Rc<Self>) {
        let (iw, ih) = self.canvas.img_size();
        let path = self.path.borrow().clone();
        let size = path
            .as_ref()
            .and_then(|p| std::fs::metadata(p).ok())
            .map_or_else(|| "unsaved".to_string(), |m| human_size(m.len()));
        let body = format!(
            "Name: {}\nSize: {}\nDimensions: {} x {} px\nPath: {}",
            self.name(),
            size,
            iw,
            ih,
            path.as_ref().map_or_else(|| "none".to_string(), |p| p.display().to_string())
        );
        let d = adw::AlertDialog::new(Some("Details"), Some(&body));
        d.add_response("close", "Close");
        d.set_close_response("close");
        d.present(Some(&self.window));
    }

    pub(crate) fn trash(self: &Rc<Self>) {
        let Some(cur) = self.path.borrow().clone() else {
            self.toast("Nothing to trash: the image has no file");
            return;
        };
        let list = folder::list_images(&cur);
        let next = folder::neighbor(&list, &cur, 1);
        match gio::File::for_path(&cur).trash(gio::Cancellable::NONE) {
            Ok(()) => match next {
                Some(n) => {
                    self.canvas.set_dirty(false);
                    self.load_path(&n);
                }
                None => {
                    self.canvas.set_dirty(false);
                    self.window.close();
                }
            },
            Err(e) => self.toast(&format!("Trash failed: {}", e.message())),
        }
    }

    fn on_close(self: &Rc<Self>) -> glib::Propagation {
        self.canvas.commit_text();
        if !self.canvas.dirty() {
            self.ctx.save_now();
            for p in self.popovers.borrow_mut().drain() {
                if p.1.parent().is_some() {
                    p.1.unparent();
                }
            }
            return glib::Propagation::Proceed;
        }
        let w = self.clone();
        self.guard_unsaved(Box::new(move || w.window.close()));
        glib::Propagation::Stop
    }
}

fn human_size(n: u64) -> String {
    let n = n as f64;
    if n >= 1_048_576.0 {
        format!("{:.1} MB", n / 1_048_576.0)
    } else if n >= 1024.0 {
        format!("{:.1} KB", n / 1024.0)
    } else {
        format!("{n} bytes")
    }
}

fn texture_from_surface(s: &ImageSurface) -> gdk::Texture {
    let stride = s.stride() as usize;
    let mut tex = None;
    let (w, h) = (s.width(), s.height());
    let _ = s.with_data(|d| {
        let bytes = glib::Bytes::from(d);
        tex = Some(gdk::MemoryTexture::new(w, h, gdk::MemoryFormat::B8g8r8a8Premultiplied, &bytes, stride));
    });
    tex.expect("texture").upcast()
}

/// Renders the window content to a PNG. Used by `SHOTNOTE_DUMP=<file>` for visual checks.
pub(crate) fn dump_window(win: &adw::ApplicationWindow, path: &str) {
    let (w, h) = (win.width() as f64, win.height() as f64);
    let paintable = gtk::WidgetPaintable::new(Some(win));
    let snap = gtk::Snapshot::new();
    paintable.snapshot(&snap, w, h);
    let (Some(node), Some(renderer)) = (snap.to_node(), win.native().and_then(|n| n.renderer())) else {
        eprintln!("dump failed");
        return;
    };
    let tex = renderer.render_texture(&node, None);
    if let Err(e) = tex.save_to_png(path) {
        eprintln!("dump failed: {e}");
    }
}
