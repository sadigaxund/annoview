//! Shotnote: screenshot annotator and image viewer for GTK4.

mod app;
mod canvas;
mod capture;
mod logic;
mod paint;
#[cfg(feature = "selftest")]
mod selftest;
mod widgets;
mod window;

use adw::prelude::*;
use gtk::{gio, glib};
use std::io::Read;
use std::sync::OnceLock;
use std::time::Instant;

pub const APP_ID: &str = "dev.local.Shotnote";
pub static START: OnceLock<Instant> = OnceLock::new();

/// Prints a stage time to stderr when `SHOTNOTE_TIMING=2` is set.
pub fn lap(stage: &str) {
    if std::env::var_os("SHOTNOTE_TIMING").is_some_and(|v| v == "2") {
        let ms = START.get().map_or(0.0, |s| s.elapsed().as_secs_f64() * 1000.0);
        eprintln!("{ms:8.1} ms  {stage}");
    }
}

fn main() -> glib::ExitCode {
    let _ = START.set(Instant::now());
    let mut args: Vec<String> = std::env::args().collect();
    let mut stdin_data: Option<Vec<u8>> = None;
    if let Some(i) = args.iter().skip(1).position(|a| a == "-") {
        args.remove(i + 1);
        let mut buf = Vec::new();
        let _ = std::io::stdin().lock().read_to_end(&mut buf);
        stdin_data = Some(buf);
    }
    let application = adw::Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::HANDLES_OPEN | gio::ApplicationFlags::NON_UNIQUE)
        .build();
    let ctx = app::Ctx::new(application.clone());
    application.connect_startup(|_| {
        lap("startup");
        app::install_css();
    });
    let c = ctx.clone();
    application.connect_activate(move |_| match stdin_data.clone() {
        Some(d) => c.open_stdin(d),
        None => c.capture(None),
    });
    let c = ctx.clone();
    application.connect_open(move |_, files, _| {
        for f in files {
            if let Some(p) = f.path() {
                c.open_path(&p);
            }
        }
    });
    application.run_with_args(&args)
}
