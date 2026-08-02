use crate::FrontendCompositionResolver;
use crate::expr::eval_expr;

use mesh_core_component::template::TemplateNode;
use mesh_core_elements::VariableStore;
use serde_json;

use super::*;

pub(super) fn evaluate_template_expression(
    expression: &str,
    state: Option<&dyn VariableStore>,
    instance_key: &str,
    composition: Option<&dyn FrontendCompositionResolver>,
) -> serde_json::Value {
    if crate::expr::uses_translation(expression) {
        return state
            .map(|store| serde_json::Value::String(eval_expr(expression, store)))
            .unwrap_or(serde_json::Value::Null);
    }
    if let (Some(state), Some(composition)) = (state, composition)
        && let Some(result) = composition.evaluate_template_expression(
            instance_key,
            expression,
            &state.template_locals(),
        )
    {
        state.record_template_service_reads(&result.service_reads);
        return result.value;
    }
    state
        .map(|store| serde_json::Value::String(eval_expr(expression, store)))
        .unwrap_or(serde_json::Value::Null)
}

pub(super) fn template_value_to_string(value: serde_json::Value) -> String {
    crate::expr::json_value_to_string(value)
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
