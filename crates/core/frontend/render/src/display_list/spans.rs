use std::collections::HashMap;
use std::sync::Arc;

use mesh_core_elements::{NodeId, WidgetNode};

use super::build::*;
use super::subtree::*;
use super::types::*;

pub(super) fn build_command_spans(
    root: &WidgetNode,
    subtrees: &HashMap<NodeId, Arc<RetainedPaintSubtree>>,
) -> Vec<RetainedCommandSpan> {
    let mut spans = Vec::new();
    if subtrees.is_empty() || !subtrees.contains_key(&root.id) {
        return spans;
    }

    collect_command_spans(root, subtrees, 0, &mut spans);
    spans
}

pub(super) fn collect_command_spans(
    node: &WidgetNode,
    subtrees: &HashMap<NodeId, Arc<RetainedPaintSubtree>>,
    command_start: usize,
    spans: &mut Vec<RetainedCommandSpan>,
) -> usize {
    let Some(subtree) = subtrees.get(&node.id) else {
        return command_start;
    };
    let subtree_end = command_start.saturating_add(subtree.commands.len());

    // An effect/compositing subtree is one indivisible span: its commands only
    // produce the right pixels when replayed between their own layer push and
    // pop, so no descendant may be selected on its own. The span's bounds are
    // the whole subtree's, already expanded for any effect overflow.
    if subtree.layer_scope {
        if let Some(span) = subtree.command_span {
            spans.push(RetainedCommandSpan {
                owner: node.id,
                start: command_start,
                end: subtree_end,
                bounds: span.bounds,
                command_count: subtree_end.saturating_sub(command_start),
                includes_scrollbars: span.includes_scrollbars,
            });
        }
        return subtree_end;
    }

    if let Some(span) = subtree.command_span {
        let owned = span.command_count;
        let has_children = subtree_end > command_start.saturating_add(owned);
        let bounds = span.local_bounds;
        let leading_owned = owned.saturating_sub(usize::from(span.local_includes_scrollbars));
        if !has_children {
            spans.push(RetainedCommandSpan {
                owner: node.id,
                start: command_start,
                end: command_start.saturating_add(owned),
                bounds,
                command_count: owned,
                includes_scrollbars: span.local_includes_scrollbars,
            });
        } else {
            spans.push(RetainedCommandSpan {
                owner: node.id,
                start: command_start,
                end: command_start.saturating_add(leading_owned),
                bounds,
                command_count: leading_owned,
                includes_scrollbars: false,
            });
            let scrollbar_index = subtree_end.saturating_sub(1);
            if span.local_includes_scrollbars {
                spans.push(RetainedCommandSpan {
                    owner: node.id,
                    start: scrollbar_index,
                    end: scrollbar_index.saturating_add(1),
                    bounds,
                    command_count: 1,
                    includes_scrollbars: true,
                });
            }
        }
    }

    if subtree.commands.is_empty() {
        return subtree_end;
    }

    let child_start = command_start.saturating_add(subtree.command_span.map_or(0, |span| {
        span.command_count
            .saturating_sub(usize::from(span.local_includes_scrollbars))
    }));
    let mut next_child_start = child_start;
    for_children_in_order(node, subtree.child_order.as_deref(), |child| {
        next_child_start = collect_command_spans(child, subtrees, next_child_start, spans);
    });
    subtree_end
}

#[cfg(test)]
pub(super) fn build_command_spans_with_ancestor_copying(
    root: &WidgetNode,
    subtrees: &HashMap<NodeId, Arc<RetainedPaintSubtree>>,
) -> Vec<RetainedCommandSpan> {
    pub(super) fn collect(
        node: &WidgetNode,
        subtrees: &HashMap<NodeId, Arc<RetainedPaintSubtree>>,
        command_start: usize,
    ) -> (Vec<RetainedCommandSpan>, usize) {
        let Some(subtree) = subtrees.get(&node.id) else {
            return (Vec::new(), command_start);
        };
        let subtree_end = command_start.saturating_add(subtree.commands.len());
        let mut spans = Vec::new();

        if subtree.layer_scope {
            if let Some(span) = subtree.command_span {
                spans.push(RetainedCommandSpan {
                    owner: node.id,
                    start: command_start,
                    end: subtree_end,
                    bounds: span.bounds,
                    command_count: subtree_end.saturating_sub(command_start),
                    includes_scrollbars: span.includes_scrollbars,
                });
            }
            return (spans, subtree_end);
        }

        if let Some(span) = subtree.command_span {
            let owned = span.command_count;
            let has_children = subtree_end > command_start.saturating_add(owned);
            let bounds = span.local_bounds;
            let leading_owned = owned.saturating_sub(usize::from(span.local_includes_scrollbars));
            if !has_children {
                spans.push(RetainedCommandSpan {
                    owner: node.id,
                    start: command_start,
                    end: command_start.saturating_add(owned),
                    bounds,
                    command_count: owned,
                    includes_scrollbars: span.local_includes_scrollbars,
                });
            } else {
                spans.push(RetainedCommandSpan {
                    owner: node.id,
                    start: command_start,
                    end: command_start.saturating_add(leading_owned),
                    bounds,
                    command_count: leading_owned,
                    includes_scrollbars: false,
                });
                if span.local_includes_scrollbars {
                    let scrollbar_index = subtree_end.saturating_sub(1);
                    spans.push(RetainedCommandSpan {
                        owner: node.id,
                        start: scrollbar_index,
                        end: scrollbar_index.saturating_add(1),
                        bounds,
                        command_count: 1,
                        includes_scrollbars: true,
                    });
                }
            }
        }

        if subtree.commands.is_empty() {
            return (spans, subtree_end);
        }

        let mut next_child_start =
            command_start.saturating_add(subtree.command_span.map_or(0, |span| {
                span.command_count
                    .saturating_sub(usize::from(span.local_includes_scrollbars))
            }));
        for_children_in_order(node, subtree.child_order.as_deref(), |child| {
            let (child_spans, child_end) = collect(child, subtrees, next_child_start);
            spans.extend(child_spans);
            next_child_start = child_end;
        });
        (spans, subtree_end)
    }

    if subtrees.is_empty() || !subtrees.contains_key(&root.id) {
        return Vec::new();
    }
    collect(root, subtrees, 0).0
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
