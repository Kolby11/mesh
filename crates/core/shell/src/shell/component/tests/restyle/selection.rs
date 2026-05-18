use super::*;
use mesh_core_elements::style::Overflow;
use std::fs;

#[test]
fn selection_boundaries_clear_when_selected_node_is_removed() {
    let mut component = test_frontend_component("<template><box /></template>");
    component.selection = Some(TextSelectionState {
        anchor: TextSelectionPoint {
            node_key: "root/0".into(),
            x: 4.0,
            y: 4.0,
        },
        focus: TextSelectionPoint {
            node_key: "root/0".into(),
            x: 24.0,
            y: 4.0,
        },
        dragging: false,
    });
    component.prune_stale_interaction_targets(&root_with(vec![]));

    assert!(
        component.selection.is_none(),
        "selection must clear when the selected node disappears during rebuild"
    );
}

#[test]
fn selection_boundaries_clear_when_surface_hides() {
    let mut component = test_frontend_component("<template><box /></template>");
    component.selection = Some(TextSelectionState {
        anchor: TextSelectionPoint {
            node_key: "root/0".into(),
            x: 4.0,
            y: 4.0,
        },
        focus: TextSelectionPoint {
            node_key: "root/0".into(),
            x: 16.0,
            y: 4.0,
        },
        dragging: false,
    });

    component
        .handle_core_event(&CoreEvent::SurfaceVisibilityChanged {
            surface_id: component.surface_id().to_string(),
            visible: false,
        })
        .unwrap();

    assert!(
        component.selection.is_none(),
        "surface hide should clear shell-owned selection state"
    );
}

#[test]
fn selection_clipboard_returns_visible_selected_text_for_ctrl_c() {
    let mut component = test_frontend_component("<template><box /></template>");
    component.last_tree = Some(root_with(vec![text_node(
        "root/0", 0.0, 0.0, 180.0, 40.0, true,
    )]));
    component.selection = Some(TextSelectionState {
        anchor: TextSelectionPoint {
            node_key: "root/0".into(),
            x: 0.0,
            y: 0.0,
        },
        focus: TextSelectionPoint {
            node_key: "root/0".into(),
            x: 1000.0,
            y: 1000.0,
        },
        dragging: false,
    });

    let theme = default_theme();
    let requests = component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::KeyPressed {
                key: "c".into(),
                modifiers: KeyModifiers {
                    ctrl: true,
                    shift: false,
                    alt: false,
                },
            },
        )
        .unwrap();

    assert_eq!(requests.len(), 1);
    assert!(matches!(
        &requests[0],
        CoreRequest::WriteClipboard { text } if text == "Selectable text"
    ));
    assert!(
        component.selection.is_some(),
        "successful copy should leave the selection visible"
    );
}

#[test]
fn selection_clipboard_rejects_clipped_text_payloads() {
    let mut component = test_frontend_component("<template><box /></template>");
    let mut clipped = text_node("root/0", 0.0, 0.0, 80.0, 20.0, true);
    clipped.computed_style.overflow_x = Overflow::Hidden;
    component.last_tree = Some(root_with(vec![clipped]));
    component.selection = Some(TextSelectionState {
        anchor: TextSelectionPoint {
            node_key: "root/0".into(),
            x: 0.0,
            y: 0.0,
        },
        focus: TextSelectionPoint {
            node_key: "root/0".into(),
            x: 1000.0,
            y: 1000.0,
        },
        dragging: false,
    });

    let theme = default_theme();
    let requests = component
        .handle_input(
            &theme,
            240,
            160,
            ComponentInput::KeyPressed {
                key: "c".into(),
                modifiers: KeyModifiers {
                    ctrl: true,
                    shift: false,
                    alt: false,
                },
            },
        )
        .unwrap();

    assert!(
        requests.is_empty(),
        "Phase 10 should not copy hidden or clipped text"
    );
}

#[test]
fn selection_fixture_module_is_disabled_in_local_graph() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let package = fs::read_to_string(root.join("config/module.json")).unwrap();
    let json: serde_json::Value = serde_json::from_str(&package).unwrap();
    let module = &json["modules"]["@mesh/text-selection-proof"];
    assert_eq!(module["kind"], "frontend");
    assert_eq!(module["path"], "frontend/text-selection-proof");
    assert_eq!(module["enabled"], false);
}

#[test]
fn phase44_selection_restyle_keeps_focused_text_payload() {
    let mut component = test_frontend_component(
        r#"
<template>
  <text selectable="true">Selectable text</text>
</template>
"#,
    );
    component.selection = Some(TextSelectionState {
        anchor: TextSelectionPoint {
            node_key: "root/0".into(),
            x: 0.0,
            y: 0.0,
        },
        focus: TextSelectionPoint {
            node_key: "root/0".into(),
            x: 180.0,
            y: 20.0,
        },
        dragging: false,
    });

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(240, 160);
    component.paint(&theme, 240, 160, &mut buffer).unwrap();

    let proof = component
        .last_focused_proof_snapshot()
        .expect("selection paint should store focused proof snapshot");
    assert!(
        proof.nodes.iter().any(|node| {
            node.parley_text.as_ref().is_some_and(|text| {
                text.selection_background.is_some() && text.selection_foreground.is_some()
            })
        }),
        "focused proof should preserve shell-annotated selection colors"
    );
}
