//! Persistent paint storage. Branches retain child sequences; only diagnostic
//! slice access materializes a flat copy. Painting walks leaves directly.
use std::sync::{Arc, OnceLock};

#[derive(Debug)]
pub struct PaintSequence<T> {
    parts: Vec<PaintPart<T>>,
    ends: Vec<usize>,
    len: usize,
    flat: OnceLock<Vec<T>>,
}

#[derive(Debug)]
enum PaintPart<T> {
    Leaf(Vec<T>),
    Branch(Arc<PaintSequence<T>>),
}

impl<T> Default for PaintSequence<T> {
    fn default() -> Self {
        Self::from(Vec::new())
    }
}
impl<T> From<Vec<T>> for PaintSequence<T> {
    fn from(items: Vec<T>) -> Self {
        let len = items.len();
        Self {
            parts: vec![PaintPart::Leaf(items)],
            ends: vec![len],
            len,
            flat: OnceLock::new(),
        }
    }
}
impl<T> PaintSequence<T> {
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn first(&self) -> Option<&T> {
        self.get(0)
    }
    pub fn get(&self, index: usize) -> Option<&T> {
        if index >= self.len {
            return None;
        }
        let part = self.ends.partition_point(|end| *end <= index);
        let offset = if part == 0 { 0 } else { self.ends[part - 1] };
        match &self.parts[part] {
            PaintPart::Leaf(items) => items.get(index - offset),
            PaintPart::Branch(child) => child.get(index - offset),
        }
    }
    pub fn iter(&self) -> PaintSequenceIter<'_, T> {
        PaintSequenceIter {
            stack: vec![self.parts.iter()],
            leaf: [].iter(),
        }
    }
    pub(super) fn assemble(local: Vec<T>, children: Vec<(usize, Arc<Self>)>) -> Self {
        let mut local = local.into_iter();
        let mut consumed = 0;
        let mut parts = Vec::with_capacity(children.len() * 2 + 1);
        let mut ends = Vec::with_capacity(children.len() * 2 + 1);
        let mut len = 0;
        for (position, child) in children {
            if position > consumed {
                let leaf: Vec<_> = local.by_ref().take(position - consumed).collect();
                len += leaf.len();
                parts.push(PaintPart::Leaf(leaf));
                ends.push(len);
                consumed = position;
            }
            if !child.is_empty() {
                len += child.len();
                parts.push(PaintPart::Branch(child));
                ends.push(len);
            }
        }
        let leaf: Vec<_> = local.collect();
        if !leaf.is_empty() {
            len += leaf.len();
            parts.push(PaintPart::Leaf(leaf));
            ends.push(len);
        }
        Self {
            parts,
            ends,
            len,
            flat: OnceLock::new(),
        }
    }
}
impl<T: Clone> PaintSequence<T> {
    /// Compatibility for inspection APIs; never used by retained replay.
    pub fn as_slice(&self) -> &[T] {
        self.flat.get_or_init(|| self.iter().cloned().collect())
    }
}
pub struct PaintSequenceIter<'a, T> {
    stack: Vec<std::slice::Iter<'a, PaintPart<T>>>,
    leaf: std::slice::Iter<'a, T>,
}
impl<'a, T> Iterator for PaintSequenceIter<'a, T> {
    type Item = &'a T;
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(item) = self.leaf.next() {
                return Some(item);
            }
            match self.stack.last_mut()?.next() {
                Some(PaintPart::Leaf(items)) => self.leaf = items.iter(),
                Some(PaintPart::Branch(child)) => self.stack.push(child.parts.iter()),
                None => {
                    self.stack.pop();
                }
            }
        }
    }
}
impl<'a, T> IntoIterator for &'a PaintSequence<T> {
    type Item = &'a T;
    type IntoIter = PaintSequenceIter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<T> std::ops::Index<usize> for PaintSequence<T> {
    type Output = T;
    fn index(&self, index: usize) -> &T {
        self.get(index).expect("paint sequence index")
    }
}
