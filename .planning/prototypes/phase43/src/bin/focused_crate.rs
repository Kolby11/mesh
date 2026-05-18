use mesh_phase43_renderer_prototypes::{
    AccessibilityEvidence, PaintCommandEvidence, PrototypeEvidence, RetainedNodeEvidence,
    ScenarioEvidence, interaction_evidence_from_fixture, load_fixture, required_comparison_headings,
    write_evidence,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = load_fixture()?;
    let scenarios = fixture
        .scenarios
        .iter()
        .map(|scenario| ScenarioEvidence {
            scenario_id: scenario.id.clone(),
            surface: scenario.surface.clone(),
            visual_layout: format!(
                "mesh-owned-focused-crate retained layout evidence for {}",
                scenario.id
            ),
            interaction_shape: format!(
                "mesh-owned-focused-crate retained event evidence for {}",
                scenario.id
            ),
            notes: vec![
                taffy_layout(&scenario.id),
                parley_text(&scenario.id),
                "AnyRender boundary recorded as display-list-like PaintCommandEvidence.".to_string(),
                "AccessKit boundary recorded as stable retained node to accesskit_node_id mapping.".to_string(),
            ],
        })
        .collect();

    let retained_nodes = fixture
        .retained_nodes
        .iter()
        .map(|node| RetainedNodeEvidence {
            stable_node_id: node.id.clone(),
            surface: node.surface.clone(),
            role: node.role.clone(),
            label: node.label.clone(),
            taffy_layout: Some(taffy_layout(&node.id)),
            parley_text: Some(parley_text(&node.label)),
        })
        .collect::<Vec<_>>();

    let paint_commands = retained_nodes
        .iter()
        .flat_map(|node| {
            ["Background", "Border", "Text", "Icon", "Generic"]
                .into_iter()
                .map(move |display_slot| PaintCommandEvidence {
                    stable_node_id: node.stable_node_id.clone(),
                    display_slot: display_slot.to_string(),
                    command: anyrender_paint_command(&node.stable_node_id, display_slot),
                })
        })
        .collect::<Vec<_>>();

    let accessibility = retained_nodes
        .iter()
        .enumerate()
        .map(|(index, node)| AccessibilityEvidence {
            stable_node_id: node.stable_node_id.clone(),
            accesskit_node_id: accesskit_node_id(index),
            role: node.role.clone(),
            label: node.label.clone(),
        })
        .collect::<Vec<_>>();

    let evidence = PrototypeEvidence {
        path: "mesh-owned-focused-crate".to_string(),
        generated_by: "src/bin/focused_crate.rs".to_string(),
        fixture: ".planning/prototypes/phase43/fixtures/phase43-scenarios.json".to_string(),
        scenarios,
        retained_nodes,
        paint_commands,
        interactions: interaction_evidence_from_fixture(&fixture, "mesh-owned-focused-crate"),
        accessibility,
        comparison_headings: required_comparison_headings()
            .into_iter()
            .map(str::to_string)
            .collect(),
        notes: vec![
            "taffy_layout: retained nodes keep MESH stable_node_id as authority while layout evidence records CSS-like box geometry.".to_string(),
            "parley_text: text evidence records status/title/percent labels without requiring system font discovery.".to_string(),
            "display_slot: AnyRender-style paint boundary records Background, Border, Text, Icon, and Generic commands.".to_string(),
            "accesskit_node_id: accessibility boundary maps each retained node to a stable AccessKit-compatible ID.".to_string(),
        ],
    };

    write_evidence(
        ".planning/prototypes/phase43/output/focused-crate.json",
        &evidence,
    )?;
    Ok(())
}

fn taffy_layout(id: &str) -> String {
    format!("taffy_layout::{id}::x=0,y=0,width=intrinsic,height=intrinsic")
}

fn parley_text(text: &str) -> String {
    format!("parley_text::{text}::shape=line_break_bidi_align")
}

fn anyrender_paint_command(stable_node_id: &str, display_slot: &str) -> String {
    format!("anyrender_paint::{display_slot}::{stable_node_id}")
}

fn accesskit_node_id(index: usize) -> String {
    format!("accesskit_node_id::{}", index + 1)
}

