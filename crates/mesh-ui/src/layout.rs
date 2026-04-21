/// Flexbox-subset layout engine.
///
/// Computes `LayoutRect` for every node in a widget tree. Supports row/column
/// direction, flex-grow/shrink, gap, padding, and margin.
use crate::style::{AlignItems, AlignSelf, Dimension, Display, FlexDirection, JustifyContent, Overflow};
use crate::tree::WidgetNode;

/// Computed layout rectangle for a node.
#[derive(Debug, Clone, Copy, Default)]
pub struct LayoutRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl LayoutRect {
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px < self.x + self.width && py >= self.y && py < self.y + self.height
    }
}

/// The layout engine. Stateless — call `compute` on a widget tree.
pub struct LayoutEngine;

impl LayoutEngine {
    /// Compute layout for the entire tree within the given bounds.
    pub fn compute(root: &mut WidgetNode, available_width: f32, available_height: f32) {
        layout_node(root, 0.0, 0.0, available_width, available_height);
    }
}

fn layout_node(node: &mut WidgetNode, x: f32, y: f32, available_w: f32, available_h: f32) {
    let style = &node.computed_style;

    if style.display == Display::None {
        node.layout = LayoutRect::default();
        return;
    }

    let padding = style.padding;
    let margin = style.margin;

    // Resolve own size, then apply min/max constraints.
    let width = match style.width {
        Dimension::Px(px) => px,
        Dimension::Percent(pct) => available_w * pct / 100.0,
        Dimension::Auto => available_w - margin.horizontal(),
    };
    let width = clamp_dimension(width, style.min_width, style.max_width);

    let height = match style.height {
        Dimension::Px(px) => px,
        Dimension::Percent(pct) => available_h * pct / 100.0,
        Dimension::Auto => available_h - margin.vertical(),
    };
    let height = clamp_dimension(height, style.min_height, style.max_height);

    node.layout = LayoutRect {
        x: x + margin.left,
        y: y + margin.top,
        width,
        height,
    };

    if node.children.is_empty() {
        return;
    }

    // Layout children along the flex axis.
    let inner_w = (width - padding.horizontal()).max(0.0);
    let inner_h = (height - padding.vertical()).max(0.0);
    let inner_x = node.layout.x + padding.left;
    let inner_y = node.layout.y + padding.top;

    let visible_children: Vec<usize> = node
        .children
        .iter()
        .enumerate()
        .filter(|(_, c)| c.computed_style.display != Display::None)
        .map(|(i, _)| i)
        .collect();

    let child_count = visible_children.len();
    if child_count == 0 {
        return;
    }

    let total_gap = style.gap * (child_count as f32 - 1.0).max(0.0);
    let is_column = style.direction == FlexDirection::Column;

    // For overflow containers, children can use their natural main-axis size.
    let overflow_clips_main = if is_column {
        style.overflow_y != Overflow::Visible
    } else {
        style.overflow_x != Overflow::Visible
    };

    let main_available = if is_column {
        inner_h - total_gap
    } else {
        inner_w - total_gap
    };

    // First pass: size fixed children; intrinsic-measure auto children with no flex-grow;
    // accumulate flex-grow total for the rest.
    let mut total_flex_grow = 0.0f32;
    let mut fixed_main = 0.0f32;
    // NAN is the sentinel for "not yet sized" — avoids a separate Vec<bool> and
    // correctly handles children whose intrinsic size is legitimately 0.
    let mut child_sizes: Vec<f32> = vec![f32::NAN; node.children.len()];

    for &idx in &visible_children {
        let child_style = &node.children[idx].computed_style;
        // flex-basis overrides the main-axis dimension if it is not Auto.
        let main_dim = if is_column {
            match child_style.flex_basis {
                Dimension::Auto => child_style.height,
                other => other,
            }
        } else {
            match child_style.flex_basis {
                Dimension::Auto => child_style.width,
                other => other,
            }
        };

        match main_dim {
            Dimension::Px(px) => {
                child_sizes[idx] = px;
                fixed_main += px;
            }
            Dimension::Percent(pct) => {
                let s = main_available * pct / 100.0;
                child_sizes[idx] = s;
                fixed_main += s;
            }
            Dimension::Auto => {
                let grow = child_style.flex_grow.max(0.0);
                if grow > 0.0 {
                    total_flex_grow += grow;
                } else {
                    // Measure intrinsic main-axis size via dry-run layout.
                    let large = 32_000.0_f32;
                    let (mw, mh) = if is_column {
                        (inner_w, large)
                    } else {
                        (large, inner_h)
                    };
                    let mut dummy = node.children[idx].clone();
                    layout_node(&mut dummy, 0.0, 0.0, mw, mh);
                    let size = if is_column {
                        dummy.layout.height
                    } else {
                        dummy.layout.width
                    };
                    child_sizes[idx] = size;
                    fixed_main += size;
                }
            }
        }
    }

    // Second pass: distribute remaining space to flex-grow children.
    let remaining = (main_available - fixed_main).max(0.0);
    for &idx in &visible_children {
        if !child_sizes[idx].is_nan() {
            continue;
        }
        let grow = node.children[idx].computed_style.flex_grow.max(0.0);
        if total_flex_grow > 0.0 && grow > 0.0 {
            child_sizes[idx] = remaining * (grow / total_flex_grow);
        } else if overflow_clips_main {
            // Overflow container: measure natural size so content can scroll.
            let large = 32_000.0_f32;
            let (mw, mh) = if is_column {
                (inner_w, large)
            } else {
                (large, inner_h)
            };
            let mut dummy = node.children[idx].clone();
            layout_node(&mut dummy, 0.0, 0.0, mw, mh);
            child_sizes[idx] = if is_column {
                dummy.layout.height
            } else {
                dummy.layout.width
            };
        } else {
            child_sizes[idx] = 0.0;
        }
    }

    // Third pass: apply justify-content initial offset and inter-item spacing.
    let justify = style.justify_content;
    let total_child_main: f32 = visible_children.iter().map(|&i| child_sizes[i]).sum();
    let leftover = (main_available - total_gap - total_child_main).max(0.0);

    let (mut cursor, extra_gap) = match justify {
        JustifyContent::End => (leftover, 0.0),
        JustifyContent::Center => (leftover / 2.0, 0.0),
        JustifyContent::SpaceBetween => (
            0.0,
            if child_count > 1 {
                leftover / (child_count as f32 - 1.0)
            } else {
                0.0
            },
        ),
        JustifyContent::SpaceAround => {
            let per = leftover / child_count as f32;
            (per / 2.0, per)
        }
        JustifyContent::Start => (0.0, 0.0),
    };

    let container_align = style.align_items;
    let mut content_main = 0.0f32;
    for &idx in &visible_children {
        let child_main_size = child_sizes[idx];
        let child_style = &node.children[idx].computed_style;
        let effective_align = resolve_align(child_style.align_self, container_align);

        let (cx, cy, cw, ch) = if is_column {
            let (child_cw, cx_off) = cross_axis_layout(effective_align, inner_w, child_style.width);
            (
                inner_x + cx_off,
                inner_y + cursor,
                child_cw,
                child_main_size,
            )
        } else {
            let (child_ch, cy_off) =
                cross_axis_layout(effective_align, inner_h, child_style.height);
            (
                inner_x + cursor,
                inner_y + cy_off,
                child_main_size,
                child_ch,
            )
        };

        layout_node(&mut node.children[idx], cx, cy, cw, ch);
        content_main += child_main_size;
        cursor += child_main_size + style.gap + extra_gap;
    }
    // Add gaps between children (not after the last one).
    let content_main = content_main + total_gap;

    // Shrink auto dimensions to fit children (bottom-up sizing).
    let style = &node.computed_style;
    if matches!(style.height, Dimension::Auto) && is_column {
        let content_h = (content_main + padding.vertical()).max(0.0);
        node.layout.height = clamp_dimension(content_h, style.min_height, style.max_height);
    }
    if matches!(style.width, Dimension::Auto) && !is_column {
        let content_w = (content_main + padding.horizontal()).max(0.0);
        node.layout.width = clamp_dimension(content_w, style.min_width, style.max_width);
    }
    // Row containers: shrink height to max child height when height is Auto.
    if matches!(style.height, Dimension::Auto) && !is_column {
        let max_h = node
            .children
            .iter()
            .filter(|c| c.computed_style.display != Display::None)
            .map(|c| c.layout.height + c.computed_style.margin.vertical())
            .fold(0.0f32, f32::max);
        let content_h = (max_h + padding.vertical()).max(0.0);
        node.layout.height = clamp_dimension(content_h, style.min_height, style.max_height);
    }
    // Column containers: shrink width to max child width when width is Auto.
    if matches!(style.width, Dimension::Auto) && is_column {
        let max_w = node
            .children
            .iter()
            .filter(|c| c.computed_style.display != Display::None)
            .map(|c| c.layout.width + c.computed_style.margin.horizontal())
            .fold(0.0f32, f32::max);
        let content_w = (max_w + padding.horizontal()).max(0.0);
        node.layout.width = clamp_dimension(content_w, style.min_width, style.max_width);
    }
}

fn clamp_dimension(value: f32, min: Option<f32>, max: Option<f32>) -> f32 {
    let v = if let Some(mn) = min {
        value.max(mn)
    } else {
        value
    };
    if let Some(mx) = max { v.min(mx) } else { v }
}

/// Resolve a child's effective cross-axis alignment.
fn resolve_align(child_self: AlignSelf, container: AlignItems) -> AlignItems {
    match child_self {
        AlignSelf::Auto => container,
        AlignSelf::Start | AlignSelf::Baseline => AlignItems::Start,
        AlignSelf::End => AlignItems::End,
        AlignSelf::Center => AlignItems::Center,
        AlignSelf::Stretch => AlignItems::Stretch,
    }
}

/// Compute the cross-axis size and offset for a child given its alignment.
fn cross_axis_layout(align: AlignItems, inner_size: f32, child_dim: Dimension) -> (f32, f32) {
    match align {
        AlignItems::Stretch => (inner_size, 0.0),
        AlignItems::Start => {
            let size = explicit_size(child_dim, inner_size);
            (size, 0.0)
        }
        AlignItems::Center => {
            let size = explicit_size(child_dim, inner_size);
            let offset = ((inner_size - size) / 2.0).max(0.0);
            (size, offset)
        }
        AlignItems::End => {
            let size = explicit_size(child_dim, inner_size);
            let offset = (inner_size - size).max(0.0);
            (size, offset)
        }
    }
}

/// Return the explicit pixel size of a dimension, falling back to inner_size for Auto.
fn explicit_size(dim: Dimension, inner_size: f32) -> f32 {
    match dim {
        Dimension::Px(px) => px,
        Dimension::Percent(pct) => inner_size * pct / 100.0,
        Dimension::Auto => inner_size,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::{Edges, FlexDirection};

    fn make_node(tag: &str, width: Dimension, height: Dimension) -> WidgetNode {
        let mut node = WidgetNode::new(tag);
        node.computed_style.width = width;
        node.computed_style.height = height;
        node
    }

    #[test]
    fn simple_row_layout() {
        let mut root = make_node("row", Dimension::Px(300.0), Dimension::Px(50.0));
        root.computed_style.direction = FlexDirection::Row;

        let child1 = make_node("text", Dimension::Px(100.0), Dimension::Auto);
        let child2 = make_node("text", Dimension::Px(100.0), Dimension::Auto);
        root.children = vec![child1, child2];

        LayoutEngine::compute(&mut root, 300.0, 50.0);

        assert_eq!(root.layout.width, 300.0);
        assert_eq!(root.children[0].layout.x, 0.0);
        assert_eq!(root.children[0].layout.width, 100.0);
        assert_eq!(root.children[1].layout.x, 100.0);
        assert_eq!(root.children[1].layout.width, 100.0);
    }

    #[test]
    fn column_with_gap() {
        let mut root = make_node("column", Dimension::Px(200.0), Dimension::Px(300.0));
        root.computed_style.direction = FlexDirection::Column;
        root.computed_style.gap = 10.0;

        let child1 = make_node("text", Dimension::Auto, Dimension::Px(50.0));
        let child2 = make_node("text", Dimension::Auto, Dimension::Px(50.0));
        root.children = vec![child1, child2];

        LayoutEngine::compute(&mut root, 200.0, 300.0);

        assert_eq!(root.children[0].layout.y, 0.0);
        assert_eq!(root.children[0].layout.height, 50.0);
        assert_eq!(root.children[1].layout.y, 60.0); // 50 + 10 gap
        assert_eq!(root.children[1].layout.height, 50.0);
    }

    #[test]
    fn flex_grow_distributes_space() {
        let mut root = make_node("row", Dimension::Px(300.0), Dimension::Px(50.0));
        root.computed_style.direction = FlexDirection::Row;

        let mut child1 = make_node("a", Dimension::Auto, Dimension::Auto);
        child1.computed_style.flex_grow = 1.0;
        let mut child2 = make_node("b", Dimension::Auto, Dimension::Auto);
        child2.computed_style.flex_grow = 2.0;
        root.children = vec![child1, child2];

        LayoutEngine::compute(&mut root, 300.0, 50.0);

        assert!((root.children[0].layout.width - 100.0).abs() < 0.1);
        assert!((root.children[1].layout.width - 200.0).abs() < 0.1);
    }

    #[test]
    fn padding_insets_children() {
        let mut root = make_node("row", Dimension::Px(200.0), Dimension::Px(100.0));
        root.computed_style.padding = Edges::all(10.0);

        let child = make_node("text", Dimension::Px(50.0), Dimension::Auto);
        root.children = vec![child];

        LayoutEngine::compute(&mut root, 200.0, 100.0);

        assert_eq!(root.children[0].layout.x, 10.0);
        assert_eq!(root.children[0].layout.y, 10.0);
    }
}
