use crate::FrontendCompositionResolver;

use mesh_core_component::template::TemplateNode;
use mesh_core_elements::VariableStore;
use mesh_core_expression::compile_expression;
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

    #[cfg(test)]
    {
        let mut context = mesh_core_scripting::ScriptContext::new(
            "@test/frontend-expression",
            Default::default(),
        )
        .unwrap();
        context
            .compile_and_execute_component_with_compiled("", &[], &[compiled.clone()])
            .unwrap();
        for name in state.keys() {
            if let Some(value) = state.get_ref(&name).cloned().or_else(|| state.get(&name)) {
                if !value.is_null() {
                    context.seed_context_global(&name, value).unwrap();
                }
            }
        }
        context
            .evaluate_compiled_template_expression(&compiled, &state.template_locals())
            .map(|(value, _)| value)
            .unwrap_or(serde_json::Value::Null)
    }
    #[cfg(not(test))]
    serde_json::Value::Null
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
