use std::collections::{BTreeMap, HashMap};

use mesh_core_elements::style::Dimension;
use mesh_core_elements::{AttributeMap, ComponentCompositionProps, EventHandlerCall, WidgetNode};
use mesh_core_frontend::FrontendCompositionResolver;
use mesh_core_interaction::source_element_tag;
use mesh_core_module::ModuleType;

use super::{FrontendSurfaceComponent, memo};

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
        mesh_core_debug::allocation::with_tracking_suspended(|| {
            self.profiling_records.borrow_mut().push(
                mesh_core_frontend_host::ComponentProfilingRecord {
                    stage: mesh_core_debug::ProfilingStage::TreeBuild,
                    duration: started.elapsed(),
                    module_id: Some(module_id.to_owned()),
                    trigger_kind: Some(format!("attribution:component_instance:{instance_key}")),
                },
            );
        });
    }

    fn record_avoided_component_build(&self) {
        if !self.profiling_enabled {
            return;
        }
        mesh_core_debug::allocation::with_tracking_suspended(|| {
            self.profiling_records.borrow_mut().push(
                mesh_core_frontend_host::ComponentProfilingRecord {
                    stage: mesh_core_debug::ProfilingStage::TreeBuild,
                    duration: std::time::Duration::ZERO,
                    module_id: Some(self.compiled.manifest.package.id.clone()),
                    trigger_kind: Some("waste:component_build_avoided".to_owned()),
                },
            );
        });
    }

    fn next_loop_occurrence(
        &self,
        host_instance_key: &str,
        source_ordinal: usize,
        repeated_by_loop: bool,
        loop_identity: Option<&str>,
    ) -> Option<usize> {
        if !repeated_by_loop {
            return None;
        }
        let host_instance_key = self.instance_keys.borrow_mut().intern(host_instance_key);
        let loop_identity =
            loop_identity.map(|identity| self.instance_keys.borrow_mut().intern(identity));
        let mut occurrences = self.composition_occurrences.borrow_mut();
        let next = occurrences
            .entry((host_instance_key, source_ordinal, loop_identity))
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
        expression: &mesh_core_expression::CompiledExpression,
        locals: &serde_json::Map<String, serde_json::Value>,
    ) -> Option<mesh_core_frontend::TemplateExpressionResult> {
        let runtimes = self.runtimes.lock().ok()?;
        let runtime = runtimes.get(instance_key)?;
        match runtime
            .script_ctx
            .evaluate_compiled_template_expression(expression, locals)
        {
            Ok((value, service_reads)) => Some(mesh_core_frontend::TemplateExpressionResult {
                value,
                service_reads,
            }),
            Err(error) => {
                tracing::warn!(
                    instance_key,
                    expression = %expression.source(),
                    %error,
                    "template expression failed"
                );
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
        owner_source_path: Option<&std::path::Path>,
        alias: &str,
        source_ordinal: usize,
        duplicate_ordinal: Option<usize>,
        repeated_by_loop: bool,
        loop_identity: Option<&str>,
        props: &ComponentCompositionProps,
        prop_handler_calls: &BTreeMap<String, EventHandlerCall>,
        container_width: f32,
        container_height: f32,
    ) -> Option<WidgetNode> {
        let loop_ordinal = self.next_loop_occurrence(
            host_instance_key,
            source_ordinal,
            repeated_by_loop,
            loop_identity,
        );
        let primary_compiled = self
            .frontend_catalog
            .modules
            .get(&host.package.id)
            .map(|entry| &entry.compiled);
        // A contribution root carries its own local components. Resolve the
        // owner first, then resolve the alias inside that owner's namespace;
        // another recursive owner using the same alias must not participate.
        let local_compiled = if let Some(owner) = owner_source_path {
            primary_compiled
                .filter(|compiled| compiled.owns_component_path(owner))
                .or_else(|| {
                    self.frontend_catalog
                        .contribution_entries_for(&host.package.id)
                        .find(|compiled| compiled.owns_component_path(owner))
                })
        } else {
            primary_compiled
                .filter(|compiled| compiled.local_components.contains_key(alias))
                .or_else(|| {
                    self.frontend_catalog
                        .contribution_entries_for(&host.package.id)
                        .find(|compiled| compiled.local_components.contains_key(alias))
                })
                .or(primary_compiled)
        };
        if let Some(compiled) = local_compiled
            && let Some(resolved) = compiled.local_component_for(owner_source_path, alias)
        {
            let instance_key = self.instance_keys.borrow_mut().intern_embedded_occurrence(
                host_instance_key,
                "local",
                alias,
                duplicate_ordinal,
                loop_ordinal,
                loop_identity,
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
            let bind_this = props.bind_this.clone();
            let props_json = runtime_props_json(&props.values);
            let mut node = self.render_local_component(
                &compiled.manifest,
                alias,
                &resolved.component,
                &resolved.source_path,
                &instance_key,
                &props_json,
                container_width,
                container_height,
                compiled
                    .component
                    .style
                    .as_ref()
                    .map(|style| style.rules.as_slice())
                    .unwrap_or(&[]),
            );
            annotate_source_file(&mut node, &resolved.source_path.display().to_string());
            apply_prop_handler_calls(&mut node, &props.values, prop_handler_calls);
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
                &compiled.manifest.package.id,
                build_started,
            );
            return Some(node);
        }

        let module_id =
            match self
                .frontend_catalog
                .imported_component_module_id(host, owner_source_path, alias)
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
                .values
                .get("hidden")
                .map(|v| v == "true" || v == "True")
                .unwrap_or(false);
            if let Some(binding) = props
                .bindings
                .get("hidden")
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
            loop_identity,
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
        let props_json = runtime_props_json(&props.values);
        let bind_this = props.bind_this.clone();
        let mut node = self.render_embedded_instance(
            &instance_key,
            &module_id,
            &props_json,
            container_width,
            container_height,
        );
        apply_prop_handler_calls(&mut node, &props.values, prop_handler_calls);
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
            node.mark_promoted_popover();
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
        extension_point: Option<&str>,
        slot_name: Option<&str>,
        customizable: bool,
        container_width: f32,
        container_height: f32,
    ) -> Vec<WidgetNode> {
        let Some(extension_point) = extension_point else {
            return Vec::new();
        };
        // A host renders only points it declares. An undeclared `<slot>` is a
        // graph diagnostic, not a silently empty region.
        if !host.hosted_extension_points.contains_key(extension_point) {
            return Vec::new();
        }

        let contributions = self
            .frontend_catalog
            .extension_point_contributions_for(&host.package.id, extension_point);
        let selected = if customizable {
            let Some(slot_name) = slot_name else {
                return vec![self.build_error_widget("customizable slot has no stable name")];
            };
            let requested = self
                .frontend_catalog
                .node_slot_placement(host_instance_key, slot_name)
                .map(|slot| {
                    slot.nodes
                        .iter()
                        .map(|node| {
                            (
                                node.id.clone(),
                                node.contribution.clone(),
                                node.props.clone(),
                            )
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_else(|| {
                    host.hosted_extension_points
                        .get(extension_point)
                        .and_then(|hosted| hosted.slots.get(slot_name))
                        .map(|slot| {
                            slot.defaults
                                .iter()
                                .enumerate()
                                .map(|(index, reference)| {
                                    (
                                        format!("default-{index}"),
                                        reference.clone(),
                                        serde_json::Map::new(),
                                    )
                                })
                                .collect()
                        })
                        .unwrap_or_default()
                });
            let mut selected = Vec::with_capacity(requested.len());
            for (placement_id, reference, prop_overrides) in requested {
                let Some((source_module_id, contribution_id)) = reference.rsplit_once(':') else {
                    selected.push(Err(format!(
                        "slot '{slot_name}' has invalid contribution reference '{reference}'"
                    )));
                    continue;
                };
                let Some(contribution) = contributions.iter().find(|entry| {
                    entry.source_module_id == source_module_id
                        && entry.contribution_id == contribution_id
                }) else {
                    selected.push(Err(format!(
                        "slot '{slot_name}' cannot resolve contribution '{reference}'"
                    )));
                    continue;
                };
                let mut contribution = contribution.clone();
                contribution.props.extend(prop_overrides);
                contribution.props_fingerprint =
                    super::memo::slot_props_fingerprint(&contribution.props);
                selected.push(Ok((placement_id, contribution)));
            }
            selected
        } else {
            contributions
                .iter()
                .cloned()
                .map(|contribution| Ok((contribution.contribution_id.clone(), contribution)))
                .collect()
        };

        let mut nodes = Vec::with_capacity(selected.len());
        for selected in selected {
            let (placement_id, contribution) = match selected {
                Ok(selected) => selected,
                Err(message) => {
                    nodes.push(self.build_error_widget(message));
                    continue;
                }
            };
            let Some(compiled) = self.frontend_catalog.contribution_entry(
                &contribution.source_module_id,
                &contribution.contribution_id,
            ) else {
                nodes.push(self.build_error_widget(format!(
                    "extension point '{extension_point}' has no compiled entry for '{}' from '{}'",
                    contribution.contribution_id, contribution.source_module_id
                )));
                continue;
            };

            let instance_key = self.instance_keys.borrow_mut().intern_slot(
                host_instance_key,
                slot_name.unwrap_or(extension_point),
                &placement_id,
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
                let node = self.render_embedded_compiled_instance(
                    &instance_key,
                    &contribution.source_module_id,
                    compiled,
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
                    &contribution.source_module_id,
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

fn runtime_props_json(props: &AttributeMap) -> HashMap<String, serde_json::Value> {
    let mut props_json = HashMap::with_capacity(props.len());
    for (key, value) in props.iter_values() {
        let value = value.to_json_value();
        if mesh_core_component::json_to_prop_value_ref(&value).is_ok() {
            props_json.insert(key.as_str().to_string(), value);
        }
    }
    props_json
}

fn apply_prop_handler_calls(
    node: &mut WidgetNode,
    props: &AttributeMap,
    prop_handler_calls: &BTreeMap<String, EventHandlerCall>,
) {
    if prop_handler_calls.is_empty() {
        return;
    }
    let mut calls_by_token = HashMap::with_capacity(prop_handler_calls.len());
    for (prop_name, call) in prop_handler_calls {
        let Some(token) = props.get(prop_name.as_str()) else {
            continue;
        };
        calls_by_token.insert(token.as_str(), call);
    }
    apply_indexed_prop_handler_calls(node, &calls_by_token);
}

fn apply_indexed_prop_handler_calls(
    node: &mut WidgetNode,
    calls_by_token: &HashMap<&str, &EventHandlerCall>,
) {
    // Most event-handler-bearing nodes in an embedded subtree carry plain
    // script handler names, not one of the few prop-bound-call tokens, so
    // `handler_calls` is usually empty; start unallocated instead of
    // pre-sizing for every one of this node's handlers regardless of match.
    let mut handler_calls = Vec::new();
    for (event_name, handler) in &node.event_handlers {
        let Some(call) = calls_by_token.get(handler.as_str()) else {
            continue;
        };
        let mut args = call.args.clone();
        if let Some(local_call) = node.event_handler_calls.get(event_name) {
            args.extend(local_call.args.iter().cloned());
        }
        handler_calls.push((
            event_name.clone(),
            EventHandlerCall {
                handler: call.handler.clone(),
                args,
            },
        ));
    }
    for (event_name, call) in handler_calls {
        node.event_handlers
            .insert(event_name.clone(), call.handler.clone());
        node.event_handler_calls.insert(event_name, call);
    }
    for child in &mut node.children {
        apply_indexed_prop_handler_calls(child, calls_by_token);
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
    use mesh_core_elements::HandlerTarget;
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
    }

    // cargo test -p mesh-core-shell --release -- unmatched_prop_handler_calls_skip_presized_vec --ignored --nocapture
    #[test]
    #[ignore = "release-only prop handler call indexing microbenchmark"]
    fn unmatched_prop_handler_calls_skip_presized_vec() {
        fn old_apply_indexed_prop_handler_calls(
            node: &mut WidgetNode,
            calls_by_token: &HashMap<&str, &EventHandlerCall>,
        ) {
            let mut handler_calls = Vec::with_capacity(node.event_handlers.len());
            for (event_name, handler) in &node.event_handlers {
                let Some(call) = calls_by_token.get(handler.as_str()) else {
                    continue;
                };
                handler_calls.push((
                    event_name.clone(),
                    EventHandlerCall {
                        handler: call.handler.clone(),
                        args: call.args.clone(),
                    },
                ));
            }
            for (event_name, call) in handler_calls {
                node.event_handlers
                    .insert(event_name.clone(), call.handler.clone());
                node.event_handler_calls.insert(event_name, call);
            }
            for child in &mut node.children {
                old_apply_indexed_prop_handler_calls(child, calls_by_token);
            }
        }

        // A realistic embedded subtree: most nodes have a handful of plain
        // script event handlers (onclick/onchange/...), none of which happen
        // to be the specific prop-bound-call token this component passed down.
        fn build_tree(width: usize, depth: usize) -> WidgetNode {
            let mut node = WidgetNode::new("button");
            node.event_handlers
                .insert("onclick".to_string(), "handleClick".into());
            node.event_handlers
                .insert("onpointerenter".to_string(), "handleHoverEnter".into());
            node.event_handlers
                .insert("onpointerleave".to_string(), "handleHoverLeave".into());
            if depth > 0 {
                node.children = (0..width).map(|_| build_tree(width, depth - 1)).collect();
            }
            node
        }

        fn count_nodes(node: &WidgetNode) -> usize {
            1 + node.children.iter().map(count_nodes).sum::<usize>()
        }

        let tree = build_tree(5, 4);
        let node_count = count_nodes(&tree);
        let call = EventHandlerCall {
            handler: HandlerTarget::embedded("@mesh/panel/local:Toolbar", "onSelect"),
            args: Vec::new(),
        };
        let mut calls_by_token: HashMap<&str, &EventHandlerCall> = HashMap::new();
        calls_by_token.insert("onSelectRequested", &call);
        let iterations = 2_000;

        let old_started = Instant::now();
        let mut old_tree = tree.clone();
        for _ in 0..iterations {
            old_apply_indexed_prop_handler_calls(
                std::hint::black_box(&mut old_tree),
                std::hint::black_box(&calls_by_token),
            );
        }
        let old_time = old_started.elapsed();

        let new_started = Instant::now();
        let mut new_tree = tree;
        for _ in 0..iterations {
            apply_indexed_prop_handler_calls(
                std::hint::black_box(&mut new_tree),
                std::hint::black_box(&calls_by_token),
            );
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "unmatched prop handler calls, {iterations} passes over {node_count} nodes: presized {old_time:?}; unallocated {new_time:?}; ratio {:.2}x",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        println!(
            "MESH_PERF metric=unmatched_prop_handler_call_speedup value={:.6}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_tree.event_handlers, new_tree.event_handlers);
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
                    .insert("click".into(), format!("onChild{index}").into());
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
        props: &AttributeMap,
        prop_handler_calls: &BTreeMap<String, EventHandlerCall>,
    ) {
        if prop_handler_calls.is_empty() {
            return;
        }
        for (event_name, handler) in node.event_handlers.clone() {
            let Some((_, call)) = prop_handler_calls.iter().find(|(prop_name, _)| {
                props.get(prop_name.as_str()).map(String::as_str) == Some(handler.as_str())
            }) else {
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
        props: &AttributeMap,
        prop_handler_calls: &BTreeMap<String, EventHandlerCall>,
    ) {
        let handler_calls = node
            .event_handlers
            .iter()
            .filter_map(|(event_name, handler)| {
                prop_handler_calls
                    .iter()
                    .find(|(prop_name, _)| {
                        props.get(prop_name.as_str()).map(String::as_str) == Some(handler.as_str())
                    })
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
        node.event_handlers
            .insert("pointermove".into(), "move-prop-token".into());
        for child in &mut node.children {
            child
                .event_handlers
                .insert("pointermove".into(), "move-prop-token".into());
        }
        let props = AttributeMap::from([("onMoveProp".into(), "move-prop-token".into())]);
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
            Some("handleMove")
        );
        assert_eq!(
            node.children[0]
                .event_handler_calls
                .get("pointermove")
                .map(|call| call.handler.as_str()),
            Some("handleMove")
        );
        assert_eq!(
            node.event_handlers.get("pointermove"),
            Some(&"handleMove".into())
        );
    }

    #[test]
    fn prop_handler_calls_link_by_prop_when_handler_values_match() {
        let mut node = WidgetNode::new("box");
        node.event_handlers
            .insert("click".into(), "primary-token".into());
        node.event_handlers
            .insert("pointerenter".into(), "secondary-token".into());
        let props = AttributeMap::from([
            ("onPrimary".into(), "primary-token".into()),
            ("onSecondary".into(), "secondary-token".into()),
        ]);
        let shared_handler = HandlerTarget::embedded("parent", "onShared");
        let calls = BTreeMap::from([
            (
                "onPrimary".into(),
                EventHandlerCall {
                    handler: shared_handler.clone(),
                    args: vec![serde_json::json!("primary")],
                },
            ),
            (
                "onSecondary".into(),
                EventHandlerCall {
                    handler: shared_handler.clone(),
                    args: vec![serde_json::json!("secondary")],
                },
            ),
        ]);

        apply_prop_handler_calls(&mut node, &props, &calls);

        assert_eq!(node.event_handlers.get("click"), Some(&shared_handler));
        assert_eq!(
            node.event_handlers.get("pointerenter"),
            Some(&shared_handler)
        );
        assert_eq!(
            node.event_handler_calls.get("click").map(|call| &call.args),
            Some(&vec![serde_json::json!("primary")])
        );
        assert_eq!(
            node.event_handler_calls
                .get("pointerenter")
                .map(|call| &call.args),
            Some(&vec![serde_json::json!("secondary")])
        );
    }

    #[test]
    fn prop_handler_calls_preserve_child_call_args_after_parent_args() {
        let mut node = WidgetNode::new("button");
        node.event_handlers
            .insert("click".into(), "select-prop-token".into());
        node.event_handler_calls.insert(
            "click".into(),
            EventHandlerCall {
                handler: "select-prop-token".into(),
                args: vec![serde_json::json!("item-id")],
            },
        );
        let props = AttributeMap::from([("onSelect".into(), "select-prop-token".into())]);
        let calls = BTreeMap::from([(
            "onSelect".into(),
            EventHandlerCall {
                handler: HandlerTarget::embedded("parent", "onSelect"),
                args: vec![serde_json::json!("parent-context")],
            },
        )]);

        apply_prop_handler_calls(&mut node, &props, &calls);

        let call = node.event_handler_calls.get("click").expect("click call");
        assert_eq!(call.handler, HandlerTarget::embedded("parent", "onSelect"));
        assert_eq!(
            call.args,
            vec![
                serde_json::json!("parent-context"),
                serde_json::json!("item-id")
            ]
        );
    }

    #[test]
    fn runtime_props_json_receives_only_public_typed_props() {
        let props = ComponentCompositionProps {
            values: AttributeMap::from([("label".into(), "Volume".into())]),
            bindings: AttributeMap::from([("hidden".into(), "isHidden".into())]),
            bind_this: Some("child".into()),
        };

        let props_json = runtime_props_json(&props.values);

        assert_eq!(
            props_json.get("label"),
            Some(&serde_json::Value::String("Volume".into()))
        );
        assert_eq!(
            props.bindings.get("hidden").map(String::as_str),
            Some("isHidden")
        );
        assert_eq!(props.bind_this.as_deref(), Some("child"));
    }

    #[test]
    fn runtime_props_json_rejects_structured_props() {
        // Structured bindings retain their JSON type at this boundary, so the
        // scalar prop conversion can reject them instead of accepting their
        // stringified representation as a `string` prop.
        let mut props = AttributeMap::new();
        props.insert_value(
            "items".into(),
            serde_json::json!([
                {"id": "en", "text": "EN"},
                {"id": "sk", "text": "SK"},
            ]),
        );
        props.insert_value("config".into(), serde_json::json!({"enabled": true}));
        props.insert("label".into(), "Volume".into());
        // Text that only looks table-ish remains a scalar string.
        props.insert("weird".into(), "[not json".into());

        let props_json = runtime_props_json(&props);

        assert!(!props_json.contains_key("items"));
        assert!(!props_json.contains_key("config"));
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
        fn old_runtime_props_json(props: &AttributeMap) -> HashMap<String, serde_json::Value> {
            props
                .iter()
                .filter(|(key, _)| {
                    !key.starts_with("__mesh_binding_") && key.as_str() != "__mesh_bind_this"
                })
                .map(|(key, value)| {
                    (
                        key.as_str().to_string(),
                        serde_json::Value::String(value.clone()),
                    )
                })
                .collect()
        }

        let mut legacy_props = AttributeMap::new();
        let mut typed_props = AttributeMap::new();
        for index in 0..64 {
            let key = format!("prop{index}");
            let value = format!("value{index}");
            legacy_props.insert(key.clone().into(), value.clone());
            typed_props.insert(key.into(), value);
            legacy_props.insert(
                format!("__mesh_binding_prop{index}").into(),
                format!("state{index}"),
            );
        }
        legacy_props.insert("__mesh_bind_this".into(), "child".into());
        let iterations = 100_000;

        let old_started = Instant::now();
        let mut old_total = 0usize;
        for _ in 0..iterations {
            old_total += old_runtime_props_json(std::hint::black_box(&legacy_props)).len();
        }
        let old_time = old_started.elapsed();

        let new_started = Instant::now();
        let mut new_total = 0usize;
        for _ in 0..iterations {
            new_total += runtime_props_json(std::hint::black_box(&typed_props)).len();
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
        let props = AttributeMap::from([("onSelected".into(), "missingHandler".into())]);
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
                (
                    mesh_core_elements::AttrKey::new(&format!("onEvent{index}")),
                    handler,
                )
            })
            .collect::<AttributeMap>();
        let calls = (0..16)
            .map(|index| {
                (
                    format!("onEvent{index}"),
                    EventHandlerCall {
                        handler: format!("event{index}").into(),
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
        let props = AttributeMap::from([("onMoveProp".into(), "onMove".into())]);
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
