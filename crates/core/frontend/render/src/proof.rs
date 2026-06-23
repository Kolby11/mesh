use mesh_core_elements::{NodeId, WidgetNode};

use crate::display_list::DisplayPaintContent;
use crate::{
    DamageRect, DisplayListMetrics, DisplayPaintCommand, RenderObjectDirtySummary,
    SelectedDisplayListPaint,
};

#[derive(Debug, Clone, Default)]
pub struct FocusedProofSnapshot {
    pub nodes: Vec<FocusedProofNode>,
    pub paint: Vec<FocusedPaintEvidence>,
    pub accessibility: Vec<FocusedAccessibilityEvidence>,
    pub dirty: FocusedDirtyEvidence,
    pub damage: FocusedDamageEvidence,
    pub diagnostics: Vec<FocusedProofDiagnostic>,
}

#[derive(Debug, Clone)]
pub struct FocusedProofNode {
    pub node_id: NodeId,
    pub stable_node_id: String,
    pub taffy_layout: FocusedLayoutEvidence,
    pub parley_text: Option<FocusedTextEvidence>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FocusedLayoutEvidence {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FocusedTextEvidence {
    pub content: String,
    pub parley_text: String,
    pub selection_background: Option<String>,
    pub selection_foreground: Option<String>,
    /// Selection anchor evidence. Default builds preserve raw surface-space
    /// attributes; `renderer-parley` builds prefer Parley cursor geometry and
    /// fall back to raw attributes only when Parley cannot produce a cursor.
    pub selection_anchor: Option<(f32, f32)>,
    /// Selection focus evidence. Coordinate space matches `selection_anchor`.
    pub selection_focus: Option<(f32, f32)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusedPaintEvidence {
    pub node_id: NodeId,
    pub stable_node_id: String,
    pub display_slot: &'static str,
    /// True when `renderer-anyrender` encoded at least one scene op for this
    /// command. Always false without the feature; software paint stays
    /// authoritative for output.
    pub anyrender_encoded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusedAccessibilityEvidence {
    pub node_id: NodeId,
    pub stable_node_id: String,
    pub accesskit_node_id: String,
    pub role: String,
    pub label: Option<String>,
}

impl FocusedAccessibilityEvidence {
    pub fn accesskit_node_id_for(node_id: NodeId) -> String {
        format!("accesskit_node_id::{node_id}")
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FocusedDirtyEvidence {
    pub geometry: usize,
    pub material: usize,
    pub text: usize,
    pub accessibility: usize,
}

impl From<RenderObjectDirtySummary> for FocusedDirtyEvidence {
    fn from(value: RenderObjectDirtySummary) -> Self {
        Self {
            geometry: value.geometry,
            material: value.material,
            text: value.text,
            accessibility: value.accessibility,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FocusedDamageEvidence {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub area: u64,
    pub full_surface: bool,
}

impl From<DamageRect> for FocusedDamageEvidence {
    fn from(value: DamageRect) -> Self {
        Self {
            x: value.x,
            y: value.y,
            width: value.width,
            height: value.height,
            area: value.area(),
            full_surface: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusedProofDiagnostic {
    pub node_id: Option<NodeId>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusedAccessKitUpdate {
    pub root_id: String,
    pub nodes: Vec<FocusedAccessibilityEvidence>,
}

pub fn build_focused_proof_snapshot(
    root: &WidgetNode,
    render_dirty: RenderObjectDirtySummary,
    display_metrics: DisplayListMetrics,
    selected_paint: &SelectedDisplayListPaint<'_>,
) -> FocusedProofSnapshot {
    let mut snapshot = FocusedProofSnapshot {
        dirty: render_dirty.into(),
        damage: display_metrics.damage_rect.into(),
        ..FocusedProofSnapshot::default()
    };
    snapshot.damage.full_surface = display_metrics.full_surface_damage;

    collect_focused_nodes(root, &mut snapshot);
    snapshot.paint = selected_paint
        .iter()
        .map(|cmd| focused_paint_evidence(cmd, &mut snapshot.diagnostics))
        .collect();
    snapshot
}

pub fn build_accesskit_update(snapshot: &FocusedProofSnapshot) -> FocusedAccessKitUpdate {
    FocusedAccessKitUpdate {
        root_id: snapshot
            .accessibility
            .first()
            .map(|node| node.accesskit_node_id.clone())
            .unwrap_or_else(|| "accesskit_node_id::empty".to_string()),
        nodes: snapshot.accessibility.clone(),
    }
}

fn collect_focused_nodes(node: &WidgetNode, snapshot: &mut FocusedProofSnapshot) {
    snapshot.nodes.push(FocusedProofNode {
        node_id: node.id,
        stable_node_id: node.id.to_string(),
        taffy_layout: FocusedLayoutEvidence {
            x: node.layout.x,
            y: node.layout.y,
            width: node.layout.width,
            height: node.layout.height,
        },
        parley_text: focused_text_evidence(node, &mut snapshot.diagnostics),
    });

    snapshot.accessibility.push(FocusedAccessibilityEvidence {
        node_id: node.id,
        stable_node_id: node.id.to_string(),
        accesskit_node_id: FocusedAccessibilityEvidence::accesskit_node_id_for(node.id),
        role: node
            .attributes
            .get("role")
            .cloned()
            .unwrap_or_else(|| "generic".to_string()),
        label: node
            .attributes
            .get("aria-label")
            .cloned()
            .or_else(|| node.attributes.get("content").cloned()),
    });

    if node.layout.width == 0.0 || node.layout.height == 0.0 {
        snapshot.diagnostics.push(FocusedProofDiagnostic {
            node_id: Some(node.id),
            message: "focused proof node has zero-size layout".to_string(),
        });
    }

    for child in &node.children {
        collect_focused_nodes(child, snapshot);
    }
}

fn focused_text_evidence(
    node: &WidgetNode,
    diagnostics: &mut Vec<FocusedProofDiagnostic>,
) -> Option<FocusedTextEvidence> {
    let content = node.attributes.get("content")?.clone();

    #[cfg(feature = "renderer-parley")]
    let (parley_text, selection_anchor, selection_focus) = {
        let (shaped, anchor, focus) = crate::parley_adapter::shape_text_with_selection_evidence(
            node,
            content.as_str(),
            diagnostics,
        );
        // Parley-derived anchor/focus take precedence when the feature is on;
        // fall back to the raw attribute values only when Parley returned None
        // (e.g. attrs absent — semantics identical to default build).
        let anchor = anchor.or_else(|| {
            selection_point(node, "_mesh_selection_anchor_x", "_mesh_selection_anchor_y")
        });
        let focus = focus.or_else(|| {
            selection_point(node, "_mesh_selection_focus_x", "_mesh_selection_focus_y")
        });
        (shaped, anchor, focus)
    };

    #[cfg(not(feature = "renderer-parley"))]
    let (parley_text, selection_anchor, selection_focus) = {
        let _ = &diagnostics; // unused without feature
        (
            format!("parley_text::{content}::shape=line_break_bidi_align"),
            selection_point(node, "_mesh_selection_anchor_x", "_mesh_selection_anchor_y"),
            selection_point(node, "_mesh_selection_focus_x", "_mesh_selection_focus_y"),
        )
    };

    Some(FocusedTextEvidence {
        parley_text,
        content,
        selection_background: node.attributes.get("_mesh_selection_background").cloned(),
        selection_foreground: node.attributes.get("_mesh_selection_foreground").cloned(),
        selection_anchor,
        selection_focus,
    })
}

fn selection_point(node: &WidgetNode, x_key: &str, y_key: &str) -> Option<(f32, f32)> {
    let x = node.attributes.get(x_key)?.parse::<f32>().ok()?;
    let y = node.attributes.get(y_key)?.parse::<f32>().ok()?;
    Some((x, y))
}

fn focused_paint_evidence(
    command: &DisplayPaintCommand,
    diagnostics: &mut Vec<FocusedProofDiagnostic>,
) -> FocusedPaintEvidence {
    #[cfg(feature = "renderer-anyrender")]
    let anyrender_encoded =
        crate::anyrender_adapter::encode_command_to_scene(command, diagnostics) > 0;
    #[cfg(not(feature = "renderer-anyrender"))]
    let anyrender_encoded = {
        let _ = diagnostics;
        false
    };

    FocusedPaintEvidence {
        node_id: command.node.id,
        stable_node_id: command.node.id.to_string(),
        display_slot: display_slot_for_command(command),
        anyrender_encoded,
    }
}

fn display_slot_for_command(command: &DisplayPaintCommand) -> &'static str {
    match &command.node.content {
        DisplayPaintContent::Text(_) => "Text",
        DisplayPaintContent::Icon(_) => "Icon",
        DisplayPaintContent::Checkmark(_) => "Checkmark",
        DisplayPaintContent::Slider(_) | DisplayPaintContent::Input(_) => "Generic",
        DisplayPaintContent::None => {
            if command.node.style.border_width.top > 0.0
                || command.node.style.border_width.right > 0.0
                || command.node.style.border_width.bottom > 0.0
                || command.node.style.border_width.left > 0.0
            {
                "Border"
            } else if command.node.style.background_color.a > 0 {
                "Background"
            } else {
                "Generic"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use mesh_core_elements::WidgetNode;
    use mesh_core_elements::layout::LayoutRect;
    use mesh_core_elements::style::{Color, Dimension};

    use crate::{DisplayListRepaintPolicy, RetainedDisplayList};

    use super::*;

    fn node(tag: &str, id: NodeId, layout: LayoutRect) -> WidgetNode {
        let mut node = WidgetNode::new(tag);
        node.id = id;
        node.layout = layout;
        node.computed_style.width = Dimension::Px(layout.width);
        node.computed_style.height = Dimension::Px(layout.height);
        node
    }

    fn proof_snapshot(root: &WidgetNode, dirty: RenderObjectDirtySummary) -> FocusedProofSnapshot {
        let mut list = RetainedDisplayList::default();
        let metrics = list.update(root, 200, 100, true, true);
        let selected = list.select_paint_commands(
            Some(DamageRect {
                x: 0,
                y: 0,
                width: 200,
                height: 100,
            }),
            DisplayListRepaintPolicy::FullSurface,
        );
        build_focused_proof_snapshot(root, dirty, metrics, &selected)
    }

    #[test]
    fn proof_snapshot_preserves_node_identity_and_layout() {
        let mut root = node(
            "box",
            42,
            LayoutRect {
                x: 1.0,
                y: 2.0,
                width: 30.0,
                height: 40.0,
            },
        );
        root.computed_style.background_color = Color::WHITE;
        let snapshot = proof_snapshot(&root, RenderObjectDirtySummary::default());

        assert_eq!(snapshot.nodes[0].node_id, 42);
        assert_eq!(snapshot.nodes[0].stable_node_id, "42");
        assert_eq!(snapshot.nodes[0].taffy_layout.width, 30.0);
        assert_eq!(snapshot.nodes[0].taffy_layout.height, 40.0);
        assert_eq!(snapshot.paint[0].stable_node_id, "42");
    }

    #[test]
    #[cfg(not(feature = "renderer-anyrender"))]
    fn proof_snapshot_anyrender_encoded_false_without_feature() {
        let mut root = node(
            "box",
            42,
            LayoutRect {
                x: 0.0,
                y: 0.0,
                width: 30.0,
                height: 40.0,
            },
        );
        root.computed_style.background_color = Color::WHITE;
        let snapshot = proof_snapshot(&root, RenderObjectDirtySummary::default());

        assert!(!snapshot.paint.is_empty());
        assert!(
            snapshot.paint.iter().all(|paint| !paint.anyrender_encoded),
            "anyrender_encoded must be false on the default build"
        );
    }

    #[test]
    #[cfg(feature = "renderer-anyrender")]
    fn proof_snapshot_anyrender_encoded_true_for_background_command() {
        let mut root = node(
            "box",
            42,
            LayoutRect {
                x: 0.0,
                y: 0.0,
                width: 30.0,
                height: 40.0,
            },
        );
        root.computed_style.background_color = Color::WHITE;
        let snapshot = proof_snapshot(&root, RenderObjectDirtySummary::default());

        assert!(!snapshot.paint.is_empty());
        assert!(
            snapshot.paint.iter().any(|paint| paint.anyrender_encoded),
            "at least one background command must be encoded"
        );
    }

    #[test]
    fn proof_snapshot_maps_render_dirty_categories() {
        let root = node(
            "box",
            7,
            LayoutRect {
                x: 0.0,
                y: 0.0,
                width: 20.0,
                height: 20.0,
            },
        );
        let dirty = RenderObjectDirtySummary {
            geometry: 1,
            material: 2,
            text: 3,
            accessibility: 4,
            ..RenderObjectDirtySummary::default()
        };

        let snapshot = proof_snapshot(&root, dirty);

        assert_eq!(snapshot.dirty.geometry, 1);
        assert_eq!(snapshot.dirty.material, 2);
        assert_eq!(snapshot.dirty.text, 3);
        assert_eq!(snapshot.dirty.accessibility, 4);
    }

    #[test]
    fn proof_snapshot_derives_accesskit_node_ids() {
        let root = node(
            "button",
            99,
            LayoutRect {
                x: 0.0,
                y: 0.0,
                width: 20.0,
                height: 20.0,
            },
        );
        let snapshot = proof_snapshot(&root, RenderObjectDirtySummary::default());

        assert_eq!(snapshot.accessibility[0].node_id, 99);
        assert_eq!(snapshot.accessibility[0].stable_node_id, "99");
        assert_eq!(
            snapshot.accessibility[0].accesskit_node_id,
            "accesskit_node_id::99"
        );
    }

    #[test]
    fn proof_snapshot_preserves_theme_owned_selection_payload() {
        let mut text = node(
            "text",
            11,
            LayoutRect {
                x: 0.0,
                y: 0.0,
                width: 160.0,
                height: 40.0,
            },
        );
        text.attributes
            .insert("content".into(), "selected text".into());
        text.attributes
            .insert("_mesh_selection_background".into(), "#112233".into());
        text.attributes
            .insert("_mesh_selection_foreground".into(), "#ddeeff".into());
        text.attributes
            .insert("_mesh_selection_anchor_x".into(), "2.0".into());
        text.attributes
            .insert("_mesh_selection_anchor_y".into(), "3.0".into());
        text.attributes
            .insert("_mesh_selection_focus_x".into(), "8.0".into());
        text.attributes
            .insert("_mesh_selection_focus_y".into(), "9.0".into());
        let snapshot = proof_snapshot(&text, RenderObjectDirtySummary::default());
        let proof_text = snapshot.nodes[0]
            .parley_text
            .as_ref()
            .expect("text evidence");

        // Theme-owned colors always pass through unmodified.
        assert_eq!(proof_text.selection_background.as_deref(), Some("#112233"));
        assert_eq!(proof_text.selection_foreground.as_deref(), Some("#ddeeff"));

        // Default build: raw attribute passthrough (byte-identical to pre-Phase-48).
        // Feature-on build: coords are Parley-derived (may differ from raw values).
        #[cfg(not(feature = "renderer-parley"))]
        {
            assert_eq!(proof_text.selection_anchor, Some((2.0, 3.0)));
            assert_eq!(proof_text.selection_focus, Some((8.0, 9.0)));
        }
        #[cfg(feature = "renderer-parley")]
        {
            // With Parley, selection coords are cursor-geometry derived.
            // Just assert they are present (Some) — exact values depend on font metrics.
            assert!(proof_text.selection_anchor.is_some());
            assert!(proof_text.selection_focus.is_some());
        }
    }

    #[test]
    fn proof_snapshot_builds_accesskit_update_from_retained_nodes() {
        let mut root = node(
            "box",
            12,
            LayoutRect {
                x: 0.0,
                y: 0.0,
                width: 40.0,
                height: 40.0,
            },
        );
        root.children.push(node(
            "text",
            13,
            LayoutRect {
                x: 0.0,
                y: 0.0,
                width: 20.0,
                height: 20.0,
            },
        ));
        let snapshot = proof_snapshot(&root, RenderObjectDirtySummary::default());

        let update = build_accesskit_update(&snapshot);

        assert!(update.root_id.starts_with("accesskit_node_id::"));
        assert!(!update.nodes.is_empty());
        for node in update.nodes {
            assert_eq!(node.stable_node_id, node.node_id.to_string());
            assert!(!node.accesskit_node_id.is_empty());
        }
    }

    #[test]
    #[cfg(not(feature = "renderer-parley"))]
    fn focused_text_evidence_default_build_preserves_placeholder() {
        let mut node = WidgetNode::new("text");
        node.attributes
            .insert("content".to_string(), "World".to_string());
        node.layout = LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 18.0,
        };
        let mut diagnostics: Vec<FocusedProofDiagnostic> = Vec::new();
        let evidence = focused_text_evidence(&node, &mut diagnostics).expect("evidence");
        assert_eq!(
            evidence.parley_text,
            "parley_text::World::shape=line_break_bidi_align"
        );
        assert!(diagnostics.is_empty());
    }

    #[test]
    #[cfg(feature = "renderer-parley")]
    fn focused_text_evidence_with_parley_feature_returns_shaped_summary() {
        let mut node = WidgetNode::new("text");
        node.attributes
            .insert("content".to_string(), "World".to_string());
        node.layout = LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 18.0,
        };
        node.computed_style.font_size = 14.0;
        node.computed_style.font_weight = 400;
        let mut diagnostics: Vec<FocusedProofDiagnostic> = Vec::new();
        let evidence = focused_text_evidence(&node, &mut diagnostics).expect("evidence");
        assert_ne!(
            evidence.parley_text, "parley_text::World::shape=line_break_bidi_align",
            "feature-on path must not return the legacy placeholder"
        );
        assert!(
            evidence.parley_text.starts_with("parley::lines=")
                || evidence.parley_text.contains("::no_fonts"),
            "unexpected parley_text: {}",
            evidence.parley_text
        );
    }

    #[test]
    #[cfg(not(feature = "renderer-parley"))]
    fn focused_text_evidence_default_build_selection_is_raw_attribute() {
        let mut node = WidgetNode::new("text");
        node.attributes
            .insert("content".to_string(), "Hi".to_string());
        node.attributes
            .insert("_mesh_selection_anchor_x".to_string(), "12.5".to_string());
        node.attributes
            .insert("_mesh_selection_anchor_y".to_string(), "3.0".to_string());
        node.layout = LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 18.0,
        };
        let mut diagnostics: Vec<FocusedProofDiagnostic> = Vec::new();
        let evidence = focused_text_evidence(&node, &mut diagnostics).expect("evidence");
        assert_eq!(evidence.selection_anchor, Some((12.5, 3.0)));
    }

    #[test]
    #[cfg(feature = "renderer-parley")]
    fn focused_text_evidence_with_parley_feature_uses_cursor_geometry() {
        let mut node = WidgetNode::new("text");
        node.attributes
            .insert("content".to_string(), "Hello World".to_string());
        node.attributes
            .insert("_mesh_selection_anchor_x".to_string(), "20".to_string());
        node.attributes
            .insert("_mesh_selection_anchor_y".to_string(), "8".to_string());
        node.attributes
            .insert("_mesh_selection_focus_x".to_string(), "60".to_string());
        node.attributes
            .insert("_mesh_selection_focus_y".to_string(), "8".to_string());
        node.layout = LayoutRect {
            x: 10.0,
            y: 5.0,
            width: 200.0,
            height: 18.0,
        };
        node.computed_style.font_size = 14.0;
        node.computed_style.font_weight = 400;
        node.computed_style.padding.left = 4.0;
        node.computed_style.padding.top = 2.0;
        let mut diagnostics: Vec<FocusedProofDiagnostic> = Vec::new();
        let evidence = focused_text_evidence(&node, &mut diagnostics).expect("evidence");

        if evidence.parley_text.contains("::no_fonts") {
            // CI without fonts: empty Parley layouts return None for cursor
            // evidence, so focused_text_evidence falls back to raw attributes.
            assert_eq!(evidence.selection_anchor, Some((20.0, 8.0)));
            assert_eq!(evidence.selection_focus, Some((60.0, 8.0)));
        } else {
            let a = evidence.selection_anchor.expect("anchor");
            assert!(
                a != (20.0, 8.0),
                "Parley should have transformed (20,8) into text-local cursor coords, got {a:?}"
            );
        }
    }

    #[test]
    #[cfg(feature = "renderer-parley")]
    fn focused_text_evidence_selection_colors_preserved() {
        let mut node = WidgetNode::new("text");
        node.attributes
            .insert("content".to_string(), "Hi".to_string());
        node.attributes.insert(
            "_mesh_selection_background".to_string(),
            "rgba(0,0,255,0.5)".to_string(),
        );
        node.attributes.insert(
            "_mesh_selection_foreground".to_string(),
            "white".to_string(),
        );
        node.layout = LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 18.0,
        };
        let mut diagnostics: Vec<FocusedProofDiagnostic> = Vec::new();
        let evidence = focused_text_evidence(&node, &mut diagnostics).expect("evidence");
        assert_eq!(
            evidence.selection_background.as_deref(),
            Some("rgba(0,0,255,0.5)")
        );
        assert_eq!(evidence.selection_foreground.as_deref(), Some("white"));
    }
}
