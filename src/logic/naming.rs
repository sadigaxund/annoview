//! File names for new screenshots and for saved copies.

use std::path::{Path, PathBuf};

/// `Screenshot from <stamp>` where stamp looks like `2026-11-11 05-23-14`.
pub fn screenshot_base(stamp: &str) -> String {
    format!("Screenshot from {stamp}")
}

/// `base.ext`, then `base (2).ext`, `base (3).ext` until `exists` says the name is free.
pub fn numbered(dir: &Path, base: &str, ext: &str, exists: &dyn Fn(&Path) -> bool) -> PathBuf {
    let first = dir.join(format!("{base}.{ext}"));
    if !exists(&first) {
        return first;
    }
    let mut n = 2;
    loop {
        let p = dir.join(format!("{base} ({n}).{ext}"));
        if !exists(&p) {
            return p;
        }
        n += 1;
    }
}

fn strip_copy_suffix(stem: &str) -> &str {
    if let Some(open) = stem.rfind(" (") {
        let tail = &stem[open + 2..];
        if let Some(num) = tail.strip_suffix(')') {
            if !num.is_empty() && num.bytes().all(|b| b.is_ascii_digit()) {
                return &stem[..open];
            }
        }
    }
    stem
}

/// The sibling copy of `src`: `<stem> (2).png`, or the next free number. Always PNG.
pub fn sibling_copy(src: &Path, exists: &dyn Fn(&Path) -> bool) -> PathBuf {
    let dir = src.parent().unwrap_or_else(|| Path::new("."));
    let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("image");
    let base = strip_copy_suffix(stem);
    let mut n = 2;
    loop {
        let p = dir.join(format!("{base} ({n}).png"));
        if !exists(&p) {
            return p;
        }
        n += 1;
    }
}

/// Where the next Ctrl+S writes. `own` is a file this window created, which may be overwritten.
pub fn save_target(
    path: Option<&Path>,
    own: bool,
    screenshots: &Path,
    stamp: &str,
    exists: &dyn Fn(&Path) -> bool,
) -> PathBuf {
    match path {
        Some(p) if own => p.to_path_buf(),
        Some(p) => sibling_copy(p, exists),
        None => numbered(screenshots, &screenshot_base(stamp), "png", exists),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn taken(names: &[&str]) -> impl Fn(&Path) -> bool {
        let set: HashSet<PathBuf> = names.iter().map(PathBuf::from).collect();
        move |p| set.contains(p)
    }

    #[test]
    fn screenshot_names_get_numbers() {
        let d = Path::new("/s");
        let base = screenshot_base("2026-11-11 05-23-14");
        assert_eq!(numbered(d, &base, "png", &taken(&[])), PathBuf::from("/s/Screenshot from 2026-11-11 05-23-14.png"));
        let t = taken(&["/s/Screenshot from 2026-11-11 05-23-14.png"]);
        assert_eq!(numbered(d, &base, "png", &t), PathBuf::from("/s/Screenshot from 2026-11-11 05-23-14 (2).png"));
        let t = taken(&[
            "/s/Screenshot from 2026-11-11 05-23-14.png",
            "/s/Screenshot from 2026-11-11 05-23-14 (2).png",
        ]);
        assert_eq!(numbered(d, &base, "png", &t), PathBuf::from("/s/Screenshot from 2026-11-11 05-23-14 (3).png"));
    }

    #[test]
    fn sibling_copy_next_to_original() {
        let t = taken(&["/p/a.png"]);
        assert_eq!(sibling_copy(Path::new("/p/a.png"), &t), PathBuf::from("/p/a (2).png"));
        let t = taken(&["/p/a.png", "/p/a (2).png"]);
        assert_eq!(sibling_copy(Path::new("/p/a.png"), &t), PathBuf::from("/p/a (3).png"));
    }

    #[test]
    fn sibling_copy_of_a_copy_does_not_stack_suffixes() {
        let t = taken(&["/p/a.png", "/p/a (2).png"]);
        assert_eq!(sibling_copy(Path::new("/p/a (2).png"), &t), PathBuf::from("/p/a (3).png"));
    }

    #[test]
    fn sibling_copy_of_jpeg_is_png() {
        assert_eq!(sibling_copy(Path::new("/p/photo.jpg"), &taken(&[])), PathBuf::from("/p/photo (2).png"));
    }

    #[test]
    fn save_target_rules() {
        let none = taken(&[]);
        let shots = Path::new("/shots");
        let orig = Path::new("/p/a.png");
        // An original is never the target.
        let t = taken(&["/p/a.png"]);
        assert_eq!(save_target(Some(orig), false, shots, "x", &t), PathBuf::from("/p/a (2).png"));
        // A copy made by this window is overwritten.
        let copy = Path::new("/p/a (2).png");
        let t = taken(&["/p/a.png", "/p/a (2).png"]);
        assert_eq!(save_target(Some(copy), true, shots, "x", &t), PathBuf::from("/p/a (2).png"));
        // Stdin images go to the screenshots folder.
        assert_eq!(
            save_target(None, false, shots, "2026-01-02 03-04-05", &none),
            PathBuf::from("/shots/Screenshot from 2026-01-02 03-04-05.png")
        );
    }
}
