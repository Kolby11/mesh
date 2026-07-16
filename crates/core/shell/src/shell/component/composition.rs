use std::collections::{BTreeMap, HashMap};

use mesh_core_elements::style::Dimension;
use mesh_core_elements::{EventHandlerCall, WidgetNode};
use mesh_core_frontend::FrontendCompositionResolver;
use mesh_core_interaction::source_element_tag;
use mesh_core_module::ModuleType;

use super::{FrontendSurfaceComponent, PROMOTED_POPOVER_MARKER, memo};

fn slot_id(module_id: &str, slot_name: &str) -> String {
    let mut id = String::with_capacity(module_id.len() + 1 + slot_name.len());
    id.push_str(module_id);
    id.push(':');
    id.push_str(slot_name);
    id
}

impl FrontendSurfaceComponent {
    fn record_component_instance_build(
        &self,
        instance_key: &str,
        module_id: &str,
        started: Option<std::time::Instant>,
    ) {
        let Some(started) = started else {
            return;
        };
        self.profiling_records.borrow_mut().push(
            mesh_core_frontend_host::ComponentProfilingRecord {
                stage: mesh_core_debug::ProfilingStage::TreeBuild,
                duration: started.elapsed(),
                module_id: Some(module_id.to_owned()),
                trigger_kind: Some(format!("attribution:component_instance:{instance_key}")),
            },
        );
    }

    fn record_avoided_component_build(&self) {
        if !self.profiling_enabled {
            return;
        }
        self.profiling_records.borrow_mut().push(
            mesh_core_frontend_host::ComponentProfilingRecord {
                stage: mesh_core_debug::ProfilingStage::TreeBuild,
                duration: std::time::Duration::ZERO,
                module_id: Some(self.compiled.manifest.package.id.clone()),
                trigger_kind: Some("waste:component_build_avoided".to_owned()),
            },
        );
    }

    fn next_loop_occurrence(
        &self,
        host_instance_key: &str,
        source_ordinal: usize,
        repeated_by_loop: bool,
    ) -> Option<usize> {
        if !repeated_by_loop {
            return None;
        }
        let host_instance_key = self.instance_keys.borrow_mut().intern(host_instance_key);
        let mut occurrences = self.composition_occurrences.borrow_mut();
        let next = occurrences
            .entry((host_instance_key, source_ordinal))
            .or_default();
        let ordinal = *next;
        *next += 1;
        Some(ordinal)
    }
}

impl FrontendCompositionResolver for FrontendSurfaceComponent {
    fn evaluate_template_expression(
        &self,
        instance_key: &str,
        expression: &str,
        locals: &serde_json::Map<String, serde_json::Value>,
    ) -> Option<mesh_core_frontend::TemplateExpressionResult> {
        let runtimes = self.runtimes.lock().ok()?;
        let runtime = runtimes.get(instance_key)?;
        match runtime
            .script_ctx
            .evaluate_template_expression(expression, locals)
        {
            Ok((value, service_reads)) => Some(mesh_core_frontend::TemplateExpressionResult {
                value,
                service_reads,
            }),
            Err(error) => {
                tracing::warn!(instance_key, expression, %error, "template expression failed");
                Some(mesh_core_frontend::TemplateExpressionResult {
                    value: serde_json::Value::Null,
                    service_reads: Vec::new(),
                })
            }
        }
    }

    fn render_import(
        &self,
        host: &mesh_core_module::Manifest,
        host_instance_key: &str,
        alias: &str,
        source_ordinal: usize,
        duplicate_ordinal: Option<usize>,
        repeated_by_loop: bool,
        props: &BTreeMap<String, String>,
        prop_handler_calls: &BTreeMap<String, EventHandlerCall>,
        container_width: f32,
        container_height: f32,
    ) -> Option<WidgetNode> {
        let loop_ordinal =
            self.next_loop_occurrence(host_instance_key, source_ordinal, repeated_by_loop);
        if let Some(entry) = self.frontend_catalog.modules.get(&host.package.id) {
            if let Some(component) = entry.compiled.local_components.get(alias) {
                let instance_key = self.instance_keys.borrow_mut().intern_embedded_occurrence(
                    host_instance_key,
                    "local",
                    alias,
                    duplicate_ordinal,
                    loop_ordinal,
                );
                let props_fingerprint =
                    memo::component_props_fingerprint(props, prop_handler_calls);
                if let Some(node) = self.lookup_component_memo(
                    &instance_key,
                    props_fingerprint,
                    container_width,
                    container_height,
                ) {
                    self.record_avoided_component_build();
                    return Some(node);
                }
                let marks_before = self.memo_effect_marks();
                let build_started = self.profiling_enabled.then(std::time::Instant::now);
                let bind_this = props.get("__mesh_bind_this").cloned();
                let props_json = runtime_props_json(props);
                let mut node = self.render_local_component(
                    &entry.compiled.manifest,
                    alias,
                    component,
                    &instance_key,
                    &props_json,
                    container_width,
                    container_height,
                    entry
                        .compiled
                        .component
                        .style
                        .as_ref()
                        .map(|style| style.rules.as_slice())
                        .unwrap_or(&[]),
                );
                let source_path = local_component_source_path(&entry.compiled, alias);
                annotate_source_file(&mut node, &source_path);
                apply_prop_handler_calls(&mut node, props, prop_handler_calls);
                if let Some(binding) = bind_this.and_then(|value| simple_state_binding(&value)) {
                    self.bind_child_instance(host_instance_key, &binding, &instance_key);
                }
                self.store_component_memo(
                    &instance_key,
                    props_fingerprint,
                    container_width,
                    container_height,
                    marks_before,
                    &node,
                );
                self.record_component_instance_build(
                    &instance_key,
                    &entry.compiled.manifest.package.id,
                    build_started,
                );
                return Some(node);
            }
        }

        let module_id = match self
            .frontend_catalog
            .imported_component_module_id(host, alias)
        {
            Ok(id) => id,
            Err(message) => return Some(self.build_error_widget(message)),
        };

        // Surface modules are portals: their visibility is tracked via pending_surface_states
        // and translated to ShowSurface/HideSurface requests in tick(). They render nothing inline.
        let is_surface = self
            .frontend_catalog
            .modules
            .get(&module_id)
            .map(|e| e.compiled.manifest.package.module_type == ModuleType::Surface)
            .unwrap_or(false);
        if is_surface {
            let hidden = props
                .get("hidden")
                .map(|v| v == "true" || v == "True")
                .unwrap_or(false);
            if let Some(binding) = props
                .get("__mesh_binding_hidden")
                .and_then(|binding| simple_state_binding(binding))
            {
                self.portal_hidden_bindings.borrow_mut().insert(
                    module_id.clone(),
                    (
                        self.instance_keys.borrow_mut().intern(host_instance_key),
                        binding,
                    ),
                );
            }
            self.pending_surface_states
                .borrow_mut()
                .insert(module_id, !hidden);
            // Portal visibility must be re-published on every build; an
            // enclosing subtree containing this write is not memoizable.
            self.portal_state_writes
                .set(self.portal_state_writes.get().wrapping_add(1));
            let mut placeholder = WidgetNode::new("box");
            placeholder.computed_style.width = Dimension::Px(0.0);
            placeholder.computed_style.height = Dimension::Px(0.0);
            placeholder
                .attributes
                .insert("hidden".into(), "true".into());
            return Some(placeholder);
        }

        let instance_key = self.instance_keys.borrow_mut().intern_embedded_occurrence(
            host_instance_key,
            "import",
            alias,
            duplicate_ordinal,
            loop_ordinal,
        );
        let props_fingerprint = memo::component_props_fingerprint(props, prop_handler_calls);
        if let Some(node) = self.lookup_component_memo(
            &instance_key,
            props_fingerprint,
            container_width,
            container_height,
        ) {
            self.record_avoided_component_build();
            return Some(node);
        }
        let marks_before = self.memo_effect_marks();
        let build_started = self.profiling_enabled.then(std::time::Instant::now);
        let props_json = runtime_props_json(props);
        let bind_this = props.get("__mesh_bind_this").cloned();
        let mut node = self.render_embedded_instance(
            &instance_key,
            &module_id,
            &props_json,
            container_width,
            container_height,
        );
        apply_prop_handler_calls(&mut node, props, prop_handler_calls);
        if let Some(binding) = bind_this.and_then(|value| simple_state_binding(&value)) {
            self.bind_child_instance(host_instance_key, &binding, &instance_key);
        }
        // Inline component modules whose root element is a `<popover>` are never
        // painted inline: the popover is realized as a promoted child `xdg_popup`
        // surface. Mark the embedded wrapper as hidden (so it is skipped by parent
        // painting and hit-testing) and tag it for out-of-flow collapse. The actual
        // `position: absolute` geometry is applied in `finalize_tree` AFTER the
        // restyle pass, because restyle re-resolves `computed_style` purely from CSS
        // and would otherwise wipe any geometry set here. Taking the wrapper out of
        // flow keeps its (full-size) popover subtree intact for
        // `collect_child_surface_requests()` and child-surface painting while
        // preventing it from contributing to the trigger row's layout — otherwise
        // the resting popover would widen the control cluster and overlap
        // neighbouring buttons. Open and closed popovers collapse identically so
        // toggling a hover popover never relayouts its trigger; only open popovers
        // are additionally promoted to a child surface.
        if embedded_root_is_popover(&node) {
            self.has_promoted_popover_wrappers.set(true);
            self.popover_wrapper_marks
                .set(self.popover_wrapper_marks.get().wrapping_add(1));
            node.attributes.insert("hidden".into(), "true".into());
            node.attributes
                .insert(PROMOTED_POPOVER_MARKER.into(), "true".into());
        }
        self.store_component_memo(
            &instance_key,
            props_fingerprint,
            container_width,
            container_height,
            marks_before,
            &node,
        );
        self.record_component_instance_build(&instance_key, &module_id, build_started);
        Some(node)
    }

    fn render_slot(
        &self,
        host: &mesh_core_module::Manifest,
        host_instance_key: &str,
        slot_name: Option<&str>,
        container_width: f32,
        container_height: f32,
    ) -> Vec<WidgetNode> {
        let Some(slot_name) = slot_name else {
            return Vec::new();
        };

        let slot_id = slot_id(&host.package.id, slot_name);
        let accepts_widget = host
            .provides_slots
            .get(slot_name)
            .and_then(|definition| definition.accepts.as_deref())
            .map(|accepts| accepts == "widget")
            .unwrap_or(false);

        let contributions = self.frontend_catalog.slot_contributions_for(&slot_id);
        let mut nodes = Vec::with_capacity(contributions.len());
        for contribution in contributions {
            let Some(entry) = self.frontend_catalog.modules.get(&contribution.widget_id) else {
                nodes.push(self.build_error_widget(format!(
                    "slot '{slot_id}' references missing module '{}'",
                    contribution.widget_id
                )));
                continue;
            };

            let module_type = entry.compiled.manifest.package.module_type;
            if accepts_widget && !matches!(module_type, ModuleType::Widget | ModuleType::Component)
            {
                nodes.push(self.build_error_widget(format!(
                    "slot '{slot_id}' accepts widgets, but '{}' is {}",
                    contribution.widget_id, module_type
                )));
                continue;
            }

            let instance_key = self.instance_keys.borrow_mut().intern_slot(
                host_instance_key,
                slot_name,
                &contribution.contribution_id,
            );
            let mut node = if let Some(node) = self.lookup_component_memo(
                &instance_key,
                contribution.props_fingerprint,
                container_width,
                container_height,
            ) {
                self.record_avoided_component_build();
                node
            } else {
                let marks_before = self.memo_effect_marks();
                let build_started = self.profiling_enabled.then(std::time::Instant::now);
                let props_json: HashMap<String, serde_json::Value> = contribution
                    .props
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect();
                let node = self.render_embedded_instance(
                    &instance_key,
                    &contribution.widget_id,
                    &props_json,
                    container_width,
                    container_height,
                );
                self.store_component_memo(
                    &instance_key,
                    contribution.props_fingerprint,
                    container_width,
                    container_height,
                    marks_before,
                    &node,
                );
                self.record_component_instance_build(
                    &instance_key,
                    &contribution.widget_id,
                    build_started,
                );
                node
            };
            node.attributes.insert(
                "_mesh_slot_source".into(),
                contribution.source_module_id.clone(),
            );
            nodes.push(node);
        }

        nodes
    }
}

pub(super) fn annotate_source_file(node: &mut WidgetNode, source_path: &str) {
    node.attributes
        .insert("_mesh_source_file".into(), source_path.into());
    for child in &mut node.children {
        annotate_source_file(child, source_path);
    }
}

fn local_component_source_path(
    compiled: &mesh_core_frontend::CompiledFrontendModule,
    alias: &str,
) -> String {
    let normalized_alias = alias
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    compiled
        .watched_paths
        .iter()
        .find(|path| {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .map(|stem| {
                    stem.chars()
                        .filter(|ch| ch.is_ascii_alphanumeric())
                        .flat_map(char::to_lowercase)
                        .collect::<String>()
                        == normalized_alias
                })
                .unwrap_or(false)
        })
        .unwrap_or(&compiled.source_path)
        .display()
        .to_string()
}

/// Returns true when an embedded component's rendered tree has a `<popover>` as its
/// top-level content, regardless of open state. The root node from
/// `build_tree_with_state` is always a "surface" wrapper, so the actual element is the
/// first child. The popover element paints as a generic `box`, carrying its identity on
/// `data-mesh-element`, so match on `source_element_tag` rather than the raw render tag.
fn embedded_root_is_popover(node: &WidgetNode) -> bool {
    node.children
        .first()
        .is_some_and(|child| source_element_tag(child) == "popover")
}

fn runtime_props_json(props: &BTreeMap<String, String>) -> HashMap<String, serde_json::Value> {
    // Typical embedded components have only a few props. Avoid a separate
    // count pass there while keeping precise capacity for larger prop maps.
    let capacity = if props.len() <= 8 {
        props.len()
    } else {
        props
            .keys()
            .filter(|key| runtime_prop_is_public(key.as_str()))
            .count()
    };
    let mut props_json = HashMap::with_capacity(capacity);
    for (key, value) in props {
        if runtime_prop_is_public(key) {
            props_json.insert(key.clone(), decode_prop_value(value));
        }
    }
    props_json
}

/// A bound table/array prop (e.g. `items="{items}"`) reaches this boundary
/// already JSON-stringified — the attribute resolver in `mesh-core-frontend`
/// stringifies every resolved value on the way here (`json_value_to_string`),
/// so the type information is otherwise lost. Recover it: a value that looks
/// like a JSON array/object is parsed back into structured JSON so it lands
/// in the child's Luau `_ENV` as a real table, not a stringified blob.
/// Anything else (the overwhelming majority of props: plain text, numbers,
/// booleans) is passed through unchanged.
fn decode_prop_value(value: &str) -> serde_json::Value {
    match value.trim_start().as_bytes().first() {
        Some(b'[') | Some(b'{') => serde_json::from_str(value)
            .unwrap_or_else(|_| serde_json::Value::String(value.to_string())),
        _ => serde_json::Value::String(value.to_string()),
    }
}

fn runtime_prop_is_public(key: &str) -> bool {
    !key.starts_with("__mesh_binding_") && key != "__mesh_bind_this"
}

fn apply_prop_handler_calls(
    node: &mut WidgetNode,
    props: &BTreeMap<String, String>,
    prop_handler_calls: &BTreeMap<String, EventHandlerCall>,
) {
    if prop_handler_calls.is_empty() {
        return;
    }
    if prop_handler_calls.len() == 1 {
        let (prop_name, call) = prop_handler_calls
            .first_key_value()
            .expect("single prop handler exists");
        if let Some(handler) = props.get(prop_name) {
            apply_single_prop_handler_call(node, handler, call);
        }
        return;
    }
    let mut handlers_by_value = HashMap::with_capacity(prop_handler_calls.len());
    for (prop_name, call) in prop_handler_calls {
        let Some(handler) = props.get(prop_name) else {
            continue;
        };
        // Preserve the old BTreeMap iteration behavior when multiple props
        // resolve to the same handler: the first prop wins.
        handlers_by_value.entry(handler.as_str()).or_insert(call);
    }
    apply_indexed_prop_handler_calls(node, &handlers_by_value);
}

fn apply_single_prop_handler_call(
    node: &mut WidgetNode,
    target_handler: &str,
    call: &EventHandlerCall,
) {
    for (event_name, handler) in &node.event_handlers {
        if handler == target_handler {
            node.event_handler_calls.insert(
                event_name.clone(),
                EventHandlerCall {
                    handler: handler.clone(),
                    args: call.args.clone(),
                },
            );
        }
    }
    for child in &mut node.children {
        apply_single_prop_handler_call(child, target_handler, call);
    }
}

fn apply_indexed_prop_handler_calls(
    node: &mut WidgetNode,
    handlers_by_value: &HashMap<&str, &EventHandlerCall>,
) {
    let mut handler_calls = Vec::with_capacity(node.event_handlers.len());
    for (event_name, handler) in &node.event_handlers {
        let Some(call) = handlers_by_value.get(handler.as_str()) else {
            continue;
        };
        handler_calls.push((
            event_name.clone(),
            EventHandlerCall {
                handler: handler.clone(),
                args: call.args.clone(),
            },
        ));
    }
    for (event_name, call) in handler_calls {
        node.event_handler_calls.insert(event_name, call);
    }
    for child in &mut node.children {
        apply_indexed_prop_handler_calls(child, handlers_by_value);
    }
}

fn simple_state_binding(binding: &str) -> Option<String> {
    let trimmed = binding.trim();
    if trimmed.is_empty()
        || !trimmed
            .chars()
            .all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
        || trimmed.chars().next().is_some_and(|ch| ch.is_ascii_digit())
    {
        return None;
    }
    Some(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn slot_instance_key_matches_legacy_format() {
        let mut interner = super::super::InstanceKeyInterner::default();
        let slot = interner.intern_slot("@mesh/panel/local:Toolbar", "main", "battery-status");
        assert_eq!(&*slot, "@mesh/panel/local:Toolbar/slot:main/battery-status");
        let repeated = interner.intern_slot("@mesh/panel/local:Toolbar", "main", "battery-status");
        assert!(std::sync::Arc::ptr_eq(&slot, &repeated));
        assert_eq!(
            &*interner.intern_embedded("@mesh/panel/local:Toolbar", "local", "BatteryStatus"),
            "@mesh/panel/local:Toolbar/local:BatteryStatus"
        );
        assert_eq!(
            &*interner.intern_embedded("@mesh/panel/local:Toolbar", "import", "audio_controls"),
            "@mesh/panel/local:Toolbar/import:audio_controls"
        );
        assert_eq!(
            slot_id("@mesh/panel", "primary_actions"),
            "@mesh/panel:primary_actions"
        );
    }

    // cargo test -p mesh-core-shell --release -- slot_instance_key_interning_beats_rebuilding_benchmark --ignored --nocapture
    #[test]
    #[ignore = "release-only slot instance-key construction microbenchmark"]
    fn slot_instance_key_interning_beats_rebuilding_benchmark() {
        let host = "@mesh/panel/local:Toolbar/import:StatusCluster";
        let slot = "primary_actions";
        let contribution = "network-status-very-long-contribution-identifier";
        let iterations = 1_000_000;

        let old_started = Instant::now();
        let mut old_total = 0usize;
        for _ in 0..iterations {
            old_total ^= std::hint::black_box(format!("{host}/slot:{slot}/{contribution}").len());
        }
        let old_time = old_started.elapsed();

        let new_started = Instant::now();
        let mut new_total = 0usize;
        let mut interner = super::super::InstanceKeyInterner::default();
        for _ in 0..iterations {
            new_total ^= std::hint::black_box(interner.intern_slot(host, slot, contribution).len());
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "slot instance key: rebuild {old_time:?}; interned {new_time:?}; ratio {:.2}x",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_total, new_total);
        assert!(new_time < old_time);
    }

    // cargo test -p mesh-core-shell --release -- slot_id_presizing_beats_format_benchmark --ignored --nocapture
    #[test]
    #[ignore = "release-only slot id construction microbenchmark"]
    fn slot_id_presizing_beats_format_benchmark() {
        let module_id = "@mesh/panel/local:StatusCluster/import:NetworkControls";
        let name = "primary_actions";
        let iterations = 1_000_000;

        let old_started = Instant::now();
        let mut old_total = 0usize;
        for _ in 0..iterations {
            old_total ^= std::hint::black_box(format!("{module_id}:{name}").len());
        }
        let old_time = old_started.elapsed();

        let new_started = Instant::now();
        let mut new_total = 0usize;
        for _ in 0..iterations {
            new_total ^= std::hint::black_box(slot_id(module_id, name).len());
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "slot id: format {old_time:?}; presized {new_time:?}; ratio {:.2}x",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_total, new_total);
        assert!(new_time < old_time);
    }

    fn handler_tree(child_count: usize) -> WidgetNode {
        let mut root = WidgetNode::new("box");
        root.event_handlers.insert("click".into(), "onClick".into());
        root.event_handlers
            .insert("pointermove".into(), "onMove".into());
        root.children = (0..child_count)
            .map(|index| {
                let mut child = WidgetNode::new("button");
                child
                    .event_handlers
                    .insert("click".into(), format!("onChild{index}"));
                child
                    .event_handlers
                    .insert("pointermove".into(), "onMove".into());
                child
            })
            .collect();
        root
    }

    fn old_apply_prop_handler_calls(
        node: &mut WidgetNode,
        props: &BTreeMap<String, String>,
        prop_handler_calls: &BTreeMap<String, EventHandlerCall>,
    ) {
        if prop_handler_calls.is_empty() {
            return;
        }
        for (event_name, handler) in node.event_handlers.clone() {
            let Some((_, call)) = prop_handler_calls
                .iter()
                .find(|(prop_name, _)| props.get(*prop_name) == Some(&handler))
            else {
                continue;
            };
            node.event_handler_calls.insert(
                event_name,
                EventHandlerCall {
                    handler,
                    args: call.args.clone(),
                },
            );
        }
        for child in &mut node.children {
            old_apply_prop_handler_calls(child, props, prop_handler_calls);
        }
    }

    fn borrow_scan_prop_handler_calls(
        node: &mut WidgetNode,
        props: &BTreeMap<String, String>,
        prop_handler_calls: &BTreeMap<String, EventHandlerCall>,
    ) {
        let handler_calls = node
            .event_handlers
            .iter()
            .filter_map(|(event_name, handler)| {
                prop_handler_calls
                    .iter()
                    .find(|(prop_name, _)| props.get(*prop_name) == Some(handler))
                    .map(|(_, call)| {
                        (
                            event_name.clone(),
                            EventHandlerCall {
                                handler: handler.clone(),
                                args: call.args.clone(),
                            },
                        )
                    })
            })
            .collect::<Vec<_>>();
        for (event_name, call) in handler_calls {
            node.event_handler_calls.insert(event_name, call);
        }
        for child in &mut node.children {
            borrow_scan_prop_handler_calls(child, props, prop_handler_calls);
        }
    }

    #[test]
    fn prop_handler_calls_still_bind_matching_handlers() {
        let mut node = handler_tree(2);
        let props = BTreeMap::from([("onMoveProp".into(), "onMove".into())]);
        let calls = BTreeMap::from([(
            "onMoveProp".into(),
            EventHandlerCall {
                handler: "handleMove".into(),
                args: vec![serde_json::json!("bound")],
            },
        )]);

        apply_prop_handler_calls(&mut node, &props, &calls);

        assert_eq!(
            node.event_handler_calls
                .get("pointermove")
                .map(|call| call.handler.as_str()),
            Some("onMove")
        );
        assert_eq!(
            node.children[0]
                .event_handler_calls
                .get("pointermove")
                .map(|call| call.handler.as_str()),
            Some("onMove")
        );
    }

    #[test]
    fn runtime_props_json_filters_internal_binding_props() {
        let props = BTreeMap::from([
            ("label".into(), "Volume".into()),
            ("__mesh_binding_hidden".into(), "isHidden".into()),
            ("__mesh_bind_this".into(), "child".into()),
        ]);

        let props_json = runtime_props_json(&props);

        assert_eq!(
            props_json.get("label"),
            Some(&serde_json::Value::String("Volume".into()))
        );
        assert!(!props_json.contains_key("__mesh_binding_hidden"));
        assert!(!props_json.contains_key("__mesh_bind_this"));
    }

    #[test]
    fn runtime_props_json_recovers_array_and_object_props_as_structured_json() {
        // A bound table prop (e.g. `items="{items}"`) arrives here already
        // JSON-stringified by the attribute resolver upstream. It must come
        // back out as a real array/object, not a string blob the child's
        // `{#for item in items}` can't iterate.
        let props = BTreeMap::from([
            (
                "items".into(),
                r#"[{"id":"en","text":"EN"},{"id":"sk","text":"SK"}]"#.into(),
            ),
            ("config".into(), r#"{"enabled":true}"#.into()),
            ("label".into(), "Volume".into()),
            // Looks table-ish at a glance but isn't valid JSON: must fall
            // back to a plain string rather than being dropped or panicking.
            ("weird".into(), "[not json".into()),
        ]);

        let props_json = runtime_props_json(&props);

        assert_eq!(
            props_json.get("items"),
            Some(&serde_json::json!([
                {"id": "en", "text": "EN"},
                {"id": "sk", "text": "SK"},
            ]))
        );
        assert_eq!(
            props_json.get("config"),
            Some(&serde_json::json!({"enabled": true}))
        );
        assert_eq!(
            props_json.get("label"),
            Some(&serde_json::Value::String("Volume".into()))
        );
        assert_eq!(
            props_json.get("weird"),
            Some(&serde_json::Value::String("[not json".into()))
        );
    }

    // cargo test -p mesh-core-shell --release -- presized_runtime_props_json_beats_filtered_collect --ignored --nocapture
    #[test]
    #[ignore = "release-only runtime prop map construction microbenchmark"]
    fn presized_runtime_props_json_beats_filtered_collect() {
        fn old_runtime_props_json(
            props: &BTreeMap<String, String>,
        ) -> HashMap<String, serde_json::Value> {
            props
                .iter()
                .filter(|(key, _)| {
                    !key.starts_with("__mesh_binding_") && key.as_str() != "__mesh_bind_this"
                })
                .map(|(key, value)| (key.clone(), serde_json::Value::String(value.clone())))
                .collect()
        }

        let mut props = BTreeMap::new();
        for index in 0..64 {
            props.insert(format!("prop{index}"), format!("value{index}"));
            props.insert(
                format!("__mesh_binding_prop{index}"),
                format!("state{index}"),
            );
        }
        props.insert("__mesh_bind_this".into(), "child".into());
        let iterations = 100_000;

        let old_started = Instant::now();
        let mut old_total = 0usize;
        for _ in 0..iterations {
            old_total += old_runtime_props_json(std::hint::black_box(&props)).len();
        }
        let old_time = old_started.elapsed();

        let new_started = Instant::now();
        let mut new_total = 0usize;
        for _ in 0..iterations {
            new_total += runtime_props_json(std::hint::black_box(&props)).len();
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "runtime props map: filtered collect {old_time:?}; presized helper {new_time:?}; ratio {:.1}x; totals={old_total}/{new_total}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_total, new_total);
        assert!(new_time < old_time);
    }

    // cargo test -p mesh-core-shell --release -- prop_handler_matching_skips_event_handler_map_clone --ignored --nocapture
    #[test]
    #[ignore = "release-only prop handler matching microbenchmark"]
    fn prop_handler_matching_skips_event_handler_map_clone() {
        let props = BTreeMap::from([("onSelected".into(), "missingHandler".into())]);
        let calls = BTreeMap::from([(
            "onSelected".into(),
            EventHandlerCall {
                handler: "select".into(),
                args: vec![serde_json::json!("alpha")],
            },
        )]);
        let template = handler_tree(64);
        let iterations = 50_000;

        let old_started = Instant::now();
        let mut old_total = 0usize;
        for _ in 0..iterations {
            let mut node = template.clone();
            old_apply_prop_handler_calls(
                std::hint::black_box(&mut node),
                std::hint::black_box(&props),
                std::hint::black_box(&calls),
            );
            old_total += std::hint::black_box(node.event_handler_calls.len());
        }
        let old_time = old_started.elapsed();

        let new_started = Instant::now();
        let mut new_total = 0usize;
        for _ in 0..iterations {
            let mut node = template.clone();
            apply_prop_handler_calls(
                std::hint::black_box(&mut node),
                std::hint::black_box(&props),
                std::hint::black_box(&calls),
            );
            new_total += std::hint::black_box(node.event_handler_calls.len());
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "prop handler matching: clone event map {old_time:?}; borrow scan {new_time:?}; ratio {:.1}x; totals={old_total}/{new_total}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_total, new_total);
        assert!(new_time < old_time);
    }

    // cargo test -p mesh-core-shell --release -- prop_handler_value_index_beats_repeated_prop_scan --ignored --nocapture
    #[test]
    #[ignore = "release-only prop handler value-index microbenchmark"]
    fn prop_handler_value_index_beats_repeated_prop_scan() {
        let props = (0..16)
            .map(|index| {
                let handler = if index == 15 {
                    "onMove".to_string()
                } else {
                    format!("missingHandler{index}")
                };
                (format!("onEvent{index}"), handler)
            })
            .collect::<BTreeMap<_, _>>();
        let calls = (0..16)
            .map(|index| {
                (
                    format!("onEvent{index}"),
                    EventHandlerCall {
                        handler: format!("event{index}"),
                        args: vec![serde_json::json!(index)],
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        let template = handler_tree(64);
        let iterations = 20_000;

        let scan_started = Instant::now();
        let mut scan_total = 0usize;
        for _ in 0..iterations {
            let mut node = template.clone();
            borrow_scan_prop_handler_calls(
                std::hint::black_box(&mut node),
                std::hint::black_box(&props),
                std::hint::black_box(&calls),
            );
            scan_total += node
                .children
                .iter()
                .map(|child| child.event_handler_calls.len())
                .sum::<usize>();
        }
        let scan_time = scan_started.elapsed();

        let indexed_started = Instant::now();
        let mut indexed_total = 0usize;
        for _ in 0..iterations {
            let mut node = template.clone();
            apply_prop_handler_calls(
                std::hint::black_box(&mut node),
                std::hint::black_box(&props),
                std::hint::black_box(&calls),
            );
            indexed_total += node
                .children
                .iter()
                .map(|child| child.event_handler_calls.len())
                .sum::<usize>();
        }
        let indexed_time = indexed_started.elapsed();

        eprintln!(
            "prop handler value lookup: repeated scan {scan_time:?}; indexed {indexed_time:?}; ratio {:.1}x; totals={scan_total}/{indexed_total}",
            scan_time.as_secs_f64() / indexed_time.as_secs_f64()
        );
        assert_eq!(scan_total, indexed_total);
        assert!(indexed_time < scan_time);
    }

    // cargo test -p mesh-core-shell --release -- single_prop_handler_fast_path_beats_repeated_map_scan --ignored --nocapture
    #[test]
    #[ignore = "release-only single prop-handler microbenchmark"]
    fn single_prop_handler_fast_path_beats_repeated_map_scan() {
        let props = BTreeMap::from([("onMoveProp".into(), "onMove".into())]);
        let calls = BTreeMap::from([(
            "onMoveProp".into(),
            EventHandlerCall {
                handler: "handleMove".into(),
                args: vec![serde_json::json!("bound")],
            },
        )]);
        let template = handler_tree(64);
        let iterations = 50_000;

        let scan_started = Instant::now();
        let mut scan_total = 0usize;
        for _ in 0..iterations {
            let mut node = template.clone();
            borrow_scan_prop_handler_calls(&mut node, &props, &calls);
            scan_total += node
                .children
                .iter()
                .map(|child| child.event_handler_calls.len())
                .sum::<usize>();
        }
        let scan_time = scan_started.elapsed();

        let fast_started = Instant::now();
        let mut fast_total = 0usize;
        for _ in 0..iterations {
            let mut node = template.clone();
            apply_prop_handler_calls(&mut node, &props, &calls);
            fast_total += node
                .children
                .iter()
                .map(|child| child.event_handler_calls.len())
                .sum::<usize>();
        }
        let fast_time = fast_started.elapsed();

        eprintln!(
            "single prop handler: repeated map scan {scan_time:?}; specialized {fast_time:?}; ratio {:.1}x; totals={scan_total}/{fast_total}",
            scan_time.as_secs_f64() / fast_time.as_secs_f64()
        );
        assert_eq!(scan_total, fast_total);
        assert!(fast_time < scan_time);
    }
}

impl FrontendSurfaceComponent {
    pub(super) fn bind_child_instance(
        &self,
        host_instance_key: &str,
        binding: &str,
        child_instance_key: &str,
    ) {
        // Live `bind:this`: parent and child share one thread VM, so the parent
        // env holds a proxy table forwarding straight to the child's live `_ENV`.
        // Reads see current values; calls run the child's real function and return
        // its real value synchronously — no snapshot, no queued call stubs.
        let runtimes = self.runtimes.lock().unwrap();
        let (Some(parent), Some(child)) = (
            runtimes.get(host_instance_key),
            runtimes.get(child_instance_key),
        ) else {
            return;
        };
        if let Err(source) = parent
            .script_ctx
            .install_live_binding(binding, &child.script_ctx)
        {
            tracing::warn!(
                component_id = %parent.module_id,
                binding = %binding,
                child_instance_key = %child_instance_key,
                error = %source,
                "failed to install live bound child instance proxy"
            );
            return;
        }
        drop(runtimes);

        // Record the link so the parent's event handlers can re-sync this child
        // after a live cross-call mutates its `_ENV` directly.
        let mut bound_children = self.bound_children.borrow_mut();
        let links = bound_children
            .entry(self.instance_keys.borrow_mut().intern(host_instance_key))
            .or_default();
        if !links
            .iter()
            .any(|(b, key)| b == binding && key.as_ref() == child_instance_key)
        {
            links.push((
                binding.to_string(),
                self.instance_keys.borrow_mut().intern(child_instance_key),
            ));
        }
    }
}
