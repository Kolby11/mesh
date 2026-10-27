use super::super::*;
use mesh_core_capability::CapabilitySet;
use mesh_core_component::{ComponentFile, parse_component};
use mesh_core_elements::{CallbackHandle, HandlerTarget, VariableStore};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

fn context(source: &str) -> (ScriptContext, ComponentFile) {
    let component = parse_component(source).unwrap();
    let mut context =
        ScriptContext::new("@test/function-handlers", CapabilitySet::default()).unwrap();
    context
        .compile_and_execute_component_with_compiled(
            component
                .script
                .as_ref()
                .map_or("", |script| script.source.as_str()),
            &[],
            &component.template_expressions,
        )
        .unwrap();
    (context, component)
}
fn handle(target: HandlerTarget) -> CallbackHandle {
    let HandlerTarget::Function { callback, .. } = target else {
        panic!("not a real function")
    };
    callback
}
fn bind(
    context: &ScriptContext,
    component: &ComponentFile,
    index: usize,
    locals: Map<String, Value>,
) -> HandlerTarget {
    context
        .bind_handler_expression(
            &component.template_expressions[index],
            &locals,
            "test callback",
        )
        .unwrap()
}

#[test]
fn handlers_resolve_private_functions_and_cache_stable_function_values() {
    let (mut context, component) = context(
        r#"
<template><button onclick={private} /></template>
<script lang="luau">
count = 0
local function private(event) count = count + event.amount end
</script>"#,
    );
    let first = handle(bind(&context, &component, 0, Map::new()));
    let second = handle(bind(&context, &component, 0, Map::new()));
    assert_eq!(first, second);
    context
        .call_callback(&first, &[json!({"amount":3})])
        .unwrap();
    assert_eq!(context.state.get("count"), Some(json!(3)));
}

#[test]
fn self_event_channel_forwards_bound_event_and_reuses_callback() {
    let (mut context, component) = context(
        r#"
<template><button onclick={self.Changed} /></template>
<script lang="luau">
received = 0
local changed = self.Changed
changed:on(function(event) received = event.amount end)
</script>"#,
    );
    let first = handle(bind(&context, &component, 0, Map::new()));
    let second = handle(bind(&context, &component, 0, Map::new()));
    assert_eq!(first, second);
    context
        .call_callback(&first, &[json!({"amount": 7})])
        .unwrap();
    assert_eq!(context.state.get("received"), Some(json!(7)));
}

#[test]
fn table_arguments_and_methods_use_real_luau_calls() {
    let (mut context, component) = context(
        r#"
<template><button onclick={actions:pick({ a = 2, b = 3 })} /></template>
<script lang="luau">
count = 0
kind = ""
local actions = { base = 10 }
function actions:pick(value, event) count = self.base + value.a + value.b; kind = event.type end
</script>"#,
    );
    let callback = handle(bind(&context, &component, 0, Map::new()));
    context
        .call_callback(&callback, &[json!({"type":"click"})])
        .unwrap();
    assert_eq!(context.state.get("count"), Some(json!(15)));
    assert_eq!(context.state.get("kind"), Some(json!("click")));
}

#[test]
fn inline_closures_capture_their_rendered_loop_locals() {
    let (mut context, component) = context(
        r#"
<template><button onclick={function() selected = item.id end} /></template>
<script lang="luau">selected = ""</script>"#,
    );
    let first = handle(bind(
        &context,
        &component,
        0,
        Map::from_iter([("item".into(), json!({"id":"first"}))]),
    ));
    let second = handle(bind(
        &context,
        &component,
        0,
        Map::from_iter([("item".into(), json!({"id":"second"}))]),
    ));
    context.call_callback(&first, &[]).unwrap();
    assert_eq!(context.state.get("selected"), Some(json!("first")));
    context.call_callback(&second, &[]).unwrap();
    assert_eq!(context.state.get("selected"), Some(json!("second")));
}

#[test]
fn callbacks_expire_on_reload_and_reject_another_context() {
    let (mut owner, component) = context(
        r#"<template><button onclick={function() count = 1 end} /></template><script lang="luau">count = 0</script>"#,
    );
    let callback = handle(bind(&owner, &component, 0, Map::new()));
    let mut other = ScriptContext::new("@test/other", CapabilitySet::default()).unwrap();
    assert!(other.call_callback(&callback, &[]).is_err());
    owner.load_script("count = 100").unwrap();
    assert!(owner.call_callback(&callback, &[]).is_err());
    assert_eq!(owner.state.get("count"), Some(json!(100)));
}

#[test]
fn callback_props_are_real_functions_available_during_setup_and_restore_defaults() {
    let (mut owner, component) = context(
        r#"<template><button onclick={function() count = count + 1 end} /></template><script lang="luau">count = 0</script>"#,
    );
    let callback = bind(&owner, &component, 0, Map::new());
    let mut child = ScriptContext::new("@test/child", CapabilitySet::default()).unwrap();
    child.set_initial_callback_props(BTreeMap::from([("onActivate".into(), callback)]));
    child
        .load_script(
            r#"
local captured = onActivate
own = 0
function onActivate() own = own + 1 end
function triggerCaptured() captured() end
function triggerCurrent() onActivate() end
"#,
        )
        .unwrap();
    child.call_handler("triggerCaptured", &[]).unwrap();
    child.call_handler("triggerCurrent", &[]).unwrap();
    assert!(owner.take_live_binding_external_accessed());
    owner.resync_state();
    assert_eq!(owner.state.get("count"), Some(json!(2)));
    child.set_callback_props(&BTreeMap::new()).unwrap();
    child.call_handler("triggerCurrent", &[]).unwrap();
    assert_eq!(child.state.get("own"), Some(json!(1)));
    owner.uninit();
    assert!(child.call_handler("triggerCaptured", &[]).is_err());
}

#[test]
fn callback_errors_and_budgets_do_not_poison_later_callbacks() {
    let (mut context, component) = context(
        r#"
<template><button onclick={infinite} /><button onclick={good} /></template>
<script lang="luau">
count = 0
local function infinite() while true do end end
local function good() count = count + 1 end
</script>"#,
    );
    let infinite = handle(bind(&context, &component, 0, Map::new()));
    let good = handle(bind(&context, &component, 1, Map::new()));
    assert!(context.call_callback(&infinite, &[]).is_err());
    context.call_callback(&good, &[]).unwrap();
    assert_eq!(context.state.get("count"), Some(json!(1)));
}

#[test]
fn rebinding_a_function_invalidates_the_handler_without_json_state() {
    let (mut context, component) = context(
        r#"
<template><button onclick={selectedHandler} /></template>
<script lang="luau">
count = 0
local selectedHandler = function() count = 1 end
function swap() selectedHandler = function() count = 2 end end
function noop() end
</script>"#,
    );
    let first = handle(bind(&context, &component, 0, Map::new()));
    context.mark_template_dependencies_ready();
    context.state.clear_dirty();
    context.call_handler("swap", &[]).unwrap();
    assert!(context.state.is_dirty());
    assert!(context.dirty_state_affects_template());
    let second = handle(bind(&context, &component, 0, Map::new()));
    assert_ne!(first, second);
    context.call_callback(&second, &[]).unwrap();
    assert_eq!(context.state.get("count"), Some(json!(2)));
    context.state.clear_dirty();
    context.call_handler("noop", &[]).unwrap();
    assert!(!context.state.is_dirty());
    let _ = context.call_handler("unmount", &[]);
    assert!(context.call_callback(&second, &[]).is_err());
}

#[test]
fn event_calls_are_deferred_and_reuse_their_bound_handle() {
    let (mut owner, component) = context(
        r#"
<template><button onclick={pick({ amount = amount })} /></template>
<script lang="luau">
count = 0
amount = 1
local function pick(value, event) count = value.amount + event.extra end
function change() amount = 10 end
</script>"#,
    );
    let expression = &component.template_expressions[0];
    let first = handle(
        owner
            .bind_event_expression(expression, &Map::new(), "pick", true)
            .unwrap()
            .unwrap(),
    );
    let second = handle(
        owner
            .bind_event_expression(expression, &Map::new(), "pick", true)
            .unwrap()
            .unwrap(),
    );
    assert_eq!(first, second);
    assert_eq!(owner.state.get("count"), Some(json!(0)));
    owner.call_handler("change", &[]).unwrap();
    owner.call_callback(&first, &[json!({"extra": 2})]).unwrap();
    assert_eq!(owner.state.get("count"), Some(json!(12)));
}

#[test]
fn forwarded_callback_functions_keep_the_defining_environment() {
    let (mut owner, component) = context(
        r#"<template><button onclick={function(value) count = count + value end} /></template><script lang="luau">count = 0</script>"#,
    );
    let callback = bind(&owner, &component, 0, Map::new());
    let middle_component =
        parse_component(r#"<template><button onclick={onPick} /></template>"#).unwrap();
    let mut middle = ScriptContext::new("@test/middle", CapabilitySet::default()).unwrap();
    middle.set_initial_callback_props(BTreeMap::from([("onPick".into(), callback)]));
    middle
        .compile_and_execute_component_with_compiled(
            "",
            &[],
            &middle_component.template_expressions,
        )
        .unwrap();
    let forwarded = bind(&middle, &middle_component, 0, Map::new());
    let mut child = ScriptContext::new("@test/last", CapabilitySet::default()).unwrap();
    child.set_initial_callback_props(BTreeMap::from([("onPick".into(), forwarded)]));
    child
        .load_script("function trigger() onPick(7) end")
        .unwrap();
    child.call_handler("trigger", &[]).unwrap();
    owner.resync_state();
    assert_eq!(owner.state.get("count"), Some(json!(7)));
    middle.uninit();
    assert!(child.call_handler("trigger", &[]).is_err());
}

#[test]
fn callback_binding_limit_reclaims_dropped_handles() {
    let (owner, component) =
        context(r#"<template><button onclick={function() return item end} /></template>"#);
    let mut callbacks = Vec::new();
    for item in 0..4096 {
        callbacks.push(bind(
            &owner,
            &component,
            0,
            Map::from_iter([("item".into(), json!(item))]),
        ));
    }
    assert!(
        owner
            .bind_handler_expression(&component.template_expressions[0], &Map::new(), "overflow")
            .is_err()
    );
    callbacks.clear();
    assert!(
        owner
            .bind_handler_expression(&component.template_expressions[0], &Map::new(), "reclaimed")
            .is_ok()
    );
}

#[test]
fn callback_does_not_inherit_the_callers_host_grants() {
    let (mut owner, component) = context(
        r#"
<template><button onclick={function() mesh.shell.show_surface("@test/surface") end} /></template>"#,
    );
    let callback = bind(&owner, &component, 0, Map::new());
    let mut child = ScriptContext::new(
        "@test/privileged-child",
        CapabilitySet::from_ids(["shell.surface"]),
    )
    .unwrap();
    child.set_initial_callback_props(BTreeMap::from([("onPick".into(), callback)]));
    child
        .load_script("function trigger() onPick() end")
        .unwrap();
    let error = child.call_handler("trigger", &[]).unwrap_err();
    assert!(error.to_string().contains("shell.surface"), "{error}");
    assert!(owner.drain_shell_effects().is_empty());
    assert!(child.drain_shell_effects().is_empty());
}

#[test]
fn nil_disables_optional_handlers_and_rebinding_enables_them() {
    let (mut owner, component) = context(
        r#"<template><button onclick={optional} /></template>
<script lang="luau">local optional = nil; count = 0
function enable() optional = function() count = count + 1 end end
</script>"#,
    );
    let expression = &component.template_expressions[0];
    assert!(
        owner
            .bind_event_expression(expression, &Map::new(), "optional", false)
            .unwrap()
            .is_none()
    );
    owner.state.clear_dirty();
    owner.call_handler("enable", &[]).unwrap();
    assert!(owner.state.is_dirty());
    let callback = handle(
        owner
            .bind_event_expression(expression, &Map::new(), "optional", false)
            .unwrap()
            .unwrap(),
    );
    owner.call_callback(&callback, &[]).unwrap();
    assert_eq!(owner.state.get("count"), Some(json!(1)));
}

#[test]
fn value_write_back_callback_notifies_template_readers_and_effects() {
    let (mut context, component) = context(
        r#"
<template><slider value={volume} onchange={function(...) volume = ... end} /></template>
<script lang="luau">
volume = 10
effect(function() observed = volume * 2 end)
</script>"#,
    );
    assert!(context.run_reactive_effects().is_empty());
    let read = component
        .template_expressions
        .iter()
        .find(|expression| expression.source() == "volume")
        .unwrap();
    context
        .evaluate_compiled_template_expression(read, &Map::new())
        .unwrap();
    let setter = component
        .template_expressions
        .iter()
        .find(|expression| expression.source().contains("function"))
        .unwrap();
    let callback = handle(
        context
            .bind_event_expression(setter, &Map::new(), "write-back", true)
            .unwrap()
            .unwrap(),
    );
    context.state.clear_dirty();
    context.call_callback(&callback, &[json!(37.5)]).unwrap();
    assert!(context.state.is_dirty());
    assert!(context.dirty_state_affects_template());
    assert!(context.run_reactive_effects().is_empty());
    assert_eq!(context.state.get("volume"), Some(json!(37.5)));
    assert_eq!(context.state.get("observed"), Some(json!(75)));
    assert_eq!(
        context
            .evaluate_compiled_template_expression(read, &Map::new())
            .unwrap()
            .0,
        json!(37.5)
    );
}
