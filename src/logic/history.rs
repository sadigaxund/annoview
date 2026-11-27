//! Undo and redo stacks that keep changed pixel rectangles, plus whole-image transforms.

use super::pixels::{write_region, Xform};

pub const MAX_UNDO_BYTES: usize = 1_200_000_000;

pub struct Patch {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub before: Vec<u8>,
    pub after: Vec<u8>,
}

impl Patch {
    pub fn bytes(&self) -> usize {
        self.before.len() + self.after.len()
    }

    /// Writes the before or after pixels back into a strided buffer.
    pub fn apply(&self, buf: &mut [u8], stride: usize, after: bool) {
        let src = if after { &self.after } else { &self.before };
        write_region(buf, stride, self.x, self.y, self.w, self.h, src);
    }
}

pub enum Op {
    Patch(Patch),
    Xform(Xform),
}

impl Op {
    fn bytes(&self) -> usize {
        match self {
            Op::Patch(p) => p.bytes(),
            Op::Xform(_) => 0,
        }
    }
}

pub struct History {
    undo: Vec<Op>,
    redo: Vec<Op>,
    bytes: usize,
    limit: usize,
}

impl History {
    pub fn new(limit: usize) -> Self {
        History { undo: Vec::new(), redo: Vec::new(), bytes: 0, limit }
    }

    pub fn push(&mut self, op: Op) {
        self.bytes += op.bytes();
        self.undo.push(op);
        while self.bytes > self.limit && self.undo.len() > 1 {
            let old = self.undo.remove(0);
            self.bytes -= old.bytes();
        }
        self.redo.clear();
    }

    /// Moves the newest edit to the redo stack and returns it so the caller can revert it.
    pub fn step_back(&mut self) -> Option<&Op> {
        let op = self.undo.pop()?;
        self.redo.push(op);
        self.redo.last()
    }

    /// Moves the newest undone edit back and returns it so the caller can reapply it.
    pub fn step_forward(&mut self) -> Option<&Op> {
        let op = self.redo.pop()?;
        self.undo.push(op);
        self.undo.last()
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    #[cfg(test)]
    pub fn bytes(&self) -> usize {
        self.bytes
    }

    #[cfg(test)]
    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.bytes = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logic::pixels::read_region;

    fn patch(buf: &mut [u8], stride: usize, x: i32, y: i32, w: i32, h: i32, fill: u8) -> Patch {
        let before = read_region(buf, stride, x, y, w, h);
        let after = vec![fill; before.len()];
        write_region(buf, stride, x, y, w, h, &after);
        Patch { x, y, w, h, before, after }
    }

    #[test]
    fn undo_and_redo_restore_pixels() {
        let mut buf = vec![7u8; 8 * 8 * 4];
        let orig = buf.clone();
        let mut h = History::new(MAX_UNDO_BYTES);
        h.push(Op::Patch(patch(&mut buf, 32, 2, 2, 3, 3, 200)));
        let edited = buf.clone();
        assert_ne!(edited, orig);
        if let Some(Op::Patch(p)) = h.step_back() {
            p.apply(&mut buf, 32, false);
        }
        assert_eq!(buf, orig);
        assert!(h.can_redo() && !h.can_undo());
        if let Some(Op::Patch(p)) = h.step_forward() {
            p.apply(&mut buf, 32, true);
        }
        assert_eq!(buf, edited);
    }

    #[test]
    fn only_the_region_is_stored() {
        let mut buf = vec![0u8; 100 * 100 * 4];
        let p = patch(&mut buf, 400, 10, 10, 5, 4, 1);
        assert_eq!(p.bytes(), 5 * 4 * 4 * 2);
    }

    #[test]
    fn new_edit_clears_redo() {
        let mut buf = vec![0u8; 4 * 4 * 4];
        let mut h = History::new(MAX_UNDO_BYTES);
        h.push(Op::Patch(patch(&mut buf, 16, 0, 0, 1, 1, 9)));
        h.step_back();
        assert!(h.can_redo());
        h.push(Op::Patch(patch(&mut buf, 16, 1, 1, 1, 1, 9)));
        assert!(!h.can_redo());
    }

    #[test]
    fn limit_drops_oldest_but_keeps_newest() {
        let mut buf = vec![0u8; 16 * 16 * 4];
        let mut h = History::new(100);
        for i in 0..5 {
            h.push(Op::Patch(patch(&mut buf, 64, 0, 0, 4, 4, i + 1)));
        }
        assert_eq!(h.undo_len(), 1);
        assert_eq!(h.bytes(), 128);
        let mut h = History::new(300);
        for i in 0..5 {
            h.push(Op::Patch(patch(&mut buf, 64, 0, 0, 4, 4, i + 1)));
        }
        assert_eq!(h.undo_len(), 2);
    }

    #[test]
    fn transforms_are_free_to_store() {
        let mut h = History::new(10);
        h.push(Op::Xform(Xform::FlipH));
        h.push(Op::Xform(Xform::RotLeft));
        assert_eq!(h.undo_len(), 2);
        assert_eq!(h.bytes(), 0);
    }
}
