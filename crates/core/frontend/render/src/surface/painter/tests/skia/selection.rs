use super::super::super::*;
use super::super::common::*;
use crate::display_list::{
    DamageRect, DisplayListRepaintPolicy, DisplayPaintCommandKind, DisplayPaintContent,
    RetainedDisplayList,
};
use crate::{RenderObjectDirtySummary, build_focused_proof_snapshot};
use mesh_core_elements::layout::LayoutRect;
use mesh_core_frontend::compile_frontend_module;
use mesh_core_theme::default_theme;
use std::path::PathBuf;

#[test]
fn selection_paint_uses_selection_colors() {
    let mut root = node(
        "box",
        LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 160.0,
            height: 60.0,
        },
        Color::TRANSPARENT,
    );
    let mut text = text_node(
        "selection proof text",
        0.0,
        0.0,
        160.0,
        60.0,
        Color::from_hex("#111111").unwrap(),
    );
    text.attributes
        .insert("_mesh_selection_background".into(), "#00ff00".into());
    text.attributes
        .insert("_mesh_selection_foreground".into(), "#ff00ff".into());
    text.attributes
        .insert("_mesh_selection_anchor_x".into(), "0.00".into());
    text.attributes
        .insert("_mesh_selection_anchor_y".into(), "0.00".into());
    text.attributes
        .insert("_mesh_selection_focus_x".into(), "1000.00".into());
    text.attributes
        .insert("_mesh_selection_focus_y".into(), "1000.00".into());
    text.attributes
        .insert("_mesh_selection_text_x".into(), "0.00".into());
    text.attributes
        .insert("_mesh_selection_text_y".into(), "0.00".into());
    root.children = vec![text].into();

    let mut buffer = PixelBuffer::new(180, 80);
    FrontendRenderEngine::new().render_tree(&root, &mut buffer, 1.0);

    let mut saw_selection_background = false;
    let mut saw_selection_foreground = false;
    for y in 0..buffer.height {
        for x in 0..buffer.width {
            let color = pixel(&buffer, x, y);
            if color == Color::from_hex("#00ff00").unwrap() {
                saw_selection_background = true;
            }
            if color == Color::from_hex("#ff00ff").unwrap() {
                saw_selection_foreground = true;
            }
        }
    }

    assert!(saw_selection_background);
    assert!(saw_selection_foreground);
}

#[test]
fn phase44_selection_paint_and_proof_use_theme_colors() {
    let mut root = node(
        "box",
        LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 160.0,
            height: 60.0,
        },
        Color::TRANSPARENT,
    );
    let mut text = text_node(
        "selection proof text",
        0.0,
        0.0,
        160.0,
        60.0,
        Color::from_hex("#111111").unwrap(),
    );
    text.attributes
        .insert("_mesh_selection_background".into(), "#00ff00".into());
    text.attributes
        .insert("_mesh_selection_foreground".into(), "#ff00ff".into());
    text.attributes
        .insert("_mesh_selection_anchor_x".into(), "0.00".into());
    text.attributes
        .insert("_mesh_selection_anchor_y".into(), "0.00".into());
    text.attributes
        .insert("_mesh_selection_focus_x".into(), "1000.00".into());
    text.attributes
        .insert("_mesh_selection_focus_y".into(), "1000.00".into());
    text.attributes
        .insert("_mesh_selection_text_x".into(), "0.00".into());
    text.attributes
        .insert("_mesh_selection_text_y".into(), "0.00".into());
    root.children = vec![text].into();

    let mut list = RetainedDisplayList::default();
    let metrics = list.update(&root, 180, 80, true, true);
    let selected = list.select_paint_commands(
        Some(DamageRect {
            x: 0,
            y: 0,
            width: 180,
            height: 80,
        }),
        DisplayListRepaintPolicy::FullSurface,
    );

    let mut buffer = PixelBuffer::new(180, 80);
    FrontendRenderEngine::new().render_selected_display_list_for_module(
        &selected,
        &mut buffer,
        1.0,
        None,
        None,
        None,
    );

    let mut saw_selection_background = false;
    let mut saw_selection_foreground = false;
    for y in 0..buffer.height {
        for x in 0..buffer.width {
            let color = pixel(&buffer, x, y);
            if color == Color::from_hex("#00ff00").unwrap() {
                saw_selection_background = true;
            }
            if color == Color::from_hex("#ff00ff").unwrap() {
                saw_selection_foreground = true;
            }
        }
    }
    assert!(saw_selection_background);
    assert!(saw_selection_foreground);

    let proof = build_focused_proof_snapshot(
        &root,
        RenderObjectDirtySummary::default(),
        metrics,
        &selected,
    );
    let text = proof
        .nodes
        .iter()
        .find_map(|node| node.parley_text.as_ref())
        .expect("text proof evidence");
    assert_eq!(text.selection_background.as_deref(), Some("#00ff00"));
    assert_eq!(text.selection_foreground.as_deref(), Some("#ff00ff"));
}

#[test]
fn selection_paint_does_not_bleed_into_neighbors() {
    let mut root = node(
        "box",
        LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 220.0,
            height: 80.0,
        },
        Color::TRANSPARENT,
    );
    let mut selected = text_node(
        "selected",
        0.0,
        0.0,
        100.0,
        40.0,
        Color::from_hex("#111111").unwrap(),
    );
    selected
        .attributes
        .insert("_mesh_selection_background".into(), "#00ff00".into());
    selected
        .attributes
        .insert("_mesh_selection_foreground".into(), "#ff00ff".into());
    selected
        .attributes
        .insert("_mesh_selection_anchor_x".into(), "0.00".into());
    selected
        .attributes
        .insert("_mesh_selection_anchor_y".into(), "0.00".into());
    selected
        .attributes
        .insert("_mesh_selection_focus_x".into(), "1000.00".into());
    selected
        .attributes
        .insert("_mesh_selection_focus_y".into(), "1000.00".into());
    selected
        .attributes
        .insert("_mesh_selection_text_x".into(), "0.00".into());
    selected
        .attributes
        .insert("_mesh_selection_text_y".into(), "0.00".into());

    let neighbor = text_node(
        "neighbor",
        120.0,
        0.0,
        100.0,
        40.0,
        Color::from_hex("#111111").unwrap(),
    );
    root.children = vec![selected, neighbor].into();

    let mut buffer = PixelBuffer::new(240, 80);
    FrontendRenderEngine::new().render_tree(&root, &mut buffer, 1.0);

    for y in 0..40 {
        for x in 120..220 {
            assert_ne!(
                pixel(&buffer, x, y),
                Color::from_hex("#00ff00").unwrap(),
                "selection background should stay inside the selected text node"
            );
        }
    }
}

#[test]
fn selection_fixture_preview_tree_paints_nonempty_surface() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../..")
        .canonicalize()
        .unwrap();
    let module_dir = root.join("modules/frontend/text-selection-proof");
    let loaded = mesh_core_module::manifest::load_manifest(&module_dir).unwrap();
    let compiled = compile_frontend_module(&loaded.manifest, &module_dir).unwrap();
    let tree = compiled.build_preview_tree(&default_theme(), 360, 176);

    let mut buffer = PixelBuffer::new(380, 196);
    FrontendRenderEngine::new().render_tree(&tree, &mut buffer, 1.0);

    let has_visible_pixels = buffer.data.chunks_exact(4).any(|px| px[3] != 0);
    assert!(
        has_visible_pixels,
        "proof fixture should paint visible output"
    );
}

#[test]
fn retained_replay_batches_adjacent_non_content_nodes() {
    let mut root = node(
        "box",
        LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 80.0,
            height: 40.0,
        },
        Color::TRANSPARENT,
    );
    root.children = vec![
        node(
            "box",
            LayoutRect {
                x: 2.0,
                y: 2.0,
                width: 18.0,
                height: 18.0,
            },
            Color::from_hex("#224466").unwrap(),
        ),
        node(
            "box",
            LayoutRect {
                x: 24.0,
                y: 2.0,
                width: 18.0,
                height: 18.0,
            },
            Color::from_hex("#446622").unwrap(),
        ),
        text_node(
            "content boundary",
            2.0,
            24.0,
            72.0,
            12.0,
            Color::from_hex("#f0f0f0").unwrap(),
        ),
    ]
    .into();

    let mut list = RetainedDisplayList::default();
    list.update(&root, 80, 40, true, true);
    let selected = list.select_paint_commands(
        Some(DamageRect {
            x: 0,
            y: 0,
            width: 80,
            height: 40,
        }),
        DisplayListRepaintPolicy::FullSurface,
    );
    let replay_commands: Vec<_> = selected
        .iter()
        .filter(|command| {
            command.kind == DisplayPaintCommandKind::Node
                && matches!(command.node.content, DisplayPaintContent::None)
        })
        .cloned()
        .collect();
    assert!(
        replay_commands.len() >= 2,
        "expected at least two adjacent non-content node commands to replay"
    );

    let backend = RecordingPaintBackend::default();
    let recorded = backend.clone();
    let engine = FrontendRenderEngine::with_paint_backend(Box::new(backend));
    let mut buffer = PixelBuffer::new(80, 40);
    engine.render_display_list_for_module(&replay_commands, &mut buffer, 1.0, None, None, None);

    let call_sizes = recorded.execute_call_sizes();
    assert_eq!(
        call_sizes,
        vec![2],
        "expected exactly one non-empty batched display command execution, got {call_sizes:?}"
    );
}
