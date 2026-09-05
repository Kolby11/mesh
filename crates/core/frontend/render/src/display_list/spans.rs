use std::collections::HashMap;
use std::sync::Arc;

use mesh_core_elements::{NodeId, WidgetNode};

use super::subtree::*;
use super::types::*;

pub(super) fn build_command_spans(
    root: &WidgetNode,
    subtrees: &HashMap<NodeId, Arc<RetainedPaintSubtree>>,
) -> Arc<[RetainedCommandSpan]> {
    subtrees
        .get(&root.id)
        .map(|subtree| Arc::clone(&subtree.spans))
        .unwrap_or_else(|| Vec::new().into())
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
