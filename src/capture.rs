//! Screenshot capture: the xdg portal on GNOME, grim and slurp on Hyprland and as a fallback.

use gtk::gio::{self, prelude::*};
use gtk::glib;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

pub fn screenshots_dir() -> PathBuf {
    let pics = glib::user_special_dir(glib::UserDirectory::Pictures)
        .or_else(|| glib::home_dir().join("Pictures").into())
        .unwrap_or_else(|| glib::home_dir().join("Pictures"));
    pics.join("Screenshots")
}

pub fn stamp() -> String {
    glib::DateTime::now_local()
        .and_then(|d| d.format("%Y-%m-%d %H-%M-%S"))
        .map(|s| s.to_string())
        .unwrap_or_else(|_| "unknown-time".into())
}

pub fn new_screenshot_path() -> PathBuf {
    let dir = screenshots_dir();
    let _ = std::fs::create_dir_all(&dir);
    crate::logic::naming::numbered(&dir, &crate::logic::naming::screenshot_base(&stamp()), "png", &|p| p.exists())
}

fn ensure_in_screenshots(path: PathBuf) -> PathBuf {
    if path.parent() == Some(screenshots_dir().as_path()) {
        return path;
    }
    let dst = new_screenshot_path();
    match std::fs::copy(&path, &dst) {
        Ok(_) => dst,
        Err(e) => {
            eprintln!("Could not copy {} into the screenshots folder: {e}", path.display());
            path
        }
    }
}

pub fn is_hyprland() -> bool {
    std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|d| d.to_lowercase().contains("hyprland"))
}

type Done = Rc<RefCell<Option<Box<dyn FnOnce(Option<PathBuf>)>>>>;

fn finish(done: &Done, path: Option<PathBuf>) {
    if let Some(f) = done.borrow_mut().take() {
        f(path);
    }
}

/// Starts an interactive capture and calls `done` once with the saved file, or None when cancelled.
pub fn capture(done: Box<dyn FnOnce(Option<PathBuf>)>) {
    let done: Done = Rc::new(RefCell::new(Some(done)));
    if is_hyprland() {
        grim(done);
    } else {
        portal(done);
    }
}

fn grim(done: Done) {
    glib::spawn_future_local(async move {
        let res = gio::spawn_blocking(|| -> Result<Option<PathBuf>, String> {
            let out = std::process::Command::new("slurp").output().map_err(|e| format!("slurp: {e}"))?;
            let geom = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !out.status.success() || geom.is_empty() {
                return Ok(None);
            }
            let path = new_screenshot_path();
            let st = std::process::Command::new("grim")
                .arg("-g")
                .arg(&geom)
                .arg(&path)
                .status()
                .map_err(|e| format!("grim: {e}"))?;
            if st.success() {
                Ok(Some(path))
            } else {
                Err("grim failed".into())
            }
        })
        .await;
        match res {
            Ok(Ok(p)) => finish(&done, p),
            Ok(Err(e)) => {
                eprintln!("Capture failed: {e}");
                finish(&done, None);
            }
            Err(_) => finish(&done, None),
        }
    });
}

fn portal(done: Done) {
    glib::spawn_future_local(async move {
        let bus = match gio::bus_get_future(gio::BusType::Session).await {
            Ok(b) => b,
            Err(e) => {
                eprintln!("Cannot reach portal: {}", e.message());
                return grim(done);
            }
        };
        let sender = bus.unique_name().map(|n| n.trim_start_matches(':').replace('.', "_")).unwrap_or_default();
        let token = format!(
            "annoview{}x{}",
            std::process::id(),
            glib::real_time() / 1000
        );
        let handle = format!("/org/freedesktop/portal/desktop/request/{sender}/{token}");
        let sub: Rc<RefCell<Option<gio::SignalSubscription>>> = Rc::new(RefCell::new(None));
        let (d2, s2) = (done.clone(), sub.clone());
        let subscription = bus.subscribe_to_signal(
            Some("org.freedesktop.portal.Desktop"),
            Some("org.freedesktop.portal.Request"),
            Some("Response"),
            Some(&handle),
            None,
            gio::DBusSignalFlags::NONE,
            move |sig| {
                s2.borrow_mut().take();
                let uri = sig
                    .parameters
                    .get::<(u32, std::collections::HashMap<String, glib::Variant>)>()
                    .and_then(|(code, res)| if code == 0 { res.get("uri").and_then(|u| u.get::<String>()) } else { None });
                let path = uri.and_then(|u| gio::File::for_uri(&u).path());
                finish(&d2, path.map(ensure_in_screenshots));
            },
        );
        *sub.borrow_mut() = Some(subscription);
        let opts = glib::VariantDict::new(None);
        opts.insert_value("interactive", &true.to_variant());
        opts.insert_value("handle_token", &token.to_variant());
        let params = glib::Variant::tuple_from_iter(["".to_variant(), opts.end()]);
        let res = bus
            .call_future(
                Some("org.freedesktop.portal.Desktop"),
                "/org/freedesktop/portal/desktop",
                "org.freedesktop.portal.Screenshot",
                "Screenshot",
                Some(&params),
                Some(glib::VariantTy::new("(o)").expect("type")),
                gio::DBusCallFlags::NONE,
                -1,
            )
            .await;
        if let Err(e) = res {
            eprintln!("Screenshot portal failed: {}", e.message());
            sub.borrow_mut().take();
            grim(done);
        }
    });
}
