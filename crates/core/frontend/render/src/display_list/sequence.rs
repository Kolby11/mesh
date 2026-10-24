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

#[derive(Debug, Clone)]
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
        Self::balanced(parts, ends, len)
    }

    fn balanced(parts: Vec<PaintPart<T>>, ends: Vec<usize>, len: usize) -> Self {
        const FANOUT: usize = 16;
        if parts.len() <= FANOUT {
            return Self {
                parts,
                ends,
                len,
                flat: OnceLock::new(),
            };
        }
        let mut groups = Vec::new();
        let mut group_ends = Vec::new();
        let mut total = 0;
        let mut parts = parts.into_iter();
        loop {
            let chunk: Vec<_> = parts.by_ref().take(FANOUT).collect();
            if chunk.is_empty() {
                break;
            }
            let mut chunk_ends = Vec::with_capacity(chunk.len());
            let mut chunk_len = 0;
            for part in &chunk {
                chunk_len += match part {
                    PaintPart::Leaf(items) => items.len(),
                    PaintPart::Branch(child) => child.len(),
                };
                chunk_ends.push(chunk_len);
            }
            total += chunk_len;
            groups.push(PaintPart::Branch(Arc::new(Self {
                parts: chunk,
                ends: chunk_ends,
                len: chunk_len,
                flat: OnceLock::new(),
            })));
            group_ends.push(total);
        }
        Self::balanced(groups, group_ends, len)
    }
}
impl<T: Clone> PaintSequence<T> {
    /// Copy only the bounded command-storage path; older frame snapshots retain
    /// their original leaves and repeated replacements do not grow the tree.
    pub(super) fn replaced(&self, index: usize, value: T) -> Option<Self> {
        if index >= self.len {
            return None;
        }
        let part = self.ends.partition_point(|end| *end <= index);
        let offset = if part == 0 { 0 } else { self.ends[part - 1] };
        let mut parts = self.parts.clone();
        match &mut parts[part] {
            PaintPart::Leaf(items) => items[index - offset] = value,
            PaintPart::Branch(child) => *child = Arc::new(child.replaced(index - offset, value)?),
        }
        Some(Self {
            parts,
            ends: self.ends.clone(),
            len: self.len,
            flat: OnceLock::new(),
        })
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacements_share_clean_branches_and_preserve_old_snapshots() {
        let children = (0..4096)
            .map(|index| (0, Arc::new(PaintSequence::from(vec![index]))))
            .collect();
        let original = Arc::new(PaintSequence::assemble(Vec::new(), children));
        let mut current = Arc::clone(&original);
        for turn in 0..128 {
            let index = turn * 31 % 1024;
            current = Arc::new(current.replaced(index, 10_000 + turn).unwrap());
            assert_eq!(current[index], 10_000 + turn);
            assert_eq!(original[index], index);
        }
        assert!(current.replaced(current.len(), 0).is_none());
        assert!(original.parts.len() <= 16);
        assert_eq!(current.parts.len(), original.parts.len());
        let shared = current.parts.iter().zip(&original.parts).filter(|(a, b)| {
            matches!((a, b), (PaintPart::Branch(a), PaintPart::Branch(b)) if Arc::ptr_eq(a, b))
        }).count();
        assert!(shared > 0);
        let flattened: Vec<_> = current.iter().copied().collect();
        assert_eq!(flattened.len(), original.len());
        for (index, item) in flattened.into_iter().enumerate() {
            assert_eq!(current[index], item);
        }
    }
}
