/// Flexbox-subset layout engine.
///
/// Computes `LayoutRect` for every node in a widget tree. Supports row/column
/// direction, flex-grow/shrink, gap, padding, and margin.
use crate::style::{Dimension, Display, FlexDirection};
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

    let padding = &style.padding;
    let margin = &style.margin;

    // Resolve own size.
    let width = match style.width {
        Dimension::Px(px) => px,
        Dimension::Percent(pct) => available_w * pct / 100.0,
        Dimension::Auto => available_w - margin.horizontal(),
    };
    let height = match style.height {
        Dimension::Px(px) => px,
        Dimension::Percent(pct) => available_h * pct / 100.0,
        Dimension::Auto => available_h - margin.vertical(),
    };

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
    let inner_w = width - padding.horizontal();
    let inner_h = height - padding.vertical();
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

    let main_available = if is_column {
        inner_h - total_gap
    } else {
        inner_w - total_gap
    };

    // First pass: measure fixed-size children and collect flex-grow totals.
    let mut total_flex_grow = 0.0f32;
    let mut fixed_main = 0.0f32;
    let mut child_sizes: Vec<f32> = vec![0.0; node.children.len()];

    for &idx in &visible_children {
        let child_style = &node.children[idx].computed_style;
        let child_main = if is_column {
            match child_style.height {
                Dimension::Px(px) => {
                    child_sizes[idx] = px;
                    fixed_main += px;
                    continue;
                }
                Dimension::Percent(pct) => {
                    let s = main_available * pct / 100.0;
                    child_sizes[idx] = s;
                    fixed_main += s;
                    continue;
                }
                Dimension::Auto => 0.0,
            }
        } else {
            match child_style.width {
                Dimension::Px(px) => {
                    child_sizes[idx] = px;
                    fixed_main += px;
                    continue;
                }
                Dimension::Percent(pct) => {
                    let s = main_available * pct / 100.0;
                    child_sizes[idx] = s;
                    fixed_main += s;
                    continue;
                }
                Dimension::Auto => 0.0,
            }
        };
        let _ = child_main;
        total_flex_grow += child_style.flex_grow.max(0.0);
    }

    // Second pass: distribute remaining space to flex-grow children.
    let remaining = (main_available - fixed_main).max(0.0);
    for &idx in &visible_children {
        if child_sizes[idx] > 0.0 {
            continue;
        }
        let grow = node.children[idx].computed_style.flex_grow;
        if total_flex_grow > 0.0 && grow > 0.0 {
            child_sizes[idx] = remaining * (grow / total_flex_grow);
        } else {
            // Auto-sized with no flex-grow: give equal share of remaining.
            let auto_count = visible_children
                .iter()
                .filter(|&&i| child_sizes[i] == 0.0)
                .count() as f32;
            if auto_count > 0.0 {
                child_sizes[idx] = remaining / auto_count;
            }
        }
    }

    // Third pass: position children.
    let mut cursor = 0.0f32;
    for &idx in &visible_children {
        let child_main_size = child_sizes[idx];
        let (cx, cy, cw, ch) = if is_column {
            (inner_x, inner_y + cursor, inner_w, child_main_size)
        } else {
            (inner_x + cursor, inner_y, child_main_size, inner_h)
        };

        layout_node(&mut node.children[idx], cx, cy, cw, ch);
        cursor += child_main_size + style.gap;
    }

    // If height is Auto for column containers, shrink to fit children.
    if matches!(node.computed_style.height, Dimension::Auto) && is_column {
        let content_height = cursor - style.gap + padding.vertical();
        node.layout.height = content_height.max(0.0);
    }
    if matches!(node.computed_style.width, Dimension::Auto) && !is_column {
        let content_width = cursor - style.gap + padding.horizontal();
        node.layout.width = content_width.max(0.0);
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
