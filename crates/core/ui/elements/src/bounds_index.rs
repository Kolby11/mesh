//! Exact child unions with bounded point updates, including shrinking extrema.

pub type Bounds = (f32, f32, f32, f32);

#[derive(Debug)]
pub struct ChildBoundsIndex {
    len: usize,
    base: usize,
    bounds: Vec<Option<Bounds>>,
}

impl ChildBoundsIndex {
    pub fn new(children: Vec<Option<Bounds>>) -> Self {
        let len = children.len();
        if len == 0 {
            return Self {
                len: 0,
                base: 0,
                bounds: Vec::new(),
            };
        }
        for _ in &children {
            crate::frame_work::visit(crate::frame_work::Pass::BoundsIndex);
        }
        let base = len.max(1).next_power_of_two();
        let mut bounds = vec![None; base * 2];
        bounds[base..base + len].copy_from_slice(&children);
        for index in (1..base).rev() {
            crate::frame_work::visit(crate::frame_work::Pass::BoundsIndex);
            bounds[index] = union(bounds[index * 2], bounds[index * 2 + 1]);
        }
        Self { len, base, bounds }
    }

    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn bounds(&self) -> Option<Bounds> {
        crate::frame_work::visit(crate::frame_work::Pass::BoundsIndex);
        if self.len == 0 { None } else { self.bounds[1] }
    }

    pub fn set(&mut self, child: usize, bounds: Option<Bounds>) {
        assert!(child < self.len);
        crate::frame_work::visit(crate::frame_work::Pass::BoundsIndex);
        let mut index = self.base + child;
        if self.bounds[index] == bounds {
            return;
        }
        self.bounds[index] = bounds;
        while index > 1 {
            crate::frame_work::visit(crate::frame_work::Pass::BoundsIndex);
            index /= 2;
            let next = union(self.bounds[index * 2], self.bounds[index * 2 + 1]);
            if self.bounds[index] == next {
                break;
            }
            self.bounds[index] = next;
        }
    }
}

pub fn union(a: Option<Bounds>, b: Option<Bounds>) -> Option<Bounds> {
    match (a, b) {
        (Some(a), Some(b)) => Some((a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3))),
        (a, b) => a.or(b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn child_unions_preserve_ties_and_shrinking_extrema() {
        for count in [1, 3, 16, 1024] {
            let mut leaves = vec![Some((-2.5, -1.5, 12.25, 20.75)); count];
            let mut index = ChildBoundsIndex::new(leaves.clone());
            for child in [0, count / 2, count - 1] {
                for next in [
                    Some((-5.25, -3.75, 100.5, 200.25)),
                    None,
                    Some((0.0, 0.0, 3.5, 4.25)),
                ] {
                    leaves[child] = next;
                    let scope = crate::frame_work::Scope::new(true);
                    index.set(child, next);
                    let counts = scope.snapshot();
                    drop(scope);
                    assert_eq!(index.bounds(), leaves.iter().copied().fold(None, union));
                    assert!(counts["bounds_index"] <= count.next_power_of_two().ilog2() as u64 + 1);
                }
            }
        }
        assert_eq!(ChildBoundsIndex::new(Vec::new()).bounds(), None);
    }
}
