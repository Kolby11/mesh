use mesh_core_component::template::{Attribute, AttributeValue, ElementNode, SourceTag};

/// Lower value bindings to ordinary Luau callbacks in the component's scope.
pub(crate) fn lower_value_bindings(element: &ElementNode) -> Result<Option<ElementNode>, String> {
    if !element
        .attributes
        .iter()
        .any(|attr| matches!(attr.value, AttributeValue::TwoWayBinding(_)))
    {
        return Ok(None);
    }
    let mut lowered = element.clone();
    if lowered.tag == "input" {
        let kind = lowered
            .attributes
            .iter()
            .find_map(|attr| match (&*attr.name, &attr.value) {
                ("type", AttributeValue::Static(kind)) => Some(kind.as_str()),
                _ => None,
            });
        match kind {
            Some("range") => {
                lowered.tag = "slider".into();
                lowered.tag_kind = SourceTag::Slider;
            }
            Some("checkbox") => {
                lowered.tag = "checkbox".into();
                lowered.tag_kind = SourceTag::Checkbox;
            }
            _ => {}
        }
    }
    let expected = match lowered.tag.as_str() {
        "input" | "slider" => "value",
        "switch" | "checkbox" => "checked",
        _ => {
            return Err(format!(
                "two-way bindings are not supported on <{}>",
                element.tag
            ));
        }
    };
    let mut assignment = String::new();
    for attr in &mut lowered.attributes {
        if let AttributeValue::TwoWayBinding(target) = &attr.value {
            let target = target.trim();
            if attr.name != expected {
                return Err(format!(
                    "<{}> supports bind:{expected}, not bind:{}",
                    element.tag, attr.name
                ));
            }
            if target.is_empty()
                || !target.bytes().enumerate().all(|(i, byte)| {
                    byte == b'_' || byte.is_ascii_alphabetic() || (i > 0 && byte.is_ascii_digit())
                })
            {
                return Err("two-way binding target must be a Luau variable name".into());
            }
            // Varargs avoid introducing a parameter that shadows the authored target.
            assignment.push_str(&format!("{target} = ...\n"));
            attr.value = AttributeValue::Binding(target.into());
        }
    }
    let mut callbacks = Vec::new();
    lowered.attributes.retain(|attr| {
        if attr.name != "onchange" && attr.name != "on:change" {
            return true;
        }
        callbacks.push(match &attr.value {
            AttributeValue::EventHandlerExpression { expression, .. } => {
                expression.source().to_owned()
            }
            AttributeValue::EventHandler(name) | AttributeValue::Binding(name) => name.clone(),
            AttributeValue::EventHandlerCall { handler, args } => format!(
                "function(...) return {handler}({}...) end",
                if args.is_empty() {
                    String::new()
                } else {
                    format!("{}, ", args.join(", "))
                }
            ),
            _ => "nil".into(),
        });
        false
    });
    let calls = callbacks.iter().map(|callback| format!(
        "do local callback = ({callback}\n); if callback ~= nil then callback(...) end end\n"
    )).collect::<String>();
    let source = format!("function(...)\n{assignment}{calls}end");
    let expression = mesh_core_expression::compile_expression(&source)
        .map_err(|error| format!("invalid two-way binding: {error}"))?;
    lowered.attributes.push(Attribute {
        name: "onchange".into(),
        value: AttributeValue::EventHandlerExpression {
            source,
            expression,
            call_binding: true,
        },
        quoted_expression: false,
        span: Some(element.span),
    });
    Ok(Some(lowered))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mesh_core_component::{parse_component, template::TemplateNode};
    use mesh_core_elements::{HandlerTarget, VariableStore};
    use serde_json::{Map, json};

    struct Live<'a>(&'a mesh_core_scripting::ScriptContext);
    impl crate::FrontendCompositionResolver for Live<'_> {
        fn bind_handler_expression(
            &self,
            _: &str,
            expression: &mesh_core_expression::CompiledExpression,
            locals: &Map<String, serde_json::Value>,
            source: &str,
            call_binding: bool,
        ) -> Option<HandlerTarget> {
            self.0
                .bind_event_expression(expression, locals, source, call_binding)
                .unwrap()
        }
        fn evaluate_template_expression(
            &self,
            _: &str,
            expression: &mesh_core_expression::CompiledExpression,
            locals: &Map<String, serde_json::Value>,
        ) -> Option<crate::TemplateExpressionResult> {
            let (value, service_reads) = self
                .0
                .evaluate_compiled_template_expression(expression, locals)
                .unwrap();
            Some(crate::TemplateExpressionResult {
                value,
                service_reads,
            })
        }
        fn render_import(
            &self,
            _: &mesh_core_module::Manifest,
            _: &str,
            _: Option<&std::path::Path>,
            _: &str,
            _: usize,
            _: Option<usize>,
            _: bool,
            _: Option<&str>,
            _: &mesh_core_elements::ComponentCompositionProps,
            _: &std::collections::BTreeMap<String, mesh_core_elements::EventHandlerCall>,
            _: f32,
            _: f32,
        ) -> Option<mesh_core_elements::WidgetNode> {
            None
        }
        fn render_slot(
            &self,
            _: &mesh_core_module::Manifest,
            _: &str,
            _: Option<&str>,
            _: Option<&str>,
            _: bool,
            _: f32,
            _: f32,
        ) -> Vec<mesh_core_elements::WidgetNode> {
            Vec::new()
        }
    }
    fn control(node: &mesh_core_elements::WidgetNode) -> &mesh_core_elements::WidgetNode {
        if node.event_handlers.contains_key("change") {
            node
        } else {
            control(&node.children[0])
        }
    }

    fn element(source: &str) -> ElementNode {
        let component = parse_component(source).unwrap();
        let TemplateNode::Element(element) = &component.template.unwrap().root[0] else {
            panic!("element")
        };
        element.clone()
    }

    #[test]
    fn two_way_compiler_rejects_non_variables_and_unsupported_attributes() {
        for markup in [
            r#"<input bind:value="state.value"/>"#,
            r#"<input bind:value="value + 1"/>"#,
            r#"<slider bind:checked="enabled"/>"#,
            r#"<text bind:value="value"/>"#,
            r#"<checkbox bind:value="enabled"/>"#,
        ] {
            let element = element(&format!("<template>{markup}</template>"));
            assert!(lower_value_bindings(&element).is_err(), "{markup}");
        }
    }

    #[test]
    fn two_way_interactions_assign_typed_values_before_authored_handlers() {
        for (tag, attr, initial, edited, runtime_tag) in [
            (
                "input",
                "value",
                json!("initial"),
                json!("hello '世界'"),
                "input",
            ),
            ("slider", "value", json!(25), json!(42.5), "slider"),
            (
                "input type=\"range\"",
                "value",
                json!(25),
                json!(42.5),
                "slider",
            ),
            ("checkbox", "checked", json!(false), json!(true), "checkbox"),
            ("switch", "checked", json!(true), json!(false), "switch"),
            (
                "input type=\"checkbox\"",
                "checked",
                json!(false),
                json!(true),
                "checkbox",
            ),
        ] {
            for handler in ["", "onchange={changed}", "onchange={changed()}"] {
                let source = format!(
                    r#"<template><{tag} {handler} bind:{attr}="current" /></template>
<script lang="luau">
current = {initial}
seen = nil
calls = 0
function changed(value)
    assert(current == value)
    seen = value
    calls += 1
end
</script>"#
                );
                let component = parse_component(&source).unwrap();
                let expressions = crate::collect_template_expressions(&component);
                let mut context =
                    mesh_core_scripting::ScriptContext::new("@test/binding", Default::default())
                        .unwrap();
                context
                    .compile_and_execute_component_with_compiled(
                        &component.script.as_ref().unwrap().source,
                        &[],
                        &expressions,
                    )
                    .unwrap();
                let manifest = mesh_core_module::package::ModuleManifest::from_json_str(r#"{"name":"@test/binding","version":"1.0.0","mesh":{"kind":"ui","apiVersion":"0.1.0","entrypoints":{"main":"main.mesh"}}}"#).unwrap().into_runtime_manifest();
                let tree = crate::build_widget_tree_from_component(
                    &component,
                    &manifest,
                    &mesh_core_theme::default_theme(),
                    320.0,
                    120.0,
                    Some(&Live(&context)),
                    "root",
                    Some(&context.state),
                    &[],
                );
                let node = control(&tree);
                assert_eq!(
                    node.tag,
                    runtime_tag
                );
                assert_eq!(
                    node.attributes.get_by_name(attr).unwrap(),
                    &match &initial {
                        serde_json::Value::String(value) => value.clone(),
                        value => value.to_string(),
                    }
                );
                let HandlerTarget::Function { callback, .. } =
                    node.event_handlers["change"].clone()
                else {
                    panic!("function")
                };
                context.state.clear_dirty();
                context.call_callback(&callback, &[edited.clone()]).unwrap();
                assert!(context.state.is_dirty());
                assert!(context.dirty_state_affects_template());
                assert_eq!(context.state.get("current"), Some(edited.clone()));
                let read = mesh_core_expression::compile_expression("current").unwrap();
                assert_eq!(
                    context
                        .evaluate_compiled_template_expression(&read, &Map::new())
                        .unwrap()
                        .0,
                    edited
                );
                if !handler.is_empty() {
                    assert_eq!(context.state.get("seen"), Some(edited.clone()));
                    assert_eq!(context.state.get("calls"), Some(json!(1)));
                }
            }
        }
    }

    #[test]
    fn two_way_callbacks_keep_loop_arguments_and_resolve_optional_handlers_at_change() {
        let source = r#"<template><input bind:value="query" onchange={changed and function(value) changed(row, value) end} /></template>
<script lang="luau">
query = ""
seen = ""
changed = nil
function enable() changed = function(row, value) seen = row .. value end end
</script>"#;
        let component = parse_component(source).unwrap();
        let mut context =
            mesh_core_scripting::ScriptContext::new("@test/binding", Default::default()).unwrap();
        context
            .compile_and_execute_component_with_compiled(
                &component.script.as_ref().unwrap().source,
                &[],
                &crate::collect_template_expressions(&component),
            )
            .unwrap();
        let lowered = lower_value_bindings(&element(source)).unwrap().unwrap();
        let AttributeValue::EventHandlerExpression {
            expression,
            source,
            call_binding,
        } = &lowered.attributes.last().unwrap().value
        else {
            panic!("callback")
        };
        let HandlerTarget::Function { callback, .. } = context
            .bind_event_expression(
                expression,
                &Map::from_iter([("row".into(), json!("row:"))]),
                source,
                *call_binding,
            )
            .unwrap()
            .unwrap()
        else {
            panic!("function")
        };
        context.call_callback(&callback, &[json!("first")]).unwrap();
        assert_eq!(context.state.get("query"), Some(json!("first")));
        assert_eq!(context.state.get("seen"), Some(json!("")));
        context.call_handler("enable", &[]).unwrap();
        context
            .call_callback(&callback, &[json!("second")])
            .unwrap();
        assert_eq!(context.state.get("query"), Some(json!("second")));
        assert_eq!(context.state.get("seen"), Some(json!("row:second")));
        assert!(
            lower_value_bindings(&element(r#"<template><input value={query} /></template>"#))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn two_way_callbacks_capture_lexical_state_and_keep_component_isolation() {
        let source = r#"<template><input bind:value="query" /></template>
<script lang="luau">local query = "initial"</script>"#;
        let component = parse_component(source).unwrap();
        let expressions = crate::collect_template_expressions(&component);
        let mut contexts = (0..2)
            .map(|_| {
                let mut context =
                    mesh_core_scripting::ScriptContext::new("@test/binding", Default::default())
                        .unwrap();
                context
                    .compile_and_execute_component_with_compiled(
                        &component.script.as_ref().unwrap().source,
                        &[],
                        &expressions,
                    )
                    .unwrap();
                context
            })
            .collect::<Vec<_>>();
        let lowered = lower_value_bindings(&element(source)).unwrap().unwrap();
        let AttributeValue::EventHandlerExpression {
            expression, source, ..
        } = &lowered.attributes.last().unwrap().value
        else {
            panic!("callback")
        };
        let HandlerTarget::Function { callback, .. } = contexts[0]
            .bind_handler_expression(expression, &Map::new(), source)
            .unwrap()
        else {
            panic!("function")
        };
        contexts[0]
            .call_callback(&callback, &[json!("edited")])
            .unwrap();
        assert!(
            contexts[1]
                .call_callback(&callback, &[json!("wrong owner")])
                .is_err()
        );
        let read = mesh_core_expression::compile_expression("query").unwrap();
        assert_eq!(
            contexts[0]
                .evaluate_compiled_template_expression(&read, &Map::new())
                .unwrap()
                .0,
            json!("edited")
        );
        assert_eq!(
            contexts[1]
                .evaluate_compiled_template_expression(&read, &Map::new())
                .unwrap()
                .0,
            json!("initial")
        );
    }
}
