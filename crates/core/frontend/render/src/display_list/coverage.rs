//! Persistent child unions: sparse repairs share every unaffected branch.
use std::sync::Arc;

use mesh_core_elements::{NodeId, frame_work};

use super::types::DamageRect;

#[derive(Debug, Clone, Default)]
pub(super) struct ChildCoverage(Option<Arc<CoverageNode>>);

#[derive(Debug, Clone)]
struct CoverageNode {
    last: NodeId,
    bounds: DamageRect,
    children: Option<(Arc<CoverageNode>, Arc<CoverageNode>)>,
}

impl ChildCoverage {
    pub(super) fn new(mut children: Vec<(NodeId, DamageRect)>) -> Self {
        fn build(children: &[(NodeId, DamageRect)]) -> Arc<CoverageNode> {
            let last = children.last().unwrap().0;
            if children.len() == 1 {
                return Arc::new(CoverageNode {
                    last,
                    bounds: children[0].1,
                    children: None,
                });
            }
            let middle = children.len() / 2;
            let (left, right) = (build(&children[..middle]), build(&children[middle..]));
            Arc::new(CoverageNode {
                last,
                bounds: left.bounds.union(right.bounds),
                children: Some((left, right)),
            })
        }
        children.sort_unstable_by_key(|child| child.0);
        Self((!children.is_empty()).then(|| build(&children)))
    }

    pub(super) fn bounds(&self) -> DamageRect {
        self.0
            .as_ref()
            .map_or(DamageRect::default(), |root| root.bounds)
    }

    pub(super) fn set(&mut self, id: NodeId, bounds: DamageRect) -> Option<()> {
        fn replace(node: &mut Arc<CoverageNode>, id: NodeId, bounds: DamageRect) -> Option<()> {
            frame_work::visit(frame_work::Pass::BoundsIndex);
            if node.children.is_some() {
                let node = Arc::make_mut(node);
                let (left, right) = node.children.as_mut()?;
                replace(if id <= left.last { left } else { right }, id, bounds)?;
                node.bounds = left.bounds.union(right.bounds);
            } else {
                if node.last != id {
                    return None;
                }
                if node.bounds != bounds {
                    Arc::make_mut(node).bounds = bounds;
                }
            }
            Some(())
        }
        replace(self.0.as_mut()?, id, bounds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn child_coverage_preserves_snapshots_and_shrinking_extrema() {
        for count in [1, 3, 16, 1024] {
            let rect = DamageRect {
                x: 10,
                y: 10,
                width: 20,
                height: 20,
            };
            let mut children: Vec<_> = (0..count).rev().map(|i| (i as NodeId * 7, rect)).collect();
            let mut index = ChildCoverage::new(children.clone());
            let original = index.clone();
            for child in [0, count / 2, count - 1] {
                for next in [
                    DamageRect {
                        x: 0,
                        y: 2,
                        width: 100,
                        height: 200,
                    },
                    DamageRect::default(),
                    DamageRect {
                        x: 12,
                        y: 15,
                        width: 2,
                        height: 3,
                    },
                ] {
                    children[child].1 = next;
                    let scope = frame_work::Scope::new(true);
                    index.set(children[child].0, next).unwrap();
                    let work = scope.snapshot();
                    drop(scope);
                    assert_eq!(
                        index.bounds(),
                        children
                            .iter()
                            .fold(DamageRect::default(), |a, b| a.union(b.1))
                    );
                    assert_eq!(original.bounds(), rect);
                    assert!(
                        work["bounds_index"]
                            <= (count as usize).next_power_of_two().ilog2() as u64 + 1
                    );
                }
            }
            assert!(index.set(99999, rect).is_none());
        }
        assert_eq!(ChildCoverage::default().bounds(), DamageRect::default());
    }
}
