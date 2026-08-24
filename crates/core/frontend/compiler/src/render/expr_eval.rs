use crate::FrontendCompositionResolver;

use mesh_core_component::template::TemplateNode;
use mesh_core_elements::VariableStore;
use mesh_core_expression::{compile_expression, evaluate_preview};
use serde_json;

use super::*;

pub(super) fn evaluate_template_expression(
    expression: &str,
    state: Option<&dyn VariableStore>,
    instance_key: &str,
    composition: Option<&dyn FrontendCompositionResolver>,
) -> serde_json::Value {
    let Some(state) = state else {
        return serde_json::Value::Null;
    };
    let Ok(compiled) = compile_expression(expression) else {
        return serde_json::Value::Null;
    };

    if let Some(composition) = composition
        && let Some(result) = composition.evaluate_template_expression(
            instance_key,
            &compiled,
            &state.template_locals(),
        )
    {
        state.record_template_service_reads(&result.service_reads);
        return result.value;
    }

    let mut variables = state.template_locals();
    for name in state.keys() {
        if !variables.contains_key(&name)
            && let Some(value) = state.get_ref(&name).cloned().or_else(|| state.get(&name))
        {
            variables.insert(name, value);
        }
    }
    evaluate_preview(&compiled, &variables, &serde_json::Map::new(), |key| {
        state.translate(key)
    })
    .unwrap_or(serde_json::Value::Null)
}

pub(super) fn template_value_to_string(value: serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => String::new(),
        serde_json::Value::String(value) => value,
        other => other.to_string(),
    }
}

pub(crate) fn collect_component_tags(nodes: &[TemplateNode], tags: &mut Vec<String>) {
    for node in nodes {
        match node {
            TemplateNode::Component(component) => {
                tags.push(component.name.clone());
                collect_component_tags(&component.children, tags);
            }
            TemplateNode::Element(element) => collect_component_tags(&element.children, tags),
            TemplateNode::If(if_node) => {
                collect_component_tags(&if_node.then_children, tags);
                collect_component_tags(&if_node.else_children, tags);
            }
            TemplateNode::For(for_node) => collect_component_tags(&for_node.children, tags),
            TemplateNode::Text(_) | TemplateNode::Expr(_) | TemplateNode::Slot(_) => {}
        }
    }
}
