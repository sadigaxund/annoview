//! The editing canvas widget: GPU image, per-frame overlay, input, history and text entry.

use crate::logic::history::{History, Op, Patch, MAX_UNDO_BYTES};
use crate::logic::pixels::{self, Xform};
use crate::logic::view;
use crate::paint::{self, Item, Rgb, Tool};
use gtk::cairo::{Context, Format, ImageSurface};
use gtk::glib::subclass::prelude::*;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib, graphene, gsk};
use std::cell::{Cell, RefCell, RefMut};
use std::rc::Rc;

thread_local! {
    pub static TEXT_CSS: gtk::CssProvider = gtk::CssProvider::new();
}

#[derive(Clone)]
pub struct Params {
    pub color: Rgb,
    pub size: f64,
    pub hl_alpha: f64,
    pub fill: bool,
    pub font_family: String,
    pub font_size: i32,
    pub bold: bool,
}

#[derive(Default, Clone)]
pub struct Callbacks {
    pub on_zoom: Option<Rc<dyn Fn(f64)>>,
    pub on_history: Option<Rc<dyn Fn(bool, bool)>>,
    pub on_dirty: Option<Rc<dyn Fn()>>,
    pub on_size: Option<Rc<dyn Fn(i32, i32)>>,
    pub on_pick: Option<Rc<dyn Fn(Rgb)>>,
    pub on_finish_pick: Option<Rc<dyn Fn()>>,
}

struct Float {
    surf: ImageSurface,
    dx: i32,
    dy: i32,
    bg: Option<Rgb>,
}

struct TextState {
    tv: gtk::TextView,
    pos: (f64, f64),
}

pub struct State {
    surface: ImageSurface,
    w: i32,
    h: i32,
    texture: Option<gdk::Texture>,
    tex_dirty: bool,
    dev: f64,
    scale: f64,
    target: f64,
    ox: f64,
    oy: f64,
    fit_mode: bool,
    history: History,
    tool: Tool,
    params: Params,
    item: Option<Item>,
    sel: Option<(i32, i32, i32, i32)>,
    sel_a: Option<(f64, f64)>,
    float: Option<Float>,
    text: Option<TextState>,
    overlay: Option<gtk::Overlay>,
    space: bool,
    panning: bool,
    pan0: (f64, f64),
    mx: Option<(f64, f64)>,
    tick_active: bool,
    last_t: f64,
    anchor: (f64, f64, f64, f64),
    dirty: bool,
    w0: (f64, f64),
    pz: f64,
}

impl Default for State {
    fn default() -> Self {
        State {
            surface: ImageSurface::create(Format::ARgb32, 1, 1).expect("surface"),
            w: 1,
            h: 1,
            texture: None,
            tex_dirty: true,
            dev: 1.0,
            scale: 1.0,
            target: 1.0,
            ox: 0.0,
            oy: 0.0,
            fit_mode: true,
            history: History::new(MAX_UNDO_BYTES),
            tool: Tool::Pen,
            params: Params {
                color: (1.0, 0.0, 0.0),
                size: 5.0,
                hl_alpha: 0.8,
                fill: false,
                font_family: "Sans".into(),
                font_size: 28,
                bold: false,
            },
            item: None,
            sel: None,
            sel_a: None,
            float: None,
            text: None,
            overlay: None,
            space: false,
            panning: false,
            pan0: (0.0, 0.0),
            mx: None,
            tick_active: false,
            last_t: 0.0,
            anchor: (0.0, 0.0, 0.0, 0.0),
            dirty: false,
            w0: (0.0, 0.0),
            pz: 1.0,
        }
    }
}

impl State {
    fn stride(&self) -> usize {
        self.surface.stride() as usize
    }

    fn read(&self, x: i32, y: i32, w: i32, h: i32) -> Vec<u8> {
        let stride = self.stride();
        let mut out = Vec::new();
        let _ = self.surface.with_data(|d| out = pixels::read_region(d, stride, x, y, w, h));
        out
    }

    fn region_surface(&self, x: i32, y: i32, w: i32, h: i32) -> ImageSurface {
        ImageSurface::create_for_data(self.read(x, y, w, h), Format::ARgb32, w, h, w * 4).expect("region")
    }

    fn ring_color(&self, x: i32, y: i32, w: i32, h: i32) -> Rgb {
        let stride = self.stride();
        let mut out = (1.0, 1.0, 1.0);
        let (iw, ih) = (self.w, self.h);
        let _ = self.surface.with_data(|d| out = pixels::ring_color(d, stride, iw, ih, x, y, w, h));
        out
    }
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Canvas {
        pub st: RefCell<State>,
        pub cb: RefCell<Callbacks>,
        pub notify_guard: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Canvas {
        const NAME: &'static str = "ShotnoteCanvas";
        type Type = super::Canvas;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for Canvas {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().setup();
        }
    }

    impl WidgetImpl for Canvas {
        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            self.parent_size_allocate(width, height, baseline);
            self.obj().on_resize(width, height);
        }

        fn snapshot(&self, snap: &gtk::Snapshot) {
            self.obj().draw(snap);
        }
    }
}

glib::wrapper! {
    pub struct Canvas(ObjectSubclass<imp::Canvas>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> graphene::Rect {
    graphene::Rect::new(x as f32, y as f32, w as f32, h as f32)
}

impl Canvas {
    pub fn new(dev: f64, surface: ImageSurface) -> Canvas {
        let c: Canvas = glib::Object::new();
        {
            let mut s = c.st();
            s.dev = dev;
            s.scale = 1.0 / dev;
            s.target = s.scale;
        }
        c.load_surface(surface);
        c
    }

    fn st(&self) -> RefMut<'_, State> {
        self.imp().st.borrow_mut()
    }

    fn cbs(&self) -> Callbacks {
        self.imp().cb.borrow().clone()
    }

    pub fn set_callbacks(&self, cb: Callbacks) {
        *self.imp().cb.borrow_mut() = cb;
    }

    pub fn set_overlay(&self, o: &gtk::Overlay) {
        self.st().overlay = Some(o.clone());
    }

    // ---- setup and input controllers
    fn setup(&self) {
        self.set_hexpand(true);
        self.set_vexpand(true);
        self.set_focusable(true);
        self.set_overflow(gtk::Overflow::Hidden);
        self.set_cursor_from_name(Some("crosshair"));

        let g = gtk::GestureDrag::new();
        g.set_button(1);
        let w = self.downgrade();
        g.connect_drag_begin(move |g, x, y| {
            if let Some(c) = w.upgrade() {
                c.drag_begin(g, x, y);
            }
        });
        let w = self.downgrade();
        g.connect_drag_update(move |g, dx, dy| {
            if let Some(c) = w.upgrade() {
                c.drag_update(g, dx, dy);
            }
        });
        let w = self.downgrade();
        g.connect_drag_end(move |_, _, _| {
            if let Some(c) = w.upgrade() {
                c.drag_end();
            }
        });
        self.add_controller(g);

        let g2 = gtk::GestureDrag::new();
        g2.set_button(2);
        let w = self.downgrade();
        g2.connect_drag_begin(move |_, _, _| {
            if let Some(c) = w.upgrade() {
                c.pan_begin();
            }
        });
        let w = self.downgrade();
        g2.connect_drag_update(move |_, dx, dy| {
            if let Some(c) = w.upgrade() {
                c.drag_update_pan(dx, dy);
            }
        });
        let w = self.downgrade();
        g2.connect_drag_end(move |_, _, _| {
            if let Some(c) = w.upgrade() {
                c.end_pan();
            }
        });
        self.add_controller(g2);

        let sc = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::BOTH_AXES);
        let w = self.downgrade();
        sc.connect_scroll(move |ctl, dx, dy| match w.upgrade() {
            Some(c) => c.on_scroll(ctl, dx, dy),
            None => glib::Propagation::Proceed,
        });
        self.add_controller(sc);

        let mc = gtk::EventControllerMotion::new();
        let w = self.downgrade();
        mc.connect_motion(move |_, x, y| {
            if let Some(c) = w.upgrade() {
                c.on_motion(x, y);
            }
        });
        let w = self.downgrade();
        mc.connect_leave(move |_| {
            if let Some(c) = w.upgrade() {
                c.st().mx = None;
            }
        });
        self.add_controller(mc);

        let pz = gtk::GestureZoom::new();
        let w = self.downgrade();
        pz.connect_begin(move |_, _| {
            if let Some(c) = w.upgrade() {
                let mut s = c.st();
                s.pz = s.scale;
            }
        });
        let w = self.downgrade();
        pz.connect_scale_changed(move |g, sc| {
            if let Some(c) = w.upgrade() {
                let base = c.st().pz;
                let at = g.bounding_box_center();
                c.zoom_to(base * sc, at);
            }
        });
        self.add_controller(pz);
    }

    // ---- accessors for the window
    pub fn img_size(&self) -> (i32, i32) {
        let s = self.st();
        (s.w, s.h)
    }

    pub fn scale(&self) -> f64 {
        self.st().scale
    }

    pub fn dirty(&self) -> bool {
        self.st().dirty
    }

    pub fn tool(&self) -> Tool {
        self.st().tool
    }

    pub fn text_active(&self) -> bool {
        self.st().text.is_some()
    }

    pub fn has_selection(&self) -> bool {
        self.st().sel.is_some()
    }

    pub fn text_view(&self) -> Option<gtk::TextView> {
        self.st().text.as_ref().map(|t| t.tv.clone())
    }

    pub fn set_dirty(&self, d: bool) {
        self.st().dirty = d;
        self.notify_dirty();
    }

    fn notify_dirty(&self) {
        if let Some(f) = self.cbs().on_dirty {
            f();
        }
    }

    fn notify_history(&self) {
        let (u, r) = {
            let s = self.st();
            (s.history.can_undo(), s.history.can_redo())
        };
        if let Some(f) = self.cbs().on_history {
            f(u, r);
        }
    }

    /// Replaces the image and resets history, selection and view.
    pub fn load_surface(&self, surface: ImageSurface) {
        self.close_text();
        {
            let mut s = self.st();
            s.w = surface.width();
            s.h = surface.height();
            s.surface = surface;
            s.tex_dirty = true;
            s.history.clear();
            s.item = None;
            s.sel = None;
            s.float = None;
            s.sel_a = None;
            s.dirty = false;
            s.fit_mode = true;
        }
        self.image_replaced();
        self.notify_history();
        self.notify_dirty();
    }

    fn image_replaced(&self) {
        {
            let mut s = self.st();
            if s.fit_mode {
                let (vw, vh) = (self.width() as f64, self.height() as f64);
                let f = view::fit_scale(s.dev, vw, vh, s.w as f64, s.h as f64);
                s.scale = f;
                s.target = f;
            }
        }
        self.constrain();
        self.view_changed();
    }

    // ---- view
    fn constrain(&self) {
        let (vw, vh) = (self.width() as f64, self.height() as f64);
        let mut s = self.st();
        let (ox, oy) = view::constrain(vw, vh, s.w as f64, s.h as f64, s.scale, s.ox, s.oy);
        s.ox = ox;
        s.oy = oy;
    }

    fn view_changed(&self) {
        self.queue_draw();
        if self.text_active() {
            self.place_text();
        }
        let sc = self.scale();
        if let Some(f) = self.cbs().on_zoom {
            f(sc);
        }
    }

    fn on_resize(&self, w: i32, h: i32) {
        if w <= 0 || h <= 0 {
            return;
        }
        {
            let mut s = self.st();
            if s.fit_mode {
                let f = view::fit_scale(s.dev, w as f64, h as f64, s.w as f64, s.h as f64);
                s.scale = f;
                s.target = f;
            }
        }
        self.constrain();
        self.view_changed();
        if let Some(f) = self.cbs().on_size {
            f(w, h);
        }
    }

    pub fn zoom_to(&self, s: f64, at: Option<(f64, f64)>) {
        {
            let mut st = self.st();
            let (wx, wy) = at
                .or(st.mx)
                .unwrap_or((self.width() as f64 / 2.0, self.height() as f64 / 2.0));
            st.fit_mode = false;
            st.target = view::clamp_scale(s);
            let (ix, iy) = view::to_img(st.ox, st.oy, st.scale, wx, wy);
            st.anchor = (wx, wy, ix, iy);
        }
        self.start_tick();
    }

    fn zoom_base(&self) -> f64 {
        let s = self.st();
        if s.tick_active {
            s.target
        } else {
            s.scale
        }
    }

    pub fn zoom_by(&self, f: f64) {
        self.zoom_to(self.zoom_base() * f, None);
    }

    fn center(&self) -> (f64, f64) {
        (self.width() as f64 / 2.0, self.height() as f64 / 2.0)
    }

    pub fn zoom_fit(&self) {
        let f = {
            let s = self.st();
            view::fit_scale(s.dev, self.width() as f64, self.height() as f64, s.w as f64, s.h as f64)
        };
        self.zoom_to(f, Some(self.center()));
        self.st().fit_mode = true;
    }

    pub fn zoom_actual(&self) {
        let d = self.st().dev;
        self.zoom_to(1.0 / d, Some(self.center()));
    }

    fn start_tick(&self) {
        {
            let mut s = self.st();
            if s.tick_active {
                return;
            }
            s.tick_active = true;
            s.last_t = 0.0;
        }
        self.add_tick_callback(|w, clock| w.tick(clock));
    }

    fn tick(&self, clock: &gdk::FrameClock) -> glib::ControlFlow {
        let now = clock.frame_time() as f64 / 1e6;
        let done = {
            let mut s = self.st();
            let dt = view::frame_dt(now, s.last_t);
            s.last_t = now;
            let (ns, done) = view::smooth_step(s.scale, s.target, dt);
            s.scale = ns;
            let (wx, wy, ix, iy) = s.anchor;
            let (ox, oy) = view::offset_for_anchor(wx, wy, ix, iy, ns);
            s.ox = ox;
            s.oy = oy;
            if done {
                s.tick_active = false;
            }
            done
        };
        self.constrain();
        self.view_changed();
        if done {
            glib::ControlFlow::Break
        } else {
            glib::ControlFlow::Continue
        }
    }

    fn on_scroll(&self, ctl: &gtk::EventControllerScroll, dx: f64, dy: f64) -> glib::Propagation {
        let st = ctl.current_event_state();
        if st.contains(gdk::ModifierType::CONTROL_MASK) {
            self.zoom_to(self.zoom_base() * (-dy * 0.18).exp(), None);
            return glib::Propagation::Stop;
        }
        let k = if ctl.unit() == gdk::ScrollUnit::Wheel { 60.0 } else { 1.0 };
        let (mut dx, mut dy) = (dx, dy);
        if st.contains(gdk::ModifierType::SHIFT_MASK) && dx == 0.0 {
            dx = dy;
            dy = 0.0;
        }
        {
            let mut s = self.st();
            s.ox -= dx * k;
            s.oy -= dy * k;
            s.fit_mode = false;
        }
        self.constrain();
        self.view_changed();
        glib::Propagation::Stop
    }

    fn pan_begin(&self) {
        {
            let mut s = self.st();
            s.panning = true;
            s.pan0 = (s.ox, s.oy);
        }
        self.set_cursor_from_name(Some("grabbing"));
    }

    fn drag_update_pan(&self, dx: f64, dy: f64) {
        {
            let mut s = self.st();
            s.ox = s.pan0.0 + dx;
            s.oy = s.pan0.1 + dy;
            s.fit_mode = false;
        }
        self.constrain();
        self.view_changed();
    }

    fn end_pan(&self) {
        self.st().panning = false;
    }

    pub fn set_space(&self, on: bool) {
        self.st().space = on;
        let name = if on { "grab" } else { self.cursor_name() };
        self.set_cursor_from_name(Some(name));
    }

    // ---- tools and state
    fn cursor_name(&self) -> &'static str {
        if self.st().tool == Tool::Text {
            "text"
        } else {
            "crosshair"
        }
    }

    pub fn set_params(&self, p: Params) {
        self.st().params = p;
        if self.text_active() {
            self.place_text();
        }
        self.queue_draw();
    }

    pub fn set_tool(&self, t: Tool) {
        self.commit_text();
        {
            let mut s = self.st();
            if t != Tool::Select {
                s.sel = None;
                s.float = None;
            }
            s.tool = t;
        }
        self.set_cursor_from_name(Some(self.cursor_name()));
        self.queue_draw();
    }

    pub fn escape(&self) {
        if self.tool() == Tool::Pick {
            if let Some(f) = self.cbs().on_finish_pick {
                f();
            }
        } else if self.text_active() {
            self.close_text();
        } else {
            {
                let mut s = self.st();
                s.sel = None;
                s.float = None;
            }
            self.queue_draw();
        }
    }

    fn on_motion(&self, x: f64, y: f64) {
        let name = {
            let mut s = self.st();
            s.mx = Some((x, y));
            if s.tool == Tool::Select && !s.space && !s.panning {
                let p = view::to_img(s.ox, s.oy, s.scale, x, y);
                let inside = s.sel.is_some_and(|r| in_rect(r, p));
                Some(if inside { "move" } else { "crosshair" })
            } else {
                None
            }
        };
        if let Some(n) = name {
            self.set_cursor_from_name(Some(n));
        }
    }

    // ---- history
    fn commit(&self, bbox: (f64, f64, f64, f64), painter: impl FnOnce(&Context)) {
        {
            let mut s = self.st();
            let x0 = (bbox.0.floor() as i32).max(0);
            let y0 = (bbox.1.floor() as i32).max(0);
            let x1 = (bbox.2.ceil() as i32).min(s.w);
            let y1 = (bbox.3.ceil() as i32).min(s.h);
            if x1 <= x0 || y1 <= y0 {
                return;
            }
            let (w, h) = (x1 - x0, y1 - y0);
            let before = s.read(x0, y0, w, h);
            {
                let cr = Context::new(&s.surface).expect("context");
                cr.rectangle(x0 as f64, y0 as f64, w as f64, h as f64);
                cr.clip();
                painter(&cr);
            }
            let after = s.read(x0, y0, w, h);
            s.history.push(Op::Patch(Patch { x: x0, y: y0, w, h, before, after }));
            s.tex_dirty = true;
            s.dirty = true;
        }
        self.notify_dirty();
        self.notify_history();
        self.queue_draw();
    }

    fn step(&self, forward: bool) {
        let mut xf: Option<Xform> = None;
        {
            let mut s = self.st();
            s.sel = None;
            s.float = None;
            let State { history, surface, .. } = &mut *s;
            let op = if forward { history.step_forward() } else { history.step_back() };
            match op {
                None => return,
                Some(Op::Patch(p)) => {
                    let stride = surface.stride() as usize;
                    let mut data = surface.data().expect("surface data");
                    p.apply(&mut data, stride, forward);
                }
                Some(Op::Xform(x)) => xf = Some(if forward { *x } else { x.inverse() }),
            }
            s.tex_dirty = true;
            s.dirty = true;
        }
        if let Some(x) = xf {
            self.apply_xform(x);
        }
        self.notify_dirty();
        self.notify_history();
        self.queue_draw();
    }

    pub fn do_undo(&self) {
        self.step(false);
    }

    pub fn do_redo(&self) {
        self.step(true);
    }

    /// Rotates or flips the whole image. The change is recorded in the history.
    pub fn transform(&self, x: Xform) {
        self.commit_text();
        self.st().history.push(Op::Xform(x));
        self.apply_xform(x);
        self.st().dirty = true;
        self.notify_dirty();
        self.notify_history();
    }

    fn apply_xform(&self, x: Xform) {
        {
            let mut s = self.st();
            let (w, h, stride) = (s.w as usize, s.h as usize, s.stride());
            let mut out = None;
            let _ = s.surface.with_data(|d| out = Some(pixels::transform(d, stride, w, h, x)));
            let (buf, nw, nh) = out.expect("transform");
            s.surface = ImageSurface::create_for_data(buf, Format::ARgb32, nw as i32, nh as i32, nw as i32 * 4)
                .expect("surface");
            s.w = nw as i32;
            s.h = nh as i32;
            s.sel = None;
            s.float = None;
            s.item = None;
            s.tex_dirty = true;
        }
        self.image_replaced();
    }

    // ---- drawing
    fn draw(&self, snap: &gtk::Snapshot) {
        let (w, h) = (self.width() as f64, self.height() as f64);
        let dscale = self.scale_factor().max(1) as f64;
        let mut s = self.st();
        snap.append_color(&gdk::RGBA::new(0.14, 0.14, 0.15, 1.0), &rect(0.0, 0.0, w, h));
        if s.tex_dirty || s.texture.is_none() {
            let stride = s.stride();
            let mut tex = None;
            let (iw, ih) = (s.w, s.h);
            let _ = s.surface.with_data(|d| {
                let bytes = glib::Bytes::from(d);
                tex = Some(gdk::MemoryTexture::new(iw, ih, gdk::MemoryFormat::B8g8r8a8Premultiplied, &bytes, stride));
            });
            s.texture = tex.map(|t| t.upcast());
            s.tex_dirty = false;
        }
        let ox = (s.ox * dscale).round() / dscale;
        let oy = (s.oy * dscale).round() / dscale;
        let (iw, ih) = (s.w as f64 * s.scale, s.h as f64 * s.scale);
        let eff = s.scale * dscale;
        let filter = if eff >= 3.0 {
            gsk::ScalingFilter::Nearest
        } else if eff < 1.0 {
            gsk::ScalingFilter::Trilinear
        } else {
            gsk::ScalingFilter::Linear
        };
        let img_rect = rect(ox, oy, iw, ih);
        let hl_live = s.item.as_ref().is_some_and(|i| i.kind == Tool::Hl);
        let tex = s.texture.clone().expect("texture");
        snap.push_clip(&rect(0.0, 0.0, w, h));
        if hl_live {
            snap.push_blend(gsk::BlendMode::Multiply);
        }
        snap.append_scaled_texture(&tex, filter, &img_rect);
        self.draw_float(&s, snap, w, h, ox, oy);
        if hl_live {
            snap.pop();
            let cr = snap.append_cairo(&rect(0.0, 0.0, w, h));
            self.image_space(&cr, &s, ox, oy);
            if let Some(it) = &s.item {
                paint::paint_item(&cr, it, false);
            }
            snap.pop();
        }
        snap.pop();
        self.draw_overlay(&s, snap, w, h, ox, oy, hl_live);
    }

    fn image_space(&self, cr: &Context, s: &State, ox: f64, oy: f64) {
        cr.translate(ox, oy);
        cr.scale(s.scale, s.scale);
        cr.rectangle(0.0, 0.0, s.w as f64, s.h as f64);
        cr.clip();
    }

    fn draw_float(&self, s: &State, snap: &gtk::Snapshot, w: f64, h: f64, ox: f64, oy: f64) {
        let (Some(f), Some((x, y, fw, fh))) = (&s.float, s.sel) else {
            return;
        };
        let cr = snap.append_cairo(&rect(0.0, 0.0, w, h));
        self.image_space(&cr, s, ox, oy);
        if let Some(bg) = f.bg {
            cr.set_source_rgb(bg.0, bg.1, bg.2);
            cr.rectangle(x as f64, y as f64, fw as f64, fh as f64);
            let _ = cr.fill();
        }
        let _ = cr.set_source_surface(&f.surf, (x + f.dx) as f64, (y + f.dy) as f64);
        let _ = cr.paint();
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_overlay(&self, s: &State, snap: &gtk::Snapshot, w: f64, h: f64, ox: f64, oy: f64, hl_live: bool) {
        let live_item = s.item.as_ref().filter(|_| !hl_live);
        let text = s.text.as_ref();
        if live_item.is_none() && text.is_none() && s.sel.is_none() {
            return;
        }
        let cr = snap.append_cairo(&rect(0.0, 0.0, w, h));
        let _ = cr.save();
        self.image_space(&cr, s, ox, oy);
        if let Some(it) = live_item {
            paint::paint_item(&cr, it, true);
        }
        if let Some(t) = text {
            let txt = text_of(&t.tv);
            let p = &s.params;
            cr.set_source_rgb(p.color.0, p.color.1, p.color.2);
            cr.move_to(t.pos.0, t.pos.1);
            let layout = paint::make_layout(&cr, &txt, &p.font_family, p.font_size as f64, p.bold);
            pangocairo::functions::show_layout(&cr, &layout);
        }
        let _ = cr.restore();
        if let Some(t) = text {
            let p = &s.params;
            let (tw, th) = paint::text_extent(&text_of(&t.tv), &p.font_family, p.font_size, p.bold);
            cr.set_line_width(1.0);
            cr.set_source_rgb(0.21, 0.52, 0.89);
            cr.set_dash(&[5.0, 4.0], 0.0);
            cr.rectangle(
                s.ox + t.pos.0 * s.scale - 2.5,
                s.oy + t.pos.1 * s.scale - 2.5,
                tw as f64 * s.scale + 5.0,
                th as f64 * s.scale + 5.0,
            );
            let _ = cr.stroke();
            cr.set_dash(&[], 0.0);
        }
        if let Some((mut x, mut y, sw, sh)) = s.sel {
            if let Some(f) = &s.float {
                x += f.dx;
                y += f.dy;
            }
            let (sx, sy) = (s.ox + x as f64 * s.scale, s.oy + y as f64 * s.scale);
            let (sw, sh) = (sw as f64 * s.scale, sh as f64 * s.scale);
            cr.set_line_width(1.0);
            for (col, off) in [((0.0, 0.0, 0.0), 0.0), ((1.0, 1.0, 1.0), 4.0)] {
                cr.set_source_rgb(col.0, col.1, col.2);
                cr.set_dash(&[4.0, 4.0], off);
                cr.rectangle(sx + 0.5, sy + 0.5, sw, sh);
                let _ = cr.stroke();
            }
        }
    }

    // ---- pointer input
    fn drag_begin(&self, g: &gtk::GestureDrag, x: f64, y: f64) {
        self.grab_focus();
        let state = g.current_event_state();
        if self.text_active() {
            self.commit_text();
        }
        if self.st().space {
            self.pan_begin();
            return;
        }
        let (tool, p) = {
            let mut s = self.st();
            s.w0 = (x, y);
            (s.tool, view::to_img(s.ox, s.oy, s.scale, x, y))
        };
        match tool {
            Tool::Pen | Tool::Hl => {
                let mut s = self.st();
                let width = if tool == Tool::Hl { s.params.size } else { s.params.size };
                let it = Item::new(tool, s.params.color, s.params.fill, width, s.params.hl_alpha, p);
                s.item = Some(it);
            }
            Tool::Text => self.start_text(p),
            Tool::Line | Tool::Rect | Tool::Ellipse | Tool::Tri => {
                let mut s = self.st();
                let it = Item::new(tool, s.params.color, s.params.fill, s.params.size, s.params.hl_alpha, p);
                s.item = Some(it);
            }
            Tool::Pick => self.pick_at(p),
            Tool::Select => {
                let mut s = self.st();
                match s.sel {
                    Some(r) if in_rect(r, p) => {
                        let copy = state.contains(gdk::ModifierType::CONTROL_MASK);
                        let surf = s.region_surface(r.0, r.1, r.2, r.3);
                        let bg = if copy { None } else { Some(s.ring_color(r.0, r.1, r.2, r.3)) };
                        s.float = Some(Float { surf, dx: 0, dy: 0, bg });
                    }
                    _ => {
                        s.sel = None;
                        s.sel_a = Some(p);
                    }
                }
            }
        }
        self.queue_draw();
    }

    fn pick_at(&self, p: (f64, f64)) {
        let rgb = {
            let s = self.st();
            let stride = s.stride();
            let mut out = (0.0, 0.0, 0.0);
            let (w, h) = (s.w, s.h);
            let _ = s.surface.with_data(|d| out = pixels::pick_pixel(d, stride, w, h, p.0, p.1));
            out
        };
        if let Some(f) = self.cbs().on_pick {
            f(rgb);
        }
    }

    fn drag_update(&self, g: &gtk::GestureDrag, dx: f64, dy: f64) {
        if self.st().panning {
            self.drag_update_pan(dx, dy);
            return;
        }
        let shift = g.current_event_state().contains(gdk::ModifierType::SHIFT_MASK);
        let pick = {
            let mut s = self.st();
            let (px, py) = view::to_img(s.ox, s.oy, s.scale, s.w0.0 + dx, s.w0.1 + dy);
            let (scale, dev) = (s.scale, s.dev);
            let (iw, ih) = (s.w as f64, s.h as f64);
            let tool = s.tool;
            if let Some(it) = s.item.as_mut() {
                if it.is_freehand() {
                    let (lx, ly) = *it.pts.last().expect("point");
                    let min = 0.7 / (scale * dev);
                    if (px - lx).powi(2) + (py - ly).powi(2) >= min * min {
                        it.pts.push((px, py));
                    }
                } else {
                    let (x0, y0) = it.p0;
                    let (mut qx, mut qy) = (px, py);
                    if shift {
                        if it.kind == Tool::Line {
                            let step = std::f64::consts::FRAC_PI_4;
                            let ang = ((py - y0).atan2(px - x0) / step).round() * step;
                            let ln = (px - x0).hypot(py - y0);
                            qx = x0 + ln * ang.cos();
                            qy = y0 + ln * ang.sin();
                        } else {
                            let m = (px - x0).abs().max((py - y0).abs());
                            qx = x0 + m.copysign(px - x0);
                            qy = y0 + m.copysign(py - y0);
                        }
                    }
                    it.p1 = (qx, qy);
                }
                None
            } else if tool == Tool::Pick {
                Some((px, py))
            } else {
                if tool == Tool::Select {
                    if let Some(f) = s.float.as_mut() {
                        f.dx = (dx / scale).round() as i32;
                        f.dy = (dy / scale).round() as i32;
                    } else if let Some((ax, ay)) = s.sel_a {
                        let (mut x0, mut x1) = (ax.clamp(0.0, iw), px.clamp(0.0, iw));
                        let (mut y0, mut y1) = (ay.clamp(0.0, ih), py.clamp(0.0, ih));
                        if x0 > x1 {
                            std::mem::swap(&mut x0, &mut x1);
                        }
                        if y0 > y1 {
                            std::mem::swap(&mut y0, &mut y1);
                        }
                        let (x0, y0, x1, y1) = (x0 as i32, y0 as i32, x1.ceil() as i32, y1.ceil() as i32);
                        s.sel = Some((x0, y0, x1 - x0, y1 - y0));
                    }
                }
                None
            }
        };
        if let Some(p) = pick {
            self.pick_at(p);
        }
        self.queue_draw();
    }

    fn drag_end(&self) {
        if self.st().panning {
            self.end_pan();
            self.set_cursor_from_name(Some(self.cursor_name()));
            return;
        }
        let item = self.st().item.take();
        if let Some(it) = item {
            if !(it.kind.is_shape() && it.p0 == it.p1) {
                self.commit(it.bbox(), move |cr| paint::paint_item(cr, &it, true));
            }
        } else {
            match self.tool() {
                Tool::Pick => {
                    if let Some(f) = self.cbs().on_finish_pick {
                        f();
                    }
                }
                Tool::Select => {
                    if self.st().float.is_some() {
                        self.finish_move();
                    } else {
                        let mut s = self.st();
                        if s.sel.is_some_and(|r| r.2 < 2 || r.3 < 2) {
                            s.sel = None;
                        }
                    }
                }
                _ => {}
            }
        }
        self.queue_draw();
    }

    // ---- selection
    fn finish_move(&self) {
        let (f, (x, y, w, h)) = {
            let mut s = self.st();
            let Some(f) = s.float.take() else { return };
            let Some(r) = s.sel else { return };
            (f, r)
        };
        let (nx, ny) = (x + f.dx, y + f.dy);
        if (f.dx, f.dy) == (0, 0) {
            return;
        }
        let surf = f.surf.clone();
        let bg = f.bg;
        self.commit(
            (x.min(nx) as f64, y.min(ny) as f64, (x.max(nx) + w) as f64, (y.max(ny) + h) as f64),
            move |cr| {
                if let Some(b) = bg {
                    cr.set_source_rgb(b.0, b.1, b.2);
                    cr.rectangle(x as f64, y as f64, w as f64, h as f64);
                    let _ = cr.fill();
                }
                let _ = cr.set_source_surface(&surf, nx as f64, ny as f64);
                let _ = cr.paint();
            },
        );
        self.st().sel = Some((nx, ny, w, h));
    }

    pub fn delete_selection(&self) {
        let Some((x, y, w, h)) = self.st().sel else { return };
        let bg = self.st().ring_color(x, y, w, h);
        self.commit((x as f64, y as f64, (x + w) as f64, (y + h) as f64), move |cr| {
            cr.set_source_rgb(bg.0, bg.1, bg.2);
            cr.rectangle(x as f64, y as f64, w as f64, h as f64);
            let _ = cr.fill();
        });
        self.st().sel = None;
        self.queue_draw();
    }

    pub fn select_all(&self) {
        {
            let mut s = self.st();
            s.sel = Some((0, 0, s.w, s.h));
        }
        self.queue_draw();
    }

    /// The selection, or the whole image, as an independent surface.
    pub fn copy_surface(&self) -> ImageSurface {
        let s = self.st();
        if let Some((x, y, w, h)) = s.sel {
            let (x0, y0, x1, y1) = (x.max(0), y.max(0), (x + w).min(s.w), (y + h).min(s.h));
            if x1 > x0 && y1 > y0 {
                return s.region_surface(x0, y0, x1 - x0, y1 - y0);
            }
        }
        s.region_surface(0, 0, s.w, s.h)
    }

    pub fn write_png(&self, path: &std::path::Path) -> Result<(), String> {
        let s = self.st();
        let tmp = {
            let mut t = path.as_os_str().to_owned();
            t.push(".tmp");
            std::path::PathBuf::from(t)
        };
        let res = (|| -> Result<(), String> {
            let mut f = std::io::BufWriter::new(std::fs::File::create(&tmp).map_err(|e| e.to_string())?);
            s.surface.write_to_png(&mut f).map_err(|e| e.to_string())?;
            std::io::Write::flush(&mut f).map_err(|e| e.to_string())?;
            std::fs::rename(&tmp, path).map_err(|e| e.to_string())
        })();
        if res.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        res
    }

    // ---- text
    fn start_text(&self, p: (f64, f64)) {
        let overlay = self.st().overlay.clone();
        let Some(overlay) = overlay else { return };
        let tv = gtk::TextView::new();
        tv.add_css_class("shotnote-text");
        tv.set_wrap_mode(gtk::WrapMode::None);
        tv.set_halign(gtk::Align::Start);
        tv.set_valign(gtk::Align::Start);
        let kc = gtk::EventControllerKey::new();
        kc.set_propagation_phase(gtk::PropagationPhase::Capture);
        let w = self.downgrade();
        kc.connect_key_pressed(move |_, key, _, state| {
            let Some(c) = w.upgrade() else { return glib::Propagation::Proceed };
            if matches!(key, gdk::Key::Return | gdk::Key::KP_Enter) && !state.contains(gdk::ModifierType::SHIFT_MASK) {
                c.commit_text();
                return glib::Propagation::Stop;
            }
            if key == gdk::Key::Escape {
                c.close_text();
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        tv.add_controller(kc);
        let w = self.downgrade();
        tv.buffer().connect_changed(move |_| {
            if let Some(c) = w.upgrade() {
                c.place_text();
                c.queue_draw();
            }
        });
        overlay.add_overlay(&tv);
        self.st().text = Some(TextState { tv: tv.clone(), pos: p });
        self.place_text();
        glib::idle_add_local_once(move || {
            tv.grab_focus();
        });
    }

    fn place_text(&self) {
        let (tv, pos, p, ox, oy, scale) = {
            let s = self.st();
            let Some(t) = &s.text else { return };
            (t.tv.clone(), t.pos, s.params.clone(), s.ox, s.oy, s.scale)
        };
        let (tw, th) = paint::text_extent(&text_of(&tv), &p.font_family, p.font_size, p.bold);
        tv.set_margin_start((ox + pos.0 * scale) as i32);
        tv.set_margin_top((oy + pos.1 * scale) as i32);
        tv.set_size_request((tw as f64 * scale).ceil() as i32 + 4, (th as f64 * scale).ceil() as i32);
        let c = crate::logic::palette::hexcol(p.color);
        let fam = p.font_family.replace('"', "");
        let css = format!(
            ".shotnote-text, .shotnote-text text {{ background: transparent; color: transparent; \
             caret-color: {c}; font-family: \"{fam}\"; font-size: {:.2}px; font-weight: {}; padding: 0; \
             margin: 0; border: none; min-height: 0; }} \
             .shotnote-text text selection {{ background: alpha(#3584e4, 0.35); }}",
            p.font_size as f64 * scale,
            if p.bold { 700 } else { 400 }
        );
        TEXT_CSS.with(|prov| prov.load_from_string(&css));
    }

    pub fn close_text(&self) {
        let (t, overlay) = {
            let mut s = self.st();
            (s.text.take(), s.overlay.clone())
        };
        if let (Some(t), Some(o)) = (t, overlay) {
            glib::idle_add_local_once(move || o.remove_overlay(&t.tv));
        }
        self.queue_draw();
        self.grab_focus();
    }

    pub fn commit_text(&self) {
        let (text, pos, p) = {
            let s = self.st();
            let Some(t) = &s.text else { return };
            (text_of(&t.tv), t.pos, s.params.clone())
        };
        self.close_text();
        if text.trim().is_empty() {
            return;
        }
        let (lw, lh) = paint::text_extent(&text, &p.font_family, p.font_size, p.bold);
        self.commit((pos.0 - 3.0, pos.1 - 3.0, pos.0 + lw as f64 + 6.0, pos.1 + lh as f64 + 6.0), move |cr| {
            cr.set_source_rgb(p.color.0, p.color.1, p.color.2);
            cr.move_to(pos.0, pos.1);
            let layout = paint::make_layout(cr, &text, &p.font_family, p.font_size as f64, p.bold);
            pangocairo::functions::show_layout(cr, &layout);
        });
    }
}

fn in_rect(r: (i32, i32, i32, i32), p: (f64, f64)) -> bool {
    p.0 >= r.0 as f64 && p.0 < (r.0 + r.2) as f64 && p.1 >= r.1 as f64 && p.1 < (r.1 + r.3) as f64
}

fn text_of(tv: &gtk::TextView) -> String {
    let b = tv.buffer();
    b.text(&b.start_iter(), &b.end_iter(), false).to_string()
}
