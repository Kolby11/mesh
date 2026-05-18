use mesh_phase43_renderer_prototypes::{
    AccessibilityEvidence, PaintCommandEvidence, PrototypeEvidence, ScenarioEvidence,
    interaction_evidence_from_fixture, load_fixture, required_comparison_headings,
    retained_node_evidence_from_fixture, write_evidence,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = load_fixture()?;
    let scenarios = fixture
        .scenarios
        .iter()
        .map(|scenario| {
            let html_equivalent = html_equivalent_for(&scenario.id);
            ScenarioEvidence {
                scenario_id: scenario.id.clone(),
                surface: scenario.surface.clone(),
                visual_layout: format!(
                    "blitz_reference structured HTML/CSS equivalent recorded for {}",
                    html_equivalent
                ),
                interaction_shape: format!(
                    "blitz_reference fixture event mapping recorded for {}",
                    scenario.id
                ),
                notes: vec![
                    "Default harness stays compileable while optional blitz-reference feature records the dependency blocker.".to_string(),
                    "Pixel rendering is blocked by blitz crate compile failure in this environment.".to_string(),
                ],
            }
        })
        .collect();

    let retained_nodes = retained_node_evidence_from_fixture(&fixture);
    let interactions = interaction_evidence_from_fixture(&fixture, "blitz-reference");
    let paint_commands = fixture
        .retained_nodes
        .iter()
        .flat_map(|node| {
            ["Background", "Border", "Text", "Icon", "Generic"]
                .into_iter()
                .map(move |slot| PaintCommandEvidence {
                    stable_node_id: node.id.clone(),
                    display_slot: slot.to_string(),
                    command: format!("html-css-equivalent::{slot}::{}", node.id),
                })
        })
        .collect();
    let accessibility = fixture
        .retained_nodes
        .iter()
        .enumerate()
        .map(|(index, node)| AccessibilityEvidence {
            stable_node_id: node.id.clone(),
            accesskit_node_id: format!("blitz-accesskit-node-{index}"),
            role: node.role.clone(),
            label: node.label.clone(),
        })
        .collect();

    let evidence = PrototypeEvidence {
        path: "blitz-reference".to_string(),
        generated_by: "src/bin/blitz_reference.rs".to_string(),
        fixture: ".planning/prototypes/phase43/fixtures/phase43-scenarios.json".to_string(),
        scenarios,
        retained_nodes,
        paint_commands,
        interactions,
        accessibility,
        comparison_headings: required_comparison_headings()
            .into_iter()
            .map(str::to_string)
            .collect(),
        notes: vec![
            "Attempted command: cargo check --manifest-path .planning/prototypes/phase43/Cargo.toml --features blitz-reference".to_string(),
            "Crate/API boundary: optional blitz = 0.3.0-alpha.4 dependency with default features disabled.".to_string(),
            "Observed blocker: blitz crate compile error E0425, cannot find value event_loop in src/lib.rs.".to_string(),
        ],
    };

    write_evidence(
        ".planning/prototypes/phase43/output/blitz-reference.json",
        &evidence,
    )?;
    Ok(())
}

fn html_equivalent_for(scenario_id: &str) -> &'static str {
    match scenario_id {
        "nav-baseline" => "nav shell with status text and volume/theme/settings controls",
        "nav-audio-trigger-hover" => "nav shell with hovered volume trigger",
        "audio-popover-visible" => "audio popover with title, status, percent, slider, and actions",
        "audio-slider-change-release" => "audio popover slider changing from 0.42 to 0.73 and releasing",
        "audio-popover-close" => "audio popover exiting with mesh-surface-exiting opacity state",
        _ => "unknown scenario",
    }
}

