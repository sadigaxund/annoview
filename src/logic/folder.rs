//! Image listing and next/previous order inside one folder.

use std::cmp::Ordering;
use std::path::{Path, PathBuf};

pub const EXTENSIONS: [&str; 8] = ["png", "jpg", "jpeg", "webp", "gif", "bmp", "tif", "tiff"];

pub fn is_image(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

fn chunks(s: &str) -> Vec<(bool, String)> {
    let mut out: Vec<(bool, String)> = Vec::new();
    for c in s.chars() {
        let d = c.is_ascii_digit();
        match out.last_mut() {
            Some((kind, text)) if *kind == d => text.push(c),
            _ => out.push((d, c.to_string())),
        }
    }
    out
}

/// Case-insensitive order where digit runs compare as numbers.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (ca, cb) = (chunks(&a.to_lowercase()), chunks(&b.to_lowercase()));
    for (x, y) in ca.iter().zip(cb.iter()) {
        let o = if x.0 && y.0 {
            let (tx, ty) = (x.1.trim_start_matches('0'), y.1.trim_start_matches('0'));
            tx.len().cmp(&ty.len()).then_with(|| tx.cmp(ty))
        } else {
            x.1.cmp(&y.1)
        };
        if o != Ordering::Equal {
            return o;
        }
    }
    ca.len().cmp(&cb.len()).then_with(|| a.cmp(b))
}

/// Image files of the folder holding `file`, in natural name order.
pub fn list_images(file: &Path) -> Vec<PathBuf> {
    let dir = file.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.is_file() && is_image(p)).collect())
        .unwrap_or_default();
    sort_natural(&mut v);
    v
}

pub fn sort_natural(v: &mut [PathBuf]) {
    let stem = |p: &PathBuf| p.file_stem().unwrap_or_default().to_string_lossy().into_owned();
    let name = |p: &PathBuf| p.file_name().unwrap_or_default().to_string_lossy().into_owned();
    v.sort_by(|a, b| natural_cmp(&stem(a), &stem(b)).then_with(|| natural_cmp(&name(a), &name(b))));
}

/// The file `step` places after `cur` (negative is before), wrapping at the ends.
/// None when the folder holds no other image.
pub fn neighbor(list: &[PathBuf], cur: &Path, step: i64) -> Option<PathBuf> {
    let others = list.iter().filter(|p| p.as_path() != cur).count();
    if others == 0 {
        return None;
    }
    let n = list.len() as i64;
    let next = match list.iter().position(|p| p == cur) {
        Some(i) => (i as i64 + step).rem_euclid(n),
        None if step > 0 => 0,
        None => n - 1,
    };
    Some(list[next as usize].clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(v: &[&str]) -> Vec<PathBuf> {
        v.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn natural_order() {
        let mut v = paths(&["/d/img10.png", "/d/img2.png", "/d/IMG1.png", "/d/a.png"]);
        sort_natural(&mut v);
        assert_eq!(v, paths(&["/d/a.png", "/d/IMG1.png", "/d/img2.png", "/d/img10.png"]));
    }

    #[test]
    fn timestamp_names_sort_by_time() {
        let mut v = paths(&[
            "/d/Screenshot from 2026-11-11 05-25-20.png",
            "/d/Screenshot from 2026-11-11 05-21-27.png",
            "/d/Screenshot from 2026-11-11 05-21-27 (2).png",
        ]);
        sort_natural(&mut v);
        assert_eq!(v[0], PathBuf::from("/d/Screenshot from 2026-11-11 05-21-27.png"));
        assert_eq!(v[2], PathBuf::from("/d/Screenshot from 2026-11-11 05-25-20.png"));
    }

    #[test]
    fn neighbors_wrap() {
        let l = paths(&["/d/a.png", "/d/b.png", "/d/c.png"]);
        assert_eq!(neighbor(&l, Path::new("/d/a.png"), 1), Some(PathBuf::from("/d/b.png")));
        assert_eq!(neighbor(&l, Path::new("/d/c.png"), 1), Some(PathBuf::from("/d/a.png")));
        assert_eq!(neighbor(&l, Path::new("/d/a.png"), -1), Some(PathBuf::from("/d/c.png")));
    }

    #[test]
    fn lone_image_has_no_neighbor() {
        let l = paths(&["/d/a.png"]);
        assert_eq!(neighbor(&l, Path::new("/d/a.png"), 1), None);
        assert_eq!(neighbor(&[], Path::new("/d/a.png"), 1), None);
    }

    #[test]
    fn unlisted_current_starts_at_an_end() {
        let l = paths(&["/d/a.png", "/d/b.png"]);
        assert_eq!(neighbor(&l, Path::new("/d/z.txt"), 1), Some(PathBuf::from("/d/a.png")));
        assert_eq!(neighbor(&l, Path::new("/d/z.txt"), -1), Some(PathBuf::from("/d/b.png")));
    }

    #[test]
    fn extension_filter() {
        assert!(is_image(Path::new("/d/a.PNG")));
        assert!(is_image(Path::new("/d/a.webp")));
        assert!(!is_image(Path::new("/d/a.txt")));
        assert!(!is_image(Path::new("/d/png")));
    }
}
