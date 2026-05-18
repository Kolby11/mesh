use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

const FIXTURE_JSON: &str = include_str!("../fixtures/phase43-scenarios.json");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Phase43Fixture {
    pub surfaces: Vec<SurfaceFixture>,
    pub scenarios: Vec<ScenarioFixture>,
    pub retained_nodes: Vec<RetainedNodeFixture>,
    pub interactions: Vec<InteractionFixture>,
    pub comparison_headings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurfaceFixture {
    pub id: String,
    pub source: String,
    pub size: SurfaceSize,
    pub required_text: Vec<String>,
    pub required_controls: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurfaceSize {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioFixture {
    pub id: String,
    pub surface: String,
    pub state: serde_json::Value,
    pub expected_nodes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetainedNodeFixture {
    pub id: String,
    pub surface: String,
    pub role: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionFixture {
    pub id: String,
    pub scenario: String,
    pub target: String,
    pub event: String,
    #[serde(default)]
    pub from: Option<f64>,
    #[serde(default)]
    pub to: Option<f64>,
    #[serde(default)]
    pub value: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrototypeEvidence {
    pub path: String,
    pub generated_by: String,
    pub fixture: String,
    pub scenarios: Vec<ScenarioEvidence>,
    pub retained_nodes: Vec<RetainedNodeEvidence>,
    pub paint_commands: Vec<PaintCommandEvidence>,
    pub interactions: Vec<InteractionEvidence>,
    pub accessibility: Vec<AccessibilityEvidence>,
    pub comparison_headings: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioEvidence {
    pub scenario_id: String,
    pub surface: String,
    pub visual_layout: String,
    pub interaction_shape: String,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetainedNodeEvidence {
    pub stable_node_id: String,
    pub surface: String,
    pub role: String,
    pub label: String,
    pub taffy_layout: Option<String>,
    pub parley_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaintCommandEvidence {
    pub stable_node_id: String,
    pub display_slot: String,
    pub command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionEvidence {
    pub interaction_id: String,
    pub scenario_id: String,
    pub target: String,
    pub event: String,
    pub result: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessibilityEvidence {
    pub stable_node_id: String,
    pub accesskit_node_id: String,
    pub role: String,
    pub label: String,
}

pub fn load_fixture() -> Result<Phase43Fixture, Box<dyn std::error::Error>> {
    let fixture = serde_json::from_str(FIXTURE_JSON)?;
    Ok(fixture)
}

pub fn write_evidence(
    path: impl AsRef<Path>,
    evidence: &PrototypeEvidence,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.as_ref().parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(evidence)?;
    fs::write(path, format!("{json}\n"))?;
    Ok(())
}

pub fn required_comparison_headings() -> [&'static str; 7] {
    [
        "visual/layout fidelity",
        "interaction shape",
        "retained identity fit",
        "accessibility boundary",
        "build/dependency cost",
        "blocker evidence",
        "Phase 44 integration readiness",
    ]
}

pub fn retained_node_evidence_from_fixture(
    fixture: &Phase43Fixture,
) -> Vec<RetainedNodeEvidence> {
    fixture
        .retained_nodes
        .iter()
        .map(|node| RetainedNodeEvidence {
            stable_node_id: node.id.clone(),
            surface: node.surface.clone(),
            role: node.role.clone(),
            label: node.label.clone(),
            taffy_layout: None,
            parley_text: None,
        })
        .collect()
}

pub fn interaction_evidence_from_fixture(
    fixture: &Phase43Fixture,
    result_prefix: &str,
) -> Vec<InteractionEvidence> {
    fixture
        .interactions
        .iter()
        .map(|interaction| InteractionEvidence {
            interaction_id: interaction.id.clone(),
            scenario_id: interaction.scenario.clone(),
            target: interaction.target.clone(),
            event: interaction.event.clone(),
            result: format!("{result_prefix}: {} on {}", interaction.event, interaction.target),
        })
        .collect()
}

