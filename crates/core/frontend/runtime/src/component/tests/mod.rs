use super::*;
use mesh_core_elements::Prop;

mod common;

mod integration;
mod interaction;
mod invalidation;
mod restyle;

#[test]
fn tooltip_settings_follow_the_effective_store_without_durable_reload() {
    let mut component = common::test_frontend_component("<template><box /></template>");
    let first_settings = Arc::new(
        mesh_core_config::SettingsStore::from_value(
            "/tmp/mesh-effective-tooltip-settings.json",
            serde_json::json!({
                "revision": 1,
                "shell": {
                    "tooltip": {
                        "position": "top",
                        "delay_ms": 17
                    }
                }
            }),
        )
        .unwrap(),
    );

    component.apply_settings(&first_settings).unwrap();

    assert_eq!(component.tooltip_settings.position, "top");
    assert_eq!(component.tooltip_settings.delay_ms, 17);
    assert_eq!(component.tooltip_settings_revision, 1);

    let second_settings = Arc::new(
        mesh_core_config::SettingsStore::from_value(
            "/tmp/mesh-effective-tooltip-settings.json",
            serde_json::json!({
                "revision": 1,
                "shell": {
                    "tooltip": {
                        "position": "bottom",
                        "delay_ms": 23
                    }
                }
            }),
        )
        .unwrap(),
    );
    component.apply_settings(&second_settings).unwrap();

    assert_eq!(component.tooltip_settings.position, "bottom");
    assert_eq!(component.tooltip_settings.delay_ms, 23);
    assert_eq!(component.tooltip_settings_revision, 1);
}

#[test]
fn frontend_runtime_rejects_missing_activation_resolved_capabilities() {
    let component_id = "@test/capability-surface";
    let component_manifest = common::minimal_test_manifest(component_id);
    let mut component = common::test_frontend_component_with_manifest(
        "<template><box /></template>",
        component_manifest,
    );
    component = component.with_effective_capabilities(Arc::new(HashMap::new()));

    component
        .mount(ComponentContext {
            component_id: component_id.into(),
            surface_id: component_id.into(),
            diagnostics: Diagnostics::new(component_id),
        })
        .unwrap();

    assert!(component.runtimes.lock().unwrap().is_empty());
    assert!(
        component
            .runtime_failure
            .borrow()
            .as_deref()
            .is_some_and(|message| message.contains("activation-resolved capability grant"))
    );
}

#[test]
fn markup_expressions_run_as_full_luau_in_component_scope() {
    let mut component = common::test_frontend_component(
        r#"
<template>
  <column title={string.upper("scope")}>
    <text>{add(secret, 2)}</text>
    {#for item in values}
      <text>{add(item.value, secret)}</text>
    {/for}
  </column>
</template>
<script lang="luau">
local secret = 40
local values = {{ value = 1 }, { value = 2 }}
local function add(a, b)
  return a + b
end
</script>
"#,
    );
    let theme = mesh_core_theme::default_theme();
    let tree = component.build_tree(&theme, 200, 100);
    let column = &tree.children[0];
    assert_eq!(
        column.attributes.get(Prop::Title).map(String::as_str),
        Some("SCOPE")
    );
    let text: Vec<_> = column
        .children
        .iter()
        .flat_map(|child| {
            if child.tag == "text" {
                vec![child]
            } else {
                child.children.iter().collect()
            }
        })
        .filter_map(|node| node.attributes.get(Prop::Content).map(String::as_str))
        .collect();
    assert_eq!(text, vec!["42", "41", "42"]);
}

#[test]
fn generated_error_placeholder_is_bounded_after_restyle_constraints() {
    let message = "missing interface ".repeat(100);
    let mut node = runtime::bounded_error_widget(&message);

    // Simulate arbitrary host CSS winning during the normal restyle pass.
    node.computed_style.max_width = mesh_core_elements::Dimension::Auto;
    node.children[0].computed_style.max_width = mesh_core_elements::Dimension::Auto;
    rendering::constrain_error_placeholders(&mut node);

    for constrained in [&node, &node.children[0]] {
        assert_eq!(
            constrained.computed_style.max_width,
            mesh_core_elements::Dimension::Px(ERROR_PLACEHOLDER_MAX_WIDTH)
        );
        assert_eq!(
            constrained.computed_style.min_width,
            mesh_core_elements::Dimension::Px(0.0)
        );
        assert_eq!(
            constrained.computed_style.overflow_x,
            mesh_core_elements::style::Overflow::Hidden
        );
        assert_eq!(
            constrained.computed_style.white_space,
            mesh_core_elements::style::WhiteSpace::Nowrap
        );
        assert_eq!(
            constrained.computed_style.text_overflow,
            mesh_core_elements::style::TextOverflow::Ellipsis
        );
    }
    assert_eq!(node.attributes.get(Prop::Content), Some(&message));
}

#[test]
fn element_metric_usage_splits_refs_from_elements() {
    let refs_only = common::test_frontend_component(
        r#"
<template>
  <button ref="action" />
</template>
<script lang="luau">
</script>
"#,
    );
    assert_eq!(
        refs_only.element_metric_usage,
        ElementMetricUsage {
            elements: false,
            refs: true
        }
    );

    let elements_binding = common::test_frontend_component(
        r#"
<template>
  <box />
</template>
<script lang="luau">
-- elements["root/0"].width
</script>
"#,
    );
    assert_eq!(
        elements_binding.element_metric_usage,
        ElementMetricUsage {
            elements: true,
            refs: false
        }
    );
}

#[test]
fn service_payload_cache_reuses_equal_payload_and_replaces_changed_payload() {
    let first = serde_json::json!({ "available": true, "percent": 42 });
    let changed = serde_json::json!({ "available": true, "percent": 73 });
    let first_fingerprint = ScriptContext::service_payload_fingerprint(&first);
    let changed_fingerprint = ScriptContext::service_payload_fingerprint(&changed);
    let mut cache = HashMap::new();

    assert!(
        update_cached_service_payload(&mut cache, "audio", &first, first_fingerprint).is_none()
    );
    let retained = Arc::clone(&cache["audio"].value);
    let previous =
        update_cached_service_payload(&mut cache, "audio", &first, first_fingerprint).unwrap();
    assert!(Arc::ptr_eq(&retained, &previous));
    assert!(Arc::ptr_eq(&retained, &cache["audio"].value));

    let previous =
        update_cached_service_payload(&mut cache, "audio", &changed, changed_fingerprint).unwrap();
    assert!(Arc::ptr_eq(&retained, &previous));
    assert!(!Arc::ptr_eq(&retained, &cache["audio"].value));
    assert_eq!(cache["audio"].value.as_ref(), &changed);
}

#[test]
fn service_capabilities_cache_reuses_the_global_entry() {
    let mut cache = HashMap::new();
    let first = cached_service_capabilities(&mut cache, "mesh.audio");
    let second = cached_service_capabilities(&mut cache, "mesh.audio");

    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(cache.len(), 1);
    assert_eq!(second.service_name, "audio");
}

#[test]
fn child_display_list_cache_evicts_only_the_least_recently_used_entry() {
    let mut cache = ChildDisplayListCache::default();
    for node_id in 1..=MAX_RETAINED_CHILD_DISPLAY_LISTS as NodeId {
        cache.get_or_insert(node_id);
    }

    cache.get_or_insert(1);
    cache.get_or_insert(MAX_RETAINED_CHILD_DISPLAY_LISTS as NodeId + 1);

    assert_eq!(cache.entries.len(), MAX_RETAINED_CHILD_DISPLAY_LISTS);
    assert!(
        cache.get(1).is_some(),
        "recently touched entry must survive"
    );
    assert!(
        cache.get(2).is_none(),
        "oldest untouched entry should be evicted"
    );
    for node_id in 3..=MAX_RETAINED_CHILD_DISPLAY_LISTS as NodeId + 1 {
        assert!(
            cache.get(node_id).is_some(),
            "inserting at the cap must preserve every non-evicted entry"
        );
    }
}

#[test]
fn last_service_trace_is_retained_only_for_debug_logging() {
    let mut summary = None;
    update_last_service_trace(&mut summary, "mesh.audio", "@mesh/pipewire", true);
    assert_eq!(summary.as_deref(), Some("mesh.audio:@mesh/pipewire"));

    update_last_service_trace(&mut summary, "mesh.audio", "@mesh/pipewire", false);
    assert!(summary.is_none());
}

#[test]
fn ref_metrics_keep_scroll_offsets_from_unpublished_ancestors() {
    let mut root = WidgetNode::new("box");
    root.scroll_metrics.get_or_insert_default().y = 12.0;

    let mut child = WidgetNode::new("button");
    child.set_mesh_key("root/0");
    child.attributes.insert(Prop::Ref, "action".into());
    child.layout.y = 30.0;
    child.layout.height = 10.0;
    root.children.push(child);

    let mut elements = serde_json::Map::new();
    let mut refs = serde_json::Map::new();
    let mut ref_keys = HashMap::new();
    collect_element_metrics(
        &root,
        0.0,
        0.0,
        false,
        true,
        &mut elements,
        &mut refs,
        &mut ref_keys,
    );

    assert!(elements.is_empty());
    assert_eq!(ref_keys.get("action").map(String::as_str), Some("root/0"));
    let top = refs
        .get("action")
        .and_then(|metrics| metrics.get("top"))
        .and_then(serde_json::Value::as_f64)
        .unwrap_or_default();
    assert_eq!(top, 18.0);
}

#[test]
fn element_metrics_move_last_preserves_all_publications() {
    let mut node = WidgetNode::new("box");
    node.set_mesh_key("root/0");
    node.attributes.insert(Prop::Id, "by_id".into());
    node.attributes.insert(Prop::Ref, "by_ref".into());
    node.set_bind_this("by_binding");
    node.layout.width = 120.0;
    node.layout.height = 32.0;

    let mut elements = serde_json::Map::new();
    let mut refs = serde_json::Map::new();
    let mut ref_keys = HashMap::new();
    collect_element_metrics(
        &node,
        0.0,
        0.0,
        true,
        true,
        &mut elements,
        &mut refs,
        &mut ref_keys,
    );

    let expected = elements.get("root/0").expect("element publication");
    assert_eq!(refs.get("by_id"), Some(expected));
    assert_eq!(refs.get("by_ref"), Some(expected));
    assert_eq!(refs.get("by_binding"), Some(expected));
    assert_eq!(ref_keys.len(), 3);
}

#[test]
fn child_settings_cache_refreshes_for_props_and_effective_settings() {
    let mut surface = common::test_frontend_component("<template><box /></template>");
    let mut child = surface.compiled.manifest.clone();
    child.package.id = "@test/child-settings".into();
    let component = mesh_core_component::parse_component(r#"<props>{ count = { type = "number", default = 7 } }</props><template><box /></template>"#).unwrap();
    let first = surface.settings_json_for_component(&child, &component, Path::new("child.mesh"));
    assert_eq!(
        surface.settings_json_for_component(&child, &component, Path::new("child.mesh")),
        first
    );
    assert_eq!(
        surface.child_settings_cache.borrow()[&child.package.id].len(),
        1
    );
    let settings = common::test_settings_store_with(
        &child.package.id,
        serde_json::json!({ "props": { "global": { "count": 12 } } }),
    );
    surface.apply_settings(&settings).unwrap();
    assert!(surface.child_settings_cache.borrow().is_empty());
    let second = surface.settings_json_for_component(&child, &component, Path::new("child.mesh"));
    assert_eq!(second["props"]["global"]["count"].as_f64(), Some(12.0));
    assert_ne!(first, second);
    let changed = mesh_core_component::parse_component(r#"<props>{ count = { type = "number", default = 9 }, extra = { type = "bool", default = true } }</props><template><box /></template>"#).unwrap();
    surface.prepare_runtimes_for_reload();
    surface.settings_json_for_component(&child, &changed, Path::new("child.mesh"));
    assert_eq!(
        surface.child_settings_cache.borrow()[&child.package.id].len(),
        1
    );
}

#[test]
#[ignore = "child settings resolution workload"]
fn child_settings_resolution_workload() {
    use std::hint::black_box;
    let mut surface = common::test_frontend_component("<template><box /></template>");
    let mut child = surface.compiled.manifest.clone();
    child.package.id = "@test/child-settings-workload".into();
    let props = (0..32)
        .map(|index| format!("p{index} = {{ type = \"number\", default = {index} }},"))
        .collect::<String>();
    let component = mesh_core_component::parse_component(&format!(
        "<props>{{{props}}}</props><template><box /></template>"
    ))
    .unwrap();
    for filled in [false, true] {
        let overrides = if filled {
            serde_json::json!({ "props": { "global": (0..32).map(|index| (format!("p{index}"), serde_json::json!(index + 1))).collect::<serde_json::Map<_, _>>() } })
        } else {
            serde_json::json!({})
        };
        surface.settings = common::test_settings_store_with(&child.package.id, overrides);
        for _ in 0..4 {
            surface.child_settings_cache.borrow_mut().clear();
            let start = Instant::now();
            for _ in 0..2000 {
                black_box(
                    resolve_frontend_module_settings_with_props(
                        &child.package.id,
                        surface.settings.namespace(&child.package.id),
                        &child,
                        component.props.as_ref(),
                    )
                    .effective,
                );
            }
            let uncached = start.elapsed();
            let start = Instant::now();
            for _ in 0..2000 {
                black_box(surface.settings_json_for_component(
                    &child,
                    &component,
                    Path::new("child.mesh"),
                ));
            }
            eprintln!(
                "child_settings overrides={filled} props=32 resolutions=2000 uncached={uncached:?} cached={:?}",
                start.elapsed()
            );
        }
    }
}

#[test]
fn promoted_tooltip_fade_keeps_parent_clean_and_paints_final_frame() {
    let mut component = common::test_frontend_component("<template><box /></template>");
    component.take_dirty_for_paint();
    let mut theme = mesh_core_theme::default_theme();
    theme
        .defaults_mut()
        .components
        .entry("tooltip".into())
        .or_default()
        .insert("animation".into(), "fade 200ms linear".into());
    theme.keyframes.insert(
        "fade".into(),
        vec![
            mesh_core_theme::ThemeKeyframeStop {
                offset: 0.0,
                easing: None,
                declarations: [("opacity".into(), "0".into())].into_iter().collect(),
            },
            mesh_core_theme::ThemeKeyframeStop {
                offset: 1.0,
                easing: None,
                declarations: [("opacity".into(), "1".into())].into_iter().collect(),
            },
        ],
    );
    component.tooltip_animation = tooltip::tooltip_animation_from_theme(&theme);
    component.tooltip_visible = true;
    component.tooltip_popup = Some(tooltip::TooltipPopup {
        text: Arc::from("Firefox"),
        element: (100.0, 30.0, 164.0, 94.0),
        side: tooltip::PlacedSide::Top,
        size: (120.0, 26.0),
        gap: 8.0,
        style: mesh_core_render::TooltipPaintStyle::DEFAULT_DARK,
        font_family: "Inter".into(),
        opacity: 0.0,
        scale: 1.0,
    });
    let mut canvas = SurfaceCanvas::from(PixelBuffer::new(160, 60));
    let key = tooltip::TOOLTIP_CHILD_KEY;
    component
        .paint_child_surface(key, &mut canvas, 1.0, (10, 10), false)
        .unwrap();
    assert!(!component.wants_render());
    for elapsed in [50, 100, 250] {
        component.tooltip_appeared_at = Some(Instant::now() - Duration::from_millis(elapsed));
        assert!(component.wants_tick());
        component.tick().unwrap();
        assert!(component.wants_render());
        assert!(!component.wants_parent_render());
        let popup = component.tooltip_popup.as_ref().unwrap();
        if elapsed < 200 {
            assert!(popup.opacity > 0.0 && popup.opacity < 1.0);
        } else {
            assert_eq!(popup.opacity, 1.0);
            assert!(!component.wants_tick());
        }
        component
            .paint_child_surface(key, &mut canvas, 1.0, (10, 10), false)
            .unwrap();
        assert!(!component.wants_render());
    }
    component.tooltip_popup.as_mut().unwrap().opacity = 0.5;
    assert!(component.wants_render());
    component.discard_child_surface_paint(key);
    assert!(!component.wants_render());
    component.request_paint();
    assert!(component.wants_parent_render());
}

#[test]
fn inline_tooltip_fade_still_invalidates_parent() {
    let mut component = common::test_frontend_component("<template><box /></template>");
    component.take_dirty_for_paint();
    component.tooltip_visible = true;
    component.tooltip_appeared_at = Some(Instant::now() - Duration::from_secs(1));
    component.tick().unwrap();
    assert!(component.wants_parent_render());
    assert!(!component.wants_tick());
}

#[test]
fn timer_binding_patches_survive_child_popover_lifecycle() {
    let mut rebuilds = 0;
    for entering in [true, false] {
        let mut component = common::test_frontend_component(
            r#"
<template><box><text>{counter}</text><popover open="true"><box class={"panel tick-" .. counter}><text>{counter}</text></box></popover></box></template>
<script lang="luau">
counter = 0
task.spawn(function()
    while true do
        task.sleep(1000)
        counter += 1
    end
end)
</script>
<style>
box { width: 200px; height: 60px; }
.panel { width: 80px; height: 40px; transition: opacity 1s linear; }
.panel.mesh-surface-entering, .panel.mesh-surface-exiting { opacity: 0; }
</style>
"#,
        );
        let theme = mesh_core_theme::default_theme();
        let extent = SurfaceExtent::unpadded(300, 100);
        let mut canvas = SurfaceCanvas::from(PixelBuffer::new(300, 100));
        component.paint(&theme, extent, &mut canvas, 1.0).unwrap();
        let key = component.child_surface_requests().remove(0).node_key;
        if entering {
            component.set_entering_child_keys([key].into_iter().collect());
        } else {
            component.set_closing_child_keys([key].into_iter().collect());
        }
        component.paint(&theme, extent, &mut canvas, 1.0).unwrap();
        component.set_profiling_enabled(true);
        let mut phase_rebuilds = 0;
        for tick in 1..=3 {
            let due = component.next_script_task_deadline().unwrap();
            component.run_due_script_tasks(due).unwrap();
            component.paint(&theme, extent, &mut canvas, 1.0).unwrap();
            let snapshot = component.take_invalidation_snapshot().unwrap();
            let rebuilt = snapshot.node_visits["validation"] > 0;
            phase_rebuilds += usize::from(rebuilt);
            let panel = common::first_node_by_class(component.last_tree.as_ref().unwrap(), "panel")
                .unwrap();
            assert!(
                panel
                    .class_tokens()
                    .iter()
                    .any(|class| class == &format!("tick-{tick}"))
            );
            assert!(panel.class_tokens().iter().any(|class| class
                == if entering {
                    "mesh-surface-entering"
                } else {
                    "mesh-surface-exiting"
                }));
            if !rebuilt {
                assert_eq!(snapshot.node_visits["validation"], 0);
                assert_eq!(snapshot.patched_node_count, 3);
            }
        }
        eprintln!("MESH_PERF entering={entering} ticks=3 rebuilds={phase_rebuilds}");
        rebuilds += phase_rebuilds;
        component.set_entering_child_keys(Default::default());
        component.set_closing_child_keys(Default::default());
        component.paint(&theme, extent, &mut canvas, 1.0).unwrap();
        let panel =
            common::first_node_by_class(component.last_tree.as_ref().unwrap(), "panel").unwrap();
        assert!(panel.class_tokens().iter().any(|class| class == "tick-3"));
        assert!(
            !panel
                .class_tokens()
                .iter()
                .any(|class| class.starts_with("mesh-surface-"))
        );
    }
    assert_eq!(
        rebuilds, 0,
        "bound timer writes must patch during child lifecycle"
    );
}
