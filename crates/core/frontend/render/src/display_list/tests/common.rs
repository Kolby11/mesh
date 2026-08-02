use super::super::*;
use mesh_core_elements::style::Color;
use mesh_core_elements::{NodeId, WidgetNode};

pub(super) fn command_debugs(commands: &[DisplayPaintCommand], ids: &[NodeId]) -> Vec<String> {
    commands
        .iter()
        .filter(|command| ids.contains(&command.node.id))
        .map(|command| format!("{command:?}"))
        .collect()
}

pub(super) fn node(id: NodeId, tag: &str, x: f32, y: f32, width: f32, height: f32) -> WidgetNode {
    let mut node = WidgetNode::new(tag);
    node.id = id;
    node.layout.x = x;
    node.layout.y = y;
    node.layout.width = width;
    node.layout.height = height;
    node.computed_style.background_color = Color {
        r: 10,
        g: 20,
        b: 30,
        a: 255,
    };
    node
}

pub(super) fn display_entry_benchmark_tree(rows: usize, cols: usize) -> WidgetNode {
    let mut root = node(
        1,
        "column",
        0.0,
        0.0,
        (cols as f32) * 12.0,
        (rows as f32) * 8.0,
    );
    let mut id = 2;
    for row_index in 0..rows {
        let mut row = node(
            id,
            "row",
            0.0,
            (row_index as f32) * 8.0,
            (cols as f32) * 12.0,
            8.0,
        );
        id += 1;
        for col_index in 0..cols {
            let mut cell = node(id, "text", (col_index as f32) * 12.0, 0.0, 10.0, 8.0);
            cell.attributes
                .insert("content".into(), format!("{row_index}:{col_index}"));
            row.children.push(cell);
            id += 1;
        }
        root.children.push(row);
    }
    root
}

pub(super) fn child_popup_benchmark_tree(rows: usize, cols: usize) -> WidgetNode {
    let mut root = node(1, "popover", 300.0, 180.0, 200.0, 120.0);
    let mut id = 2;
    for row in 0..rows {
        for col in 0..cols {
            let mut child = node(
                id,
                "box",
                4.0 + col as f32 * 19.0,
                4.0 + row as f32 * 18.0,
                16.0,
                14.0,
            );
            child.computed_style.background_color = Color {
                r: (20 + row * 9) as u8,
                g: (30 + col * 7) as u8,
                b: 120,
                a: 255,
            };
            root.children.push(child);
            id += 1;
        }
    }
    root
}
