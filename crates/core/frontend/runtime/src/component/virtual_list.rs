//! Visible-range selection for windowed `{#for}` lists.
//!
//! The compiler builds only a chosen item range and brackets it with spacer
//! boxes that carry the list identity and window (`Prop::DataMeshVirtualList`).
//! After layout this module reads those spacers, the built items and the
//! nearest scrolling ancestor, and reports the lists whose range, measured item
//! size or fallback to a full build should change.

use super::FrontendSurfaceComponent;
use mesh_core_elements::style::FlexDirection;
use mesh_core_elements::{Prop, VirtualRange, WidgetNode};
use std::collections::HashMap;

/// A focused row stays built while it is within this many items of the window;
/// further away it is released rather than building everything in between.
const FOCUS_KEEP_DISTANCE: usize = 256;

/// Measured item pitches may differ from the recorded one by this many px
/// (layout rounding) before the list is built in full instead.
const PITCH_TOLERANCE: f32 = 0.75;

impl FrontendSurfaceComponent {
    /// Tells every instance how large the surface is, so a long list's first
    /// build already covers its viewport.
    pub(super) fn publish_virtual_viewport_hint(&mut self, extent: f32) {
        for runtime in self.runtimes.lock().unwrap().values_mut() {
            runtime
                .script_ctx
                .state_mut()
                .set_virtual_viewport_hint(extent);
        }
    }

    /// Records the range each list should build and schedules a rebuild when
    /// any changed. The range lives in the owning instance's script state, so
    /// the ordinary state-change path invalidates its memo.
    pub(super) fn update_virtual_windows(&mut self, tree: &WidgetNode) {
        let (choices, seen) = choose_virtual_windows(
            tree,
            self.focused_key.as_deref(),
            &mut self.virtual_pitches,
        );
        self.virtual_lists_seen = seen;
        let mut changed = false;
        {
            let mut runtimes = self.runtimes.lock().unwrap();
            for choice in &choices {
                let Some((instance_key, _)) = choice.list_id.rsplit_once('#') else {
                    continue;
                };
                if let Some(runtime) = runtimes.get_mut(instance_key) {
                    changed |= runtime
                        .script_ctx
                        .state_mut()
                        .set_virtual_window(&choice.list_id, choice.range);
                }
            }
        }
        if changed {
            self.invalidate_script_state();
        }
    }
}

/// Measured item pitches for the lists whose items differ in size, keyed by
/// list identity. A list enters this mode when its built items are not all the
/// same size and stays in it until its length changes.
#[derive(Debug, Default)]
pub(super) struct PitchCache {
    lists: HashMap<String, ListPitches>,
}

#[derive(Debug, Default)]
struct ListPitches {
    total: usize,
    /// Distance from each item to the next along the scroll axis, gap
    /// included; zero until the item has been built and laid out.
    pitches: Vec<f32>,
    /// Pitch assumed for items not yet measured. Fixed when the list enters
    /// this mode, so later measurements do not move content under the viewport.
    estimate: f32,
}

impl ListPitches {
    fn pitch(&self, index: usize) -> f32 {
        match self.pitches.get(index) {
            Some(pitch) if *pitch > 0.0 => *pitch,
            _ => self.estimate,
        }
    }
}

/// A windowed list whose recorded range should change.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct VirtualWindowChoice {
    pub(super) list_id: String,
    pub(super) range: VirtualRange,
}

struct SpacerMeta<'a> {
    list_id: &'a str,
    first: usize,
    last: usize,
    total: usize,
    /// Item size along the scroll axis, gap excluded; zero when unmeasured.
    extent: f32,
    overscan: usize,
}

fn parse_meta(node: &WidgetNode) -> Option<SpacerMeta<'_>> {
    let mut fields = node
        .attributes
        .get(Prop::DataMeshVirtualList)?
        .split_whitespace();
    let meta = SpacerMeta {
        list_id: fields.next()?,
        first: fields.next()?.parse().ok()?,
        last: fields.next()?.parse().ok()?,
        total: fields.next()?.parse().ok()?,
        extent: fields.next()?.parse().ok()?,
        overscan: fields.next()?.parse().ok()?,
    };
    Some(meta)
}

/// Lists in `tree` whose recorded range should change, and whether the tree
/// contains any windowed list at all.
pub(super) fn choose_virtual_windows(
    tree: &WidgetNode,
    focused_key: Option<&str>,
    cache: &mut PitchCache,
) -> (Vec<VirtualWindowChoice>, bool) {
    let mut choices = Vec::new();
    let mut seen = false;
    let mut ancestors = Vec::new();
    walk(
        tree,
        focused_key,
        cache,
        &mut ancestors,
        &mut choices,
        &mut seen,
    );
    (choices, seen)
}

fn walk<'a>(
    node: &'a WidgetNode,
    focused_key: Option<&str>,
    cache: &mut PitchCache,
    ancestors: &mut Vec<&'a WidgetNode>,
    choices: &mut Vec<VirtualWindowChoice>,
    seen: &mut bool,
) {
    ancestors.push(node);
    let mut handled: Vec<&str> = Vec::new();
    for child in &node.children {
        if let Some(meta) = parse_meta(child)
            && !handled.contains(&meta.list_id)
        {
            handled.push(meta.list_id);
            *seen = true;
            if let Some(range) = choose_for_list(node, ancestors, &meta, focused_key, cache) {
                choices.push(VirtualWindowChoice {
                    list_id: meta.list_id.to_owned(),
                    range,
                });
            }
        }
    }
    for child in &node.children {
        walk(child, focused_key, cache, ancestors, choices, seen);
    }
    ancestors.pop();
}

/// The new range for one list, or `None` when its recorded range still holds.
fn choose_for_list(
    parent: &WidgetNode,
    ancestors: &[&WidgetNode],
    meta: &SpacerMeta<'_>,
    focused_key: Option<&str>,
    cache: &mut PitchCache,
) -> Option<VirtualRange> {
    let column = !matches!(parent.computed_style.direction, FlexDirection::Row);
    let position = |node: &WidgetNode| if column { node.layout.y } else { node.layout.x };
    let spacer_indices: Vec<usize> = parent
        .children
        .iter()
        .enumerate()
        .filter(|(_, child)| parse_meta(child).is_some_and(|other| other.list_id == meta.list_id))
        .map(|(index, _)| index)
        .collect();
    let built = meta.last - meta.first;
    // The built items sit between the spacers: the trailing spacer ends them
    // and the leading one starts them.
    let items = if meta.last < meta.total {
        let end = *spacer_indices.last()?;
        parent.children.get(end.checked_sub(built)?..end)?
    } else {
        let start = *spacer_indices.first()? + 1;
        parent.children.get(start..start + built)?
    };
    let gap = parent.computed_style.main_axis_gap();

    let scroller = ancestors.iter().rev().find(|node| {
        let overflow = if column {
            node.computed_style.overflow_y
        } else {
            node.computed_style.overflow_x
        };
        overflow.clips_contents()
    });
    // Without a scrolling container there is no viewport to window against.
    let Some(scroller) = scroller else {
        return Some(VirtualRange::FULL);
    };

    let pitch = uniform_pitch(items, &position);
    let variable = cache.lists.contains_key(meta.list_id)
        || if meta.extent > 0.0 {
            match pitch {
                Some(measured) => (measured - (meta.extent + gap)).abs() > PITCH_TOLERANCE,
                None => items.len() >= 3,
            }
        } else {
            pitch.is_none_or(|pitch| pitch - gap <= 0.0) && items.len() >= 3
        };
    if variable {
        return choose_variable(
            parent,
            &spacer_indices,
            items,
            meta,
            (gap, scroller, column),
            focused_key,
            cache,
        );
    }
    let extent = if meta.extent > 0.0 {
        meta.extent
    } else {
        match pitch {
            Some(pitch) if pitch - gap > 0.0 => pitch - gap,
            _ => return None,
        }
    };
    let measuring = meta.extent <= 0.0;
    let pitch = extent + gap;

    let list_start = position(items.first()?) - meta.first as f32 * pitch;
    let scroll = scroller.resolved_scroll_metrics();
    let (scroll_offset, viewport) = if column {
        (scroll.y, scroller.layout.height)
    } else {
        (scroll.x, scroller.layout.width)
    };
    let band_start = position(scroller) + scroll_offset - list_start;
    let band_end = band_start + viewport;
    let visible_first = ((band_start / pitch).floor().max(0.0) as usize).min(meta.total);
    let visible_last = ((band_end / pitch).ceil().max(0.0) as usize).min(meta.total);

    let covered = (meta.first == 0 || visible_first >= meta.first)
        && (meta.last == meta.total || visible_last <= meta.last);
    if covered && !measuring {
        return None;
    }
    let (first, last) = if covered {
        (meta.first, meta.last)
    } else {
        let first = visible_first.saturating_sub(meta.overscan);
        let last = (visible_last + meta.overscan).min(meta.total).max(first);
        keep_focused_item(
            (first, last),
            focused_item_ordinal(items, meta.first, focused_key),
        )
    };
    Some(VirtualRange {
        first,
        last,
        extent,
        spacers: None,
    })
}

/// Window selection for a list whose items differ in size: spacers are sums of
/// measured item pitches, with the fixed estimate standing in for items that
/// have not been built yet.
fn choose_variable(
    parent: &WidgetNode,
    spacer_indices: &[usize],
    items: &[WidgetNode],
    meta: &SpacerMeta<'_>,
    (gap, scroller, column): (f32, &WidgetNode, bool),
    focused_key: Option<&str>,
    cache: &mut PitchCache,
) -> Option<VirtualRange> {
    let position = |node: &WidgetNode| if column { node.layout.y } else { node.layout.x };
    let size = |node: &WidgetNode| if column { node.layout.height } else { node.layout.width };
    let entered = !cache.lists.contains_key(meta.list_id);
    let list = cache.lists.entry(meta.list_id.to_owned()).or_default();
    if list.total != meta.total {
        *list = ListPitches {
            total: meta.total,
            pitches: vec![0.0; meta.total],
            estimate: 0.0,
        };
    }
    let trailing = (meta.last < meta.total)
        .then(|| spacer_indices.last().and_then(|&at| parent.children.get(at)))
        .flatten();
    for (offset, item) in items.iter().enumerate() {
        let next = items.get(offset + 1).or(trailing);
        let pitch = match next {
            Some(next) => position(next) - position(item),
            None => size(item) + gap,
        };
        if let Some(slot) = list.pitches.get_mut(meta.first + offset) {
            *slot = pitch.max(0.0);
        }
    }
    if list.estimate <= 0.0 {
        let known: Vec<f32> = list.pitches.iter().copied().filter(|p| *p > 0.0).collect();
        list.estimate = if known.is_empty() && entered && meta.extent > 0.0 {
            meta.extent + gap
        } else {
            known.iter().sum::<f32>() / known.len().max(1) as f32
        };
    }
    if list.estimate <= 0.0 {
        return None;
    }

    let main = |node: &WidgetNode| size(node);
    let leading = (meta.first > 0)
        .then(|| spacer_indices.first().and_then(|&at| parent.children.get(at)))
        .flatten();
    let list_start = leading.map_or_else(|| position(&items[0]), |spacer| position(spacer));
    let scroll = scroller.resolved_scroll_metrics();
    let (scroll_offset, viewport) = if column {
        (scroll.y, scroller.layout.height)
    } else {
        (scroll.x, scroller.layout.width)
    };
    let band_start = position(scroller) + scroll_offset - list_start;
    let band_end = band_start + viewport;
    let mut visible_first = meta.total;
    let mut visible_last = meta.total;
    let mut start = 0.0_f32;
    for index in 0..meta.total {
        let end = start + list.pitch(index);
        if visible_first == meta.total && end > band_start {
            visible_first = index;
        }
        if start >= band_end {
            visible_last = index;
            break;
        }
        start = end;
    }
    let visible_first = visible_first.min(visible_last);

    let covered = (meta.first == 0 || visible_first >= meta.first)
        && (meta.last == meta.total || visible_last <= meta.last);
    let (first, last) = if covered {
        (meta.first, meta.last)
    } else {
        let first = visible_first.saturating_sub(meta.overscan);
        let last = (visible_last + meta.overscan).min(meta.total).max(first);
        keep_focused_item(
            (first, last),
            focused_item_ordinal(items, meta.first, focused_key),
        )
    };
    let skipped = |range: std::ops::Range<usize>| {
        if range.is_empty() {
            0.0
        } else {
            (range.map(|index| list.pitch(index)).sum::<f32>() - gap).max(0.0)
        }
    };
    let (lead, trail) = (skipped(0..first), skipped(last..meta.total));
    let recorded = |spacer: Option<&WidgetNode>| spacer.map_or(0.0, main);
    let settled = (lead - recorded(leading)).abs() < 0.5
        && (trail - recorded(trailing)).abs() < 0.5
        && (meta.extent - list.estimate).abs() < 0.01;
    if covered && settled {
        return None;
    }
    Some(VirtualRange {
        first,
        last,
        extent: list.estimate,
        spacers: Some((lead, trail)),
    })
}

/// The ordinal of the built item holding keyboard focus, if any.
fn focused_item_ordinal(
    items: &[WidgetNode],
    first_ordinal: usize,
    focused_key: Option<&str>,
) -> Option<usize> {
    fn holds(node: &WidgetNode, key: &str) -> bool {
        node.mesh_key() == Some(key) || node.children.iter().any(|child| holds(child, key))
    }
    let key = focused_key?;
    items
        .iter()
        .position(|item| holds(item, key))
        .map(|index| first_ordinal + index)
}

/// Widens `window` to include the focused item when it is close enough.
fn keep_focused_item(window: (usize, usize), focused: Option<usize>) -> (usize, usize) {
    let (first, last) = window;
    match focused {
        Some(ordinal) if ordinal < first && first - ordinal <= FOCUS_KEEP_DISTANCE => {
            (ordinal, last)
        }
        Some(ordinal) if ordinal >= last && ordinal + 1 - last <= FOCUS_KEEP_DISTANCE => {
            (first, ordinal + 1)
        }
        _ => window,
    }
}

/// The distance between consecutive items along the scroll axis when it is the
/// same for all of them (at least three items), else `None`.
fn uniform_pitch(items: &[WidgetNode], position: &impl Fn(&WidgetNode) -> f32) -> Option<f32> {
    if items.len() < 3 {
        return None;
    }
    let pitch = position(&items[1]) - position(&items[0]);
    items
        .windows(2)
        .all(|pair| (position(&pair[1]) - position(&pair[0]) - pitch).abs() <= PITCH_TOLERANCE)
        .then_some(pitch)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mesh_core_elements::WidgetScrollMetrics;
    use mesh_core_elements::style::Overflow;

    const EXTENT: f32 = 40.0;

    fn choose(tree: &WidgetNode) -> (Vec<VirtualWindowChoice>, bool) {
        choose_virtual_windows(tree, None, &mut PitchCache::default())
    }

    fn spacer(meta: &str, pos: f32) -> WidgetNode {
        let mut node = WidgetNode::new("box");
        node.attributes
            .insert(Prop::DataMeshVirtualList, meta.to_owned());
        node.layout.y = pos;
        node
    }

    /// A 200px-tall scroller scrolled by `scroll` over a column holding items
    /// `[first, last)` of a `total`-item list whose items are `extent` apart
    /// (`gap` included), recorded with `meta_extent`.
    fn tree_with(
        (first, last, total): (usize, usize, usize),
        scroll: f32,
        pitches: impl Fn(usize) -> f32,
        gap: f32,
        meta_extent: f32,
    ) -> WidgetNode {
        let meta = format!("main#7 {first} {last} {total} {meta_extent} 2 0");
        let mut list_parent = WidgetNode::new("column");
        list_parent.computed_style.direction = FlexDirection::Column;
        list_parent.computed_style.gap = gap;
        if first > 0 {
            list_parent.children.push(spacer(&meta, 0.0));
        }
        let mut position = first as f32 * (EXTENT + gap);
        for index in first..last {
            let mut item = WidgetNode::new("text");
            item.layout.y = position;
            list_parent.children.push(item);
            position += pitches(index);
        }
        if last < total {
            list_parent.children.push(spacer(&meta, position));
        }
        let mut scroller = WidgetNode::new("box");
        scroller.computed_style.overflow_y = Overflow::Auto;
        scroller.layout.height = 200.0;
        scroller.scroll_metrics = Some(WidgetScrollMetrics {
            y: scroll,
            ..Default::default()
        });
        scroller.children.push(list_parent);
        let mut root = WidgetNode::new("box");
        root.children.push(scroller);
        root
    }

    fn tree(first: usize, last: usize, total: usize, scroll: f32) -> WidgetNode {
        tree_with((first, last, total), scroll, |_| EXTENT, 0.0, EXTENT)
    }

    fn range(first: usize, last: usize) -> Vec<VirtualWindowChoice> {
        vec![VirtualWindowChoice {
            list_id: "main#7".into(),
            range: VirtualRange {
                first,
                last,
                extent: EXTENT,
                spacers: None,
            },
        }]
    }

    #[test]
    fn a_range_covering_the_visible_band_needs_no_change() {
        let (choices, seen) = choose(&tree(0, 7, 100, 0.0));
        assert!(seen);
        assert!(choices.is_empty());
    }

    #[test]
    fn scrolling_past_the_built_range_chooses_a_window_around_the_viewport() {
        // Scrolled 1000px: visible items 25..30, so 23..32 with overscan 2.
        let (choices, _) = choose(&tree(0, 7, 100, 1000.0));
        assert_eq!(choices, range(23, 32));
    }

    #[test]
    fn scrolling_back_above_the_built_range_chooses_an_earlier_window() {
        let (choices, _) = choose(&tree(23, 32, 100, 0.0));
        assert_eq!(choices, range(0, 7));
    }

    #[test]
    fn the_end_of_the_list_clamps_the_window() {
        let (choices, _) = choose(&tree(0, 7, 100, 99_999.0));
        assert_eq!(choices, range(98, 100));
    }

    #[test]
    fn a_tree_without_windowed_lists_reports_none() {
        let (choices, seen) = choose(&WidgetNode::new("box"));
        assert!(choices.is_empty());
        assert!(!seen);
    }

    #[test]
    fn a_list_outside_any_scroller_is_built_in_full() {
        let mut root = tree(0, 7, 100, 1000.0);
        root.children[0].computed_style.overflow_y = Overflow::Visible;
        let (choices, seen) = choose(&root);
        assert!(seen);
        assert_eq!(choices.len(), 1);
        assert_eq!(choices[0].range, VirtualRange::FULL);
    }

    #[test]
    fn an_unmeasured_list_infers_its_item_size_and_window() {
        // Items 52px apart with a 12px gap are 40px items.
        let (choices, _) = choose(&tree_with(
            (0, 26, 500),
            0.0,
            |_| 52.0,
            12.0,
            0.0,
        ));
        assert_eq!(choices.len(), 1);
        let chosen = choices[0].range;
        assert_eq!(chosen.extent, 40.0);
        assert_eq!((chosen.first, chosen.last), (0, 26));
    }

    fn uneven(index: usize) -> f32 {
        if index % 5 == 0 { 90.0 } else { 40.0 }
    }

    #[test]
    fn items_of_different_sizes_window_against_an_estimated_pitch() {
        let (choices, _) = choose(&tree_with((0, 26, 500), 0.0, uneven, 0.0, 0.0));
        assert_eq!(choices.len(), 1);
        let range = choices[0].range;
        // Items 0, 5, .. 25 are 90px: 6 * 90 + 20 * 40 = 1340 over 26 items.
        let estimate = 1340.0 / 26.0;
        assert_eq!((range.first, range.last), (0, 26));
        assert!((range.extent - estimate).abs() < 0.01);
        let (lead, trail) = range.spacers.unwrap();
        assert_eq!(lead, 0.0);
        assert!((trail - 474.0 * estimate).abs() < 0.5);
    }

    #[test]
    fn a_recorded_extent_that_no_longer_matches_measures_the_real_pitch() {
        let (choices, _) = choose(&tree_with((0, 26, 500), 0.0, |_| 64.0, 0.0, EXTENT));
        assert_eq!(choices.len(), 1);
        let range = choices[0].range;
        assert_eq!(range.extent, 64.0);
        assert_eq!(range.spacers, Some((0.0, 474.0 * 64.0)));
    }

    #[test]
    fn a_settled_uneven_list_asks_for_no_change() {
        let mut cache = PitchCache::default();
        let (first, _) = choose_virtual_windows(
            &tree_with((0, 26, 500), 0.0, uneven, 0.0, 0.0),
            None,
            &mut cache,
        );
        let range = first[0].range;
        let mut settled = tree_with((0, 26, 500), 0.0, uneven, 0.0, range.extent);
        let list = &mut settled.children[0].children[0];
        list.children.last_mut().unwrap().layout.height = range.spacers.unwrap().1;
        let (second, _) = choose_virtual_windows(&settled, None, &mut cache);
        assert!(second.is_empty(), "{second:?}");
    }

    #[test]
    fn scrolling_an_uneven_list_uses_cumulative_pitches() {
        let mut cache = PitchCache::default();
        choose_virtual_windows(
            &tree_with((0, 26, 500), 0.0, uneven, 0.0, 0.0),
            None,
            &mut cache,
        );
        let estimate = 1340.0 / 26.0;
        let (choices, _) = choose_virtual_windows(
            &tree_with((0, 26, 500), 10_000.0, uneven, 0.0, estimate),
            None,
            &mut cache,
        );
        let range = choices[0].range;
        let visible = (10_000.0 / estimate) as usize;
        assert!(range.first <= visible && range.first + 6 >= visible, "{range:?}");
        let (lead, trail) = range.spacers.unwrap();
        assert!((lead - range.first as f32 * estimate).abs() < estimate + 1.0);
        assert!(trail > 0.0);
    }

    #[test]
    fn gaps_are_part_of_the_scroll_pitch() {
        // 40px items 10px apart: the band at 1000px starts at item 20.
        let (choices, _) = choose(&tree_with(
            (0, 7, 100),
            1000.0,
            |_| 50.0,
            10.0,
            EXTENT,
        ));
        assert_eq!(choices.len(), 1);
        assert_eq!(choices[0].range.first, 18);
        assert_eq!(choices[0].range.extent, EXTENT);
    }

    /// Marks the item at `ordinal` (built within `first..`) as holding focus.
    fn focus_item(tree: &mut WidgetNode, first: usize, ordinal: usize) -> &'static str {
        let list = &mut tree.children[0].children[0];
        let spacer = usize::from(first > 0);
        list.children[spacer + ordinal - first].set_mesh_key("focused-row");
        "focused-row"
    }

    #[test]
    fn a_focused_row_just_above_the_new_window_stays_built() {
        let mut tree = tree(0, 7, 100, 1000.0);
        let key = focus_item(&mut tree, 0, 3);
        let (choices, _) = choose_virtual_windows(&tree, Some(key), &mut PitchCache::default());
        assert_eq!(choices, range(3, 32));
    }

    #[test]
    fn a_focused_row_below_the_new_window_stays_built() {
        let mut tree = tree(23, 32, 100, 0.0);
        let key = focus_item(&mut tree, 23, 30);
        let (choices, _) = choose_virtual_windows(&tree, Some(key), &mut PitchCache::default());
        assert_eq!(choices, range(0, 31));
    }

    #[test]
    fn a_focused_row_far_from_the_window_is_released() {
        let mut tree = tree(0, 7, 2_000, 70_000.0);
        let key = focus_item(&mut tree, 0, 3);
        let (choices, _) = choose_virtual_windows(&tree, Some(key), &mut PitchCache::default());
        assert_eq!(choices.len(), 1);
        assert!(choices[0].range.first > FOCUS_KEEP_DISTANCE);
    }

    #[test]
    fn focus_outside_the_list_does_not_change_the_window() {
        let (with_focus, _) = choose_virtual_windows(
            &tree(0, 7, 100, 1000.0),
            Some("elsewhere"),
            &mut PitchCache::default(),
        );
        assert_eq!(with_focus, range(23, 32));
    }
}
