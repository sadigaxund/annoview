//! Application context: settings file, opening images and capture.

use crate::capture;
use crate::logic::settings::Settings;
use crate::window::Win;
use adw::prelude::*;
use gtk::cairo::{Format, ImageSurface};
use gtk::{gdk, glib};
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

pub struct Ctx {
    pub app: adw::Application,
    pub settings: RefCell<Settings>,
    save_pending: Cell<bool>,
    pub timing: bool,
}

pub fn conf_path() -> PathBuf {
    glib::user_config_dir().join("shotnote").join("settings.json")
}

pub fn surface_from_texture(tex: &gdk::Texture) -> Result<ImageSurface, String> {
    let (w, h) = (tex.width(), tex.height());
    let mut dl = gdk::TextureDownloader::new(tex);
    dl.set_format(gdk::MemoryFormat::B8g8r8a8Premultiplied);
    let (bytes, stride) = dl.download_bytes();
    ImageSurface::create_for_data(bytes.to_vec(), Format::ARgb32, w, h, stride as i32).map_err(|e| e.to_string())
}

pub fn surface_from_file(path: &Path) -> Result<ImageSurface, String> {
    let tex = gdk::Texture::from_filename(path).map_err(|e| e.message().to_string())?;
    crate::lap("decoded");
    let s = surface_from_texture(&tex);
    crate::lap("converted");
    s
}

impl Ctx {
    pub fn new(app: adw::Application) -> Rc<Ctx> {
        let text = std::fs::read_to_string(conf_path()).unwrap_or_default();
        Rc::new(Ctx {
            app,
            settings: RefCell::new(Settings::from_json(&text)),
            save_pending: Cell::new(false),
            timing: std::env::var_os("SHOTNOTE_TIMING").is_some_and(|v| v == "1" || v == "2"),
        })
    }

    pub fn save_soon(self: &Rc<Self>) {
        if self.save_pending.replace(true) {
            return;
        }
        let ctx = self.clone();
        glib::timeout_add_local_once(Duration::from_millis(600), move || {
            ctx.save_pending.set(false);
            ctx.save_now();
        });
    }

    pub fn save_now(&self) {
        let p = conf_path();
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(&p, self.settings.borrow().to_json());
    }

    pub fn open_path(self: &Rc<Self>, path: &Path) {
        match surface_from_file(path) {
            Ok(s) => Win::new(self, s, Some(path.to_path_buf())).present(),
            Err(e) => eprintln!("Could not open {}: {e}", path.display()),
        }
    }

    pub fn open_stdin(self: &Rc<Self>, data: Vec<u8>) {
        let tex = gdk::Texture::from_bytes(&glib::Bytes::from_owned(data)).map_err(|e| e.message().to_string());
        match tex.and_then(|t| surface_from_texture(&t)) {
            Ok(s) => Win::new(self, s, None).present(),
            Err(e) => eprintln!("Could not read image from stdin: {e}"),
        }
    }

    /// Captures a screenshot. The origin window hides first and shows again afterwards.
    pub fn capture(self: &Rc<Self>, origin: Option<Rc<Win>>) {
        let hold = self.app.hold();
        if let Some(o) = &origin {
            o.window.set_visible(false);
        }
        let ctx = self.clone();
        let delay = if origin.is_some() { 250 } else { 1 };
        glib::timeout_add_local_once(Duration::from_millis(delay), move || {
            capture::capture(Box::new(move |path| {
                if let Some(o) = origin {
                    o.window.set_visible(true);
                }
                if let Some(p) = path {
                    ctx.open_path(&p);
                }
                drop(hold);
            }));
        });
    }
}

pub fn install_css() {
    let provider = gtk::CssProvider::new();
    provider.load_from_string(
        ".tool-active { background: alpha(@accent_bg_color, 0.32); }\
         .tool-active:hover { background: alpha(@accent_bg_color, 0.45); }\
         .shotnote-toast { background: rgba(30, 30, 30, 0.88); color: white;\
          padding: 6px 16px; border-radius: 999px; font-weight: 600; }",
    );
    if let Some(display) = gdk::Display::default() {
        gtk::style_context_add_provider_for_display(&display, &provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
        crate::canvas::TEXT_CSS.with(|p| {
            gtk::style_context_add_provider_for_display(&display, p, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION)
        });
    }
}
