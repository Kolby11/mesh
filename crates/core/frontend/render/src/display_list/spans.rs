use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use mesh_core_elements::{NodeId, WidgetNode, frame_work};

use super::subtree::*;
use super::types::*;

pub(super) fn build_command_spans(
    root: &WidgetNode,
    subtrees: &HashMap<NodeId, Arc<RetainedPaintSubtree>>,
) -> Arc<PaintSpanSequence> {
    subtrees
        .get(&root.id)
        .map(|subtree| Arc::clone(&subtree.spans))
        .unwrap_or_else(|| Arc::new(PaintSpanSequence::default()))
}

pub(super) fn insert_selected_command_span(
    spans: &mut Vec<SelectedCommandSpan>,
    mut next: SelectedCommandSpan,
) {
    if next.start >= next.end {
        return;
    }
    let last_end_start = spans.last().map(|span| (span.start, span.end));
    let Some((last_start, last_end)) = last_end_start else {
        spans.push(next);
        return;
    };
    if next.start >= last_start {
        if next.start <= last_end {
            if let Some(last) = spans.last_mut() {
                last.end = last.end.max(next.end);
            }
            return;
        }
        spans.push(next);
        return;
    }

    let insert_index = spans.partition_point(|span| span.end < next.start);
    let mut next_index = insert_index;
    while next_index < spans.len() && spans[next_index].start <= next.end {
        let span = spans[next_index];
        next.start = next.start.min(span.start);
        next.end = next.end.max(span.end);
        next_index = next_index.saturating_add(1);
    }
    spans.drain(insert_index..next_index);
    spans.insert(insert_index, next);
}

pub(super) fn rects_intersect_any(bounds: DamageRect, rects: &[DamageRect]) -> bool {
    rects.iter().copied().any(|rect| bounds.intersects(rect))
}

/// Persistent replay spans in command order. Child branches retain their local
/// command offsets; inspection can materialize a contiguous slice on demand.
#[derive(Debug, Default)]
pub struct PaintSpanSequence {
    root: Option<SpanLink>,
    flat: OnceLock<Vec<FramePaintReplaySpan>>,
}

#[derive(Debug, Clone)]
pub(super) struct SpanLink {
    offset: usize,
    node: Arc<SpanNode>,
}

#[derive(Debug)]
struct SpanNode {
    len: usize,
    last_start: usize,
    kind: SpanKind,
}

#[derive(Debug)]
enum SpanKind {
    Leaf(Vec<FramePaintReplaySpan>),
    Branch {
        children: Vec<SpanLink>,
        ends: Vec<usize>,
    },
}

impl SpanLink {
    fn last_start(&self) -> usize {
        self.offset + self.node.last_start
    }
}

impl PaintSpanSequence {
    pub(super) fn child_link(&self, offset: usize) -> Option<SpanLink> {
        let link = self.root.as_ref()?;
        Some(SpanLink {
            offset: offset + link.offset,
            node: Arc::clone(&link.node),
        })
    }

    #[cfg(test)]
    fn compose(
        leading: Vec<FramePaintReplaySpan>,
        children: Vec<(usize, Arc<Self>)>,
        trailing: Vec<FramePaintReplaySpan>,
    ) -> Self {
        Self::compose_links(
            leading,
            children
                .into_iter()
                .filter_map(|(offset, child)| child.child_link(offset))
                .collect(),
            trailing,
        )
    }

    /// Join owner spans around shared child branches in command order.
    pub(super) fn compose_links(
        leading: Vec<FramePaintReplaySpan>,
        children: Vec<SpanLink>,
        trailing: Vec<FramePaintReplaySpan>,
    ) -> Self {
        fn leaves(spans: Vec<FramePaintReplaySpan>, links: &mut Vec<SpanLink>) {
            let mut spans = spans.into_iter();
            loop {
                let leaf: Vec<_> = spans.by_ref().take(16).collect();
                let Some(last) = leaf.last() else {
                    break;
                };
                links.push(SpanLink {
                    offset: 0,
                    node: Arc::new(SpanNode {
                        len: leaf.len(),
                        last_start: last.start,
                        kind: SpanKind::Leaf(leaf),
                    }),
                });
            }
        }
        fn balance(mut links: Vec<SpanLink>) -> SpanLink {
            while links.len() > 1 {
                let mut groups = Vec::with_capacity(links.len().div_ceil(16));
                let mut links_iter = links.into_iter();
                loop {
                    let mut children: Vec<_> = links_iter.by_ref().take(16).collect();
                    if children.is_empty() {
                        break;
                    }
                    if children.len() == 1 {
                        groups.push(children.pop().unwrap());
                        continue;
                    }
                    let mut len = 0;
                    let ends = children
                        .iter()
                        .map(|child| {
                            len += child.node.len;
                            len
                        })
                        .collect();
                    groups.push(SpanLink {
                        offset: 0,
                        node: Arc::new(SpanNode {
                            len,
                            last_start: children.last().unwrap().last_start(),
                            kind: SpanKind::Branch { children, ends },
                        }),
                    });
                }
                links = groups;
            }
            links.pop().unwrap()
        }

        let mut links = Vec::with_capacity(children.len() + 2);
        leaves(leading, &mut links);
        links.extend(children);
        leaves(trailing, &mut links);
        Self {
            root: (!links.is_empty()).then(|| balance(links)),
            flat: OnceLock::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.root.as_ref().map_or(0, |root| root.node.len)
    }
    pub fn is_empty(&self) -> bool {
        self.root.is_none()
    }

    pub fn get(&self, mut index: usize) -> Option<FramePaintReplaySpan> {
        let mut link = self.root.as_ref()?;
        if index >= link.node.len {
            return None;
        }
        let mut offset = 0;
        loop {
            offset += link.offset;
            match &link.node.kind {
                SpanKind::Leaf(spans) => return Some(shift_span(spans[index], offset)),
                SpanKind::Branch { children, ends } => {
                    let part = ends.partition_point(|end| *end <= index);
                    if part > 0 {
                        index -= ends[part - 1];
                    }
                    link = &children[part];
                }
            }
        }
    }

    /// Replace a represented owner's exact range. Covered descendants and
    /// unchanged bounds preserve their entire previous storage branch.
    pub(super) fn with_bounds(
        &self,
        owner: NodeId,
        start: usize,
        bounds: DamageRect,
    ) -> Option<Self> {
        fn replace(
            link: &SpanLink,
            owner: NodeId,
            start: usize,
            bounds: DamageRect,
        ) -> Option<SpanLink> {
            frame_work::visit(frame_work::Pass::DisplaySpanLookup);
            let start = start.checked_sub(link.offset)?;
            let kind = match &link.node.kind {
                SpanKind::Leaf(spans) => {
                    let index = spans.binary_search_by_key(&start, |span| span.start).ok()?;
                    let span = spans[index];
                    if span.owner != owner || span.bounds == bounds {
                        return None;
                    }
                    for _ in spans {
                        frame_work::visit(frame_work::Pass::DisplaySpanCopy);
                    }
                    let mut spans = spans.clone();
                    spans[index].bounds = bounds;
                    SpanKind::Leaf(spans)
                }
                SpanKind::Branch { children, ends } => {
                    let part = children.partition_point(|child| child.last_start() < start);
                    let replacement = replace(children.get(part)?, owner, start, bounds)?;
                    let mut children = children.clone();
                    children[part] = replacement;
                    SpanKind::Branch {
                        children,
                        ends: ends.clone(),
                    }
                }
            };
            Some(SpanLink {
                offset: link.offset,
                node: Arc::new(SpanNode {
                    len: link.node.len,
                    last_start: link.node.last_start,
                    kind,
                }),
            })
        }
        Some(Self {
            root: Some(replace(self.root.as_ref()?, owner, start, bounds)?),
            flat: OnceLock::new(),
        })
    }

    pub fn iter(&self) -> PaintSpanIter<'_> {
        PaintSpanIter {
            stack: self
                .root
                .as_ref()
                .map(|link| (&*link.node, link.offset))
                .into_iter()
                .collect(),
            leaf: [].iter(),
            offset: 0,
        }
    }

    /// Contiguous diagnostic view; frame preparation and replay use branches.
    pub fn as_slice(&self) -> &[FramePaintReplaySpan] {
        self.flat.get_or_init(|| self.iter().collect())
    }
}

fn shift_span(mut span: FramePaintReplaySpan, offset: usize) -> FramePaintReplaySpan {
    span.start += offset;
    span.end += offset;
    span
}

pub struct PaintSpanIter<'a> {
    stack: Vec<(&'a SpanNode, usize)>,
    leaf: std::slice::Iter<'a, FramePaintReplaySpan>,
    offset: usize,
}

impl Iterator for PaintSpanIter<'_> {
    type Item = FramePaintReplaySpan;
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(span) = self.leaf.next() {
                return Some(shift_span(*span, self.offset));
            }
            let (node, offset) = self.stack.pop()?;
            match &node.kind {
                SpanKind::Leaf(spans) => {
                    self.leaf = spans.iter();
                    self.offset = offset;
                }
                SpanKind::Branch { children, .. } => {
                    for child in children.iter().rev() {
                        self.stack.push((&child.node, offset + child.offset));
                    }
                }
            }
        }
    }
}

impl PartialEq for PaintSpanSequence {
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.iter().eq(other.iter())
    }
}
impl Eq for PaintSpanSequence {}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(owner: NodeId, start: usize, end: usize) -> FramePaintReplaySpan {
        FramePaintReplaySpan {
            owner,
            start,
            end,
            bounds: DamageRect {
                x: 1,
                y: 2,
                width: 3,
                height: 4,
            },
            command_count: end - start,
            includes_scrollbars: false,
        }
    }

    #[test]
    fn offset_span_branches_preserve_snapshots_and_bound_repairs() {
        for count in [16, 1024] {
            let children: Vec<_> = (0..count)
                .map(|i| {
                    (
                        1 + i * 2,
                        Arc::new(PaintSpanSequence::compose(
                            vec![span(i as NodeId + 1, 0, 2)],
                            Vec::new(),
                            Vec::new(),
                        )),
                    )
                })
                .collect();
            let original = Arc::new(PaintSpanSequence::compose(
                vec![span(0, 0, 1)],
                children,
                vec![span(0, count * 2 + 1, count * 2 + 2)],
            ));
            let outer = PaintSpanSequence::compose(
                Vec::new(),
                vec![(7, Arc::clone(&original))],
                Vec::new(),
            );
            let expected: Vec<_> = original.iter().map(|s| shift_span(s, 7)).collect();
            assert_eq!(outer.iter().collect::<Vec<_>>(), expected);
            for index in [0, count / 2, count - 1] {
                let bounds = DamageRect {
                    width: 8,
                    ..DamageRect::default()
                };
                let scope = frame_work::Scope::new(true);
                let repaired = outer
                    .with_bounds(index as NodeId + 1, 8 + index * 2, bounds)
                    .unwrap();
                let work = scope.snapshot();
                drop(scope);
                assert_eq!(work["display_span_copy"], 1);
                assert!(
                    work["display_span_lookup"]
                        <= (count + 2).next_power_of_two().ilog2() as u64 + 2
                );
                let mut full = expected.clone();
                full[index + 1].bounds = bounds;
                assert_eq!(repaired.iter().collect::<Vec<_>>(), full);
                assert_eq!(outer.iter().collect::<Vec<_>>(), expected);
                assert_eq!(
                    original.get(index + 1).unwrap().bounds,
                    expected[index + 1].bounds
                );
                assert!(outer.flat.get().is_none());
                assert!(repaired.flat.get().is_none());
            }
            let trailing_start = count * 2 + 8;
            let next = outer
                .with_bounds(0, trailing_start, DamageRect::default())
                .unwrap();
            assert_eq!(next.get(count + 1).unwrap().bounds, DamageRect::default());
            assert_eq!(next.get(0), outer.get(0));
            assert!(outer.with_bounds(99999, 8, DamageRect::default()).is_none());
            assert!(
                outer
                    .with_bounds(0, trailing_start + 1, DamageRect::default())
                    .is_none()
            );
            assert_eq!(outer.as_slice(), expected);
        }
        assert!(PaintSpanSequence::default().iter().next().is_none());
    }
}
