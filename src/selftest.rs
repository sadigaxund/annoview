//! Scripted self-test driver, built only with the `selftest` feature.

use crate::logic::palette::parse_hex;
use crate::paint::Tool;
use crate::window::{dump_window, Win};
use gtk::{gdk, glib};
use std::rc::Rc;
use std::time::Duration;

fn parse_pts(args: &[&str]) -> Vec<(f64, f64)> {
    args.iter()
        .filter_map(|a| {
            let (x, y) = a.split_once(',')?;
            Some((x.parse().ok()?, y.parse().ok()?))
        })
        .collect()
}

fn key_state(spec: &str) -> (gdk::Key, gdk::ModifierType) {
    let mut st = gdk::ModifierType::empty();
    let mut name = spec;
    while let Some((m, rest)) = name.split_once('+') {
        match m {
            "ctrl" => st |= gdk::ModifierType::CONTROL_MASK,
            "shift" => st |= gdk::ModifierType::SHIFT_MASK,
            _ => break,
        }
        name = rest;
    }
    (gdk::Key::from_name(name).unwrap_or_else(|| panic!("unknown key {name}")), st)
}

fn exec(win: &Rc<Win>, line: &str) -> u64 {
    let parts: Vec<&str> = line.split_whitespace().collect();
    let c = &win.canvas;
    match parts.first().copied() {
        Some("wait") => return parts[1].parse().unwrap_or(0),
        Some("tool") => {
            let t = match parts[1] {
                "pen" => Tool::Pen,
                "hl" => Tool::Hl,
                "text" => Tool::Text,
                "select" => Tool::Select,
                "line" => Tool::Line,
                "ellipse" => Tool::Ellipse,
                "tri" => Tool::Tri,
                "pick" => Tool::Pick,
                _ => Tool::Rect,
            };
            win.choose_tool(t);
        }
        Some("drag") => {
            let ctrl = parts.contains(&"ctrl");
            let shift = parts.contains(&"shift");
            c.test_drag(&parse_pts(&parts[1..]), ctrl, shift);
        }
        Some("text") => {
            let p = parse_pts(&parts[1..2])[0];
            c.test_text(p, &parts[2..].join(" ").replace("\\n", "\n"));
        }
        Some("key") => {
            let (k, st) = key_state(parts[1]);
            win.on_key(k, st);
        }
        Some("color") => {
            if let Some(rgb) = parse_hex(parts[1]) {
                win.set_color(rgb);
            }
        }
        Some("size") => {
            let v: i64 = parts[2].parse().unwrap_or(5);
            win.set_size(parts[1], v);
        }
        Some("gshot") => {
            let _ = std::process::Command::new("grim").arg(parts[1]).status();
        }
        Some("shot") => dump_window(&win.window, parts[1]),
        Some("popover") => win.test_popover(parts[1]),
        Some("commit") => c.commit_text(),
        Some("quit") => std::process::exit(0),
        _ => eprintln!("selftest: unknown command {line}"),
    }
    0
}

pub fn run(win: Rc<Win>, script: String) {
    let cmds: Rc<Vec<String>> =
        Rc::new(script.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).map(String::from).collect());
    step(win, cmds, 0);
}

fn step(win: Rc<Win>, cmds: Rc<Vec<String>>, i: usize) {
    if i >= cmds.len() {
        return;
    }
    let extra = exec(&win, &cmds[i]);
    glib::timeout_add_local_once(Duration::from_millis(250 + extra), move || step(win, cmds, i + 1));
}
