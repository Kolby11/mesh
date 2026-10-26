use super::types::CoreRequest;
use mesh_core_capability::Capability;
#[cfg(test)]
use mesh_core_capability::CapabilityCatalog;
use mesh_core_frontend_abi::{
    DebugEffect, EffectScope, EffectSource, FrontendEffect, FrontendEffectRevision,
    ScopedFrontendEffect, ServiceEffect,
};
use mesh_core_frontend_shell_adapter::ShellEffectAdapter;
use mesh_core_scripting::{OperationRegistry, OperationRejection, PublishedEvent, ScriptState};
pub(super) use mesh_core_service::service_name_from_interface;
pub(super) use mesh_core_service::service_name_from_interface_cow;
use std::borrow::Borrow;
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, OnceLock, RwLock};

/// Bundle of interned capability values derived from an interface name.
///
/// The shell does a per-service-event capability check on every component
/// runtime. Constructing capability values from formatted strings showed
/// up in profiling as a hot allocation. This struct lets the
/// caller compute them once per interface and pass borrowed refs through
/// the inner loop.
pub(super) struct ServiceCapabilities {
    pub service_name: String,
    pub control: Capability,
}

/// Get (or build) the interned capability bundle for a given interface.
///
/// Returns `Arc<ServiceCapabilities>` so the lock is released before the
/// caller iterates over runtimes. The set of interfaces is bounded and
/// stable in steady state, so the cache does not need eviction.
pub(super) fn service_capabilities(interface: &str) -> Arc<ServiceCapabilities> {
    static CACHE: OnceLock<RwLock<HashMap<String, Arc<ServiceCapabilities>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| RwLock::new(HashMap::new()));

    if let Ok(guard) = cache.read() {
        if let Some(entry) = guard.get(interface) {
            return Arc::clone(entry);
        }
    }

    let service_name = service_name_from_interface(interface);
    let entry = Arc::new(ServiceCapabilities {
        control: Capability::new(format!("service.{service_name}.control")),
        service_name,
    });

    if let Ok(mut guard) = cache.write() {
        guard
            .entry(interface.to_string())
            .or_insert_with(|| Arc::clone(&entry));
    }

    entry
}

/// The host-owned trace of which service last delivered an update.
pub(super) fn service_update_metadata(
    service_name: &str,
    source_module: &str,
) -> serde_json::Value {
    serde_json::json!({ "name": service_name, "source_module": source_module })
}

/// Seed a component's script state with default values before the first
/// service update arrives. This prevents template crashes on first render.
///
/// Prefer [`seed_service_context`] for a live runtime: template expressions are
/// Luau closures over the context's `_ENV`, so a value that reaches only
/// `ScriptState` is invisible to `{last_service_update.name}`.
#[cfg(test)]
pub(super) fn seed_service_state(state: &mut ScriptState) {
    state.set("last_service_update", service_update_metadata("", ""));
}

/// Seed `last_service_update` into both a context's script state and its `_ENV`.
pub(super) fn seed_service_context(
    script_ctx: &mut mesh_core_scripting::ScriptContext,
) -> Result<(), mesh_core_scripting::ScriptError> {
    script_ctx.seed_context_global("last_service_update", service_update_metadata("", ""))
}

/// Apply a service update payload into a component's script state.
///
/// The payload is set directly as `state[service_name]` — no parsing in core.
/// `has_read` must be pre-computed by the caller from the component's capability set
/// (`service.<name>.read`) to avoid simultaneous mutable/immutable borrows.
#[cfg(test)]
pub(super) fn apply_service_update(
    state: &mut ScriptState,
    has_read: bool,
    service: &str,
    source_module: &str,
    payload: impl Borrow<serde_json::Value>,
) {
    let service_name = service_name_from_interface_cow(service);
    apply_service_update_with_name(
        state,
        has_read,
        service_name.as_ref(),
        source_module,
        payload,
    );
}

pub(super) fn apply_service_update_with_name(
    state: &mut ScriptState,
    has_read: bool,
    service_name: &str,
    source_module: &str,
    payload: impl Borrow<serde_json::Value>,
) {
    if has_read {
        state.set(
            "last_service_update",
            service_update_metadata(service_name, source_module),
        );
        state.set(service_name, payload.borrow().clone());
    }
}

/// Returns `true` when the `last_service_update` trace changed, so the caller
/// can mirror it into the context's `_ENV` without paying a JSON-to-Lua
/// conversion on every unchanged fan-out probe.
/// `observed` says whether this runtime has been seen reading the service. A
/// runtime that has evaluated its template without ever touching the service
/// still receives the value — module-wide capability makes the fan-out
/// unavoidable — but the write is installed non-reactively, so it neither
/// dirties the instance nor invalidates its memoized subtree.
pub(super) fn apply_service_update_with_name_and_fingerprint(
    state: &mut ScriptState,
    has_read: bool,
    observed: bool,
    service_name: &str,
    source_module: &str,
    payload: &serde_json::Value,
    fingerprint: u64,
) -> bool {
    if !has_read {
        return false;
    }
    if !observed {
        state.set_unobserved_value_with_fingerprint(service_name, payload, fingerprint);
        return false;
    }
    let metadata_fingerprint = service_update_metadata_fingerprint(service_name, source_module);
    let metadata_changed =
        state.set_with_fingerprint_lazy("last_service_update", metadata_fingerprint, || {
            service_update_metadata(service_name, source_module)
        });
    state.set_with_fingerprint(service_name, payload, fingerprint);
    metadata_changed
}

fn service_update_metadata_fingerprint(service_name: &str, source_module: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    service_name.hash(&mut hasher);
    source_module.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
pub(super) fn script_events_to_requests(events: Vec<PublishedEvent>) -> Vec<CoreRequest> {
    events
        .into_iter()
        .filter_map(script_event_to_request)
        .collect()
}

/// Route script effects through the current catalog/runtime revision. The
/// unversioned helper above remains for standalone compatibility fixtures;
/// live component dispatch uses this boundary.
pub(super) fn script_events_to_requests_at(
    events: Vec<PublishedEvent>,
    revision: FrontendEffectRevision,
) -> Vec<CoreRequest> {
    events
        .into_iter()
        .filter_map(|event| script_event_to_request_at(event, Some(revision)))
        .collect()
}

/// Lower typed `mesh.shell` effects. They were validated and authorized at
/// the Luau call, so no channel matching or payload reading happens here; the
/// ABI adapter still checks the effect against its caller scope.
pub(super) fn typed_effects_to_requests_at(
    scope: EffectScope,
    effects: Vec<FrontendEffect>,
    revision: FrontendEffectRevision,
) -> Vec<CoreRequest> {
    effects
        .into_iter()
        .map(|effect| {
            let effect = ScopedFrontendEffect::new(scope.clone(), effect).with_revision(revision);
            ShellEffectAdapter::lower_at(effect, revision).unwrap_or_else(|error| {
                CoreRequest::PublishDiagnostics {
                    message: error.to_string(),
                }
            })
        })
        .collect()
}

/// Lower a standalone context's typed shell effects the way a live
/// component does, for tests that drive a `ScriptContext` directly.
#[cfg(test)]
pub(crate) fn context_shell_requests(
    ctx: &mut mesh_core_scripting::ScriptContext,
) -> Vec<CoreRequest> {
    let scope = EffectScope::new(
        EffectSource::new(ctx.module_id.clone(), Some(ctx.instance_id.clone())),
        ctx.capabilities.clone(),
    );
    typed_effects_to_requests_at(
        scope,
        ctx.drain_shell_effects(),
        FrontendEffectRevision::new(0, 0),
    )
}

fn lower_frontend_effect(
    event: &PublishedEvent,
    effect: FrontendEffect,
    revision: Option<FrontendEffectRevision>,
) -> CoreRequest {
    let scope = EffectScope::new(
        EffectSource::new(
            event.source_module_id.clone(),
            event.source_instance_id.clone(),
        ),
        event.source_capabilities.clone(),
    );
    let effect = ScopedFrontendEffect::new(scope, effect);
    let effect = if let Some(revision) = revision {
        effect.with_revision(revision)
    } else {
        effect
    };
    let lowered = match revision {
        Some(revision) => ShellEffectAdapter::lower_at(effect, revision),
        None => ShellEffectAdapter::lower_unchecked(effect),
    };
    lowered.unwrap_or_else(|error| CoreRequest::PublishDiagnostics {
        message: error.to_string(),
    })
}

#[cfg(test)]
fn script_event_to_request(event: PublishedEvent) -> Option<CoreRequest> {
    script_event_to_request_at(event, None)
}

fn script_event_to_request_at(
    event: PublishedEvent,
    revision: Option<FrontendEffectRevision>,
) -> Option<CoreRequest> {
    if event.channel.starts_with("shell.")
        && let Err(rejection) = OperationRegistry::builtin().authorize_event(
            &event.channel,
            &event.payload,
            &event.source_module_id,
            &event.source_capabilities,
        )
    {
        tracing::warn!(
            source_module_id = %event.source_module_id,
            channel = %event.channel,
            rejection = %rejection,
            "shell operation rejected"
        );
        return Some(CoreRequest::PublishDiagnostics {
            message: rejection.to_string(),
        });
    }

    match event.channel.as_str() {
        "mesh.service.cancel" => {
            let interface = event
                .payload
                .get("interface")
                .and_then(|value| value.as_str())?;
            let call_id = event.call_id.or_else(|| {
                event
                    .payload
                    .get("call_id")
                    .and_then(|value| value.as_u64())
            })?;
            let capabilities = service_capabilities(interface);
            let required = &capabilities.control;
            if !event.source_capabilities.is_granted(required) {
                return Some(CoreRequest::PublishDiagnostics {
                    message: format!(
                        "Denied service call cancellation for '{}' from '{}' without {}",
                        interface, event.source_module_id, required
                    ),
                });
            }
            Some(lower_frontend_effect(
                &event,
                FrontendEffect::Service(ServiceEffect::Cancel {
                    interface: interface.to_string(),
                    call_id,
                    instance_id: event.source_instance_id.clone()?,
                }),
                revision,
            ))
        }
        "shell.toggle-debug-overlay" => Some(lower_frontend_effect(
            &event,
            FrontendEffect::Debug(DebugEffect::ToggleOverlay),
            revision,
        )),
        "shell.toggle-debug-layout-bounds" => Some(lower_frontend_effect(
            &event,
            FrontendEffect::Debug(DebugEffect::ToggleLayoutBounds),
            revision,
        )),
        "shell.toggle-debug-element-picker" => Some(lower_frontend_effect(
            &event,
            FrontendEffect::Debug(DebugEffect::ToggleElementPicker),
            revision,
        )),
        "shell.open-debug-source" => {
            let path = event.payload.get("path").and_then(|value| value.as_str())?;
            let line = event
                .payload
                .get("line")
                .and_then(|value| value.as_u64())
                .unwrap_or(1)
                .clamp(1, u64::from(u32::MAX)) as u32;
            Some(lower_frontend_effect(
                &event,
                FrontendEffect::Debug(DebugEffect::OpenSource {
                    path: path.to_string(),
                    line,
                }),
                revision,
            ))
        }
        "shell.toggle-debug-profiling" => Some(lower_frontend_effect(
            &event,
            FrontendEffect::Debug(DebugEffect::ToggleProfiling),
            revision,
        )),
        "shell.run-debug-benchmark" => {
            match event.payload.get("scenario_id").and_then(|v| v.as_str()) {
                Some(scenario_id) if !scenario_id.is_empty() => Some(lower_frontend_effect(
                    &event,
                    FrontendEffect::Debug(DebugEffect::RunBenchmark {
                        scenario_id: scenario_id.to_string(),
                    }),
                    revision,
                )),
                _ => Some(CoreRequest::PublishDiagnostics {
                    message: "debug benchmark request missing scenario_id".to_string(),
                }),
            }
        }
        other if other.starts_with("shell.") => {
            let rejection = OperationRejection::Dropped {
                channel: event.channel.clone(),
                reason: "the operation must be handled by its owning component".into(),
            };
            Some(CoreRequest::PublishDiagnostics {
                message: rejection.to_string(),
            })
        }
        other => Some(match other.rfind('.') {
            // Authorization happens at dispatch, against the contract's
            // per-method capabilities; `service.<name>.control` is only the
            // fallback for an interface without a contract.
            Some(pos) => match (event.call_id, event.source_instance_id.clone()) {
                (Some(call_id), Some(source_instance_id)) => lower_frontend_effect(
                    &event,
                    FrontendEffect::Service(ServiceEffect::Call {
                        interface: other[..pos].to_string(),
                        command: other[pos + 1..].to_string(),
                        payload: event.payload.clone(),
                        call_id,
                        instance_id: source_instance_id,
                    }),
                    revision,
                ),
                _ => CoreRequest::PublishDiagnostics {
                    message: OperationRejection::Dropped {
                        channel: event.channel.clone(),
                        reason: "service methods are called through interface proxies".into(),
                    }
                    .to_string(),
                },
            },
            None => CoreRequest::PublishDiagnostics {
                message: OperationRejection::Dropped {
                    channel: event.channel,
                    reason: "channel is not routable".into(),
                }
                .to_string(),
            },
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mesh_core_elements::VariableStore;
    use mesh_core_frontend_abi::{SurfaceEffect, SurfaceRole};

    #[test]
    fn apply_service_update_does_not_leak_metadata_without_read_capability() {
        let mut state = ScriptState::new();
        seed_service_state(&mut state);

        apply_service_update(
            &mut state,
            false,
            "mesh.audio",
            "@mesh/pipewire-audio",
            serde_json::json!({ "percent": 42 }),
        );

        assert_eq!(
            state.get("last_service_update"),
            Some(serde_json::json!({ "name": "", "source_module": "" }))
        );
        assert_eq!(state.get("audio"), None);
    }

    #[test]
    fn script_events_to_requests_rejects_service_methods_without_a_call() {
        let audio_caps = mesh_core_capability::CapabilitySet::from_ids(["service.audio.control"]);
        let requests = script_events_to_requests(vec![PublishedEvent {
            channel: "mesh.audio.set_volume".into(),
            payload: serde_json::json!({ "percent": 55 }),
            source_module_id: "@mesh/quick-settings".into(),
            source_capabilities: audio_caps,
            call_id: None,
            source_instance_id: None,
        }]);

        assert!(matches!(
            requests.as_slice(),
            [CoreRequest::PublishDiagnostics { message }]
                if message.contains("interface proxies")
        ));
    }

    #[test]
    fn script_events_to_requests_preserves_ticket_route_identity() {
        let capabilities = mesh_core_capability::CapabilitySet::from_ids(["service.audio.control"]);
        let requests = script_events_to_requests(vec![PublishedEvent {
            channel: "mesh.audio.set_volume".into(),
            payload: serde_json::json!({ "percent": 55 }),
            source_module_id: "@mesh/audio-popover".into(),
            source_capabilities: capabilities.clone(),
            call_id: Some(1u64 << 63),
            source_instance_id: Some("@mesh/audio-popover#bottom".into()),
        }]);

        assert!(matches!(
            requests.as_slice(),
            [CoreRequest::ServiceCall {
                call_id,
                source_instance_id,
                ..
            }] if *call_id == 1u64 << 63 && source_instance_id == "@mesh/audio-popover#bottom"
        ));

        let cancel_requests = script_events_to_requests(vec![PublishedEvent {
            channel: "mesh.service.cancel".into(),
            payload: serde_json::json!({
                "interface": "mesh.audio",
                "call_id": 1u64 << 63,
            }),
            source_module_id: "@mesh/audio-popover".into(),
            source_capabilities: capabilities,
            call_id: Some(1u64 << 63),
            source_instance_id: Some("@mesh/audio-popover#bottom".into()),
        }]);
        assert!(matches!(
            cancel_requests.as_slice(),
            [CoreRequest::CancelServiceCall {
                call_id,
                source_instance_id,
                ..
            }] if *call_id == 1u64 << 63 && source_instance_id == "@mesh/audio-popover#bottom"
        ));
    }

    /// A proxy call authorized by a contract method capability such as
    /// `service.apps.launch` reaches dispatch, which checks the contract;
    /// lowering must not demand `service.apps.control` first.
    #[test]
    fn proxy_calls_reach_contract_authorization_without_the_control_capability() {
        let requests = script_events_to_requests(vec![PublishedEvent {
            channel: "mesh.apps.activate".into(),
            payload: serde_json::json!({ "id": "kitty" }),
            source_module_id: "@mesh/glass-dock".into(),
            source_capabilities: mesh_core_capability::CapabilitySet::from_ids([
                "service.apps.launch",
            ]),
            call_id: Some(7),
            source_instance_id: Some("@mesh/glass-dock#default".into()),
        }]);

        assert!(
            matches!(
                requests.as_slice(),
                [CoreRequest::ServiceCall { interface, command, .. }]
                    if interface == "mesh.apps" && command == "activate"
            ),
            "{requests:?}"
        );
    }

    fn typed(effects: Vec<FrontendEffect>, module: &str, granted: &[&str]) -> Vec<CoreRequest> {
        let scope = EffectScope::new(
            EffectSource::new(module, Some(format!("{module}#0"))),
            mesh_core_capability::CapabilitySet::from_ids(granted.iter().copied()),
        );
        typed_effects_to_requests_at(scope, effects, FrontendEffectRevision::new(0, 0))
    }

    #[test]
    fn typed_shell_effects_map_widget_role_operations() {
        let requests = typed(
            vec![
                FrontendEffect::Surface(SurfaceEffect::SetChildRole {
                    surface_id: "@mesh/panel".into(),
                    node_key: "root/widgets/media".into(),
                    role: SurfaceRole::Window,
                }),
                FrontendEffect::Surface(SurfaceEffect::SetChildRole {
                    surface_id: "@mesh/panel".into(),
                    node_key: "root/widgets/media".into(),
                    role: SurfaceRole::Layer,
                }),
            ],
            "@mesh/panel",
            &["shell.surface"],
        );

        assert!(matches!(
            &requests[..],
            [
                CoreRequest::SetChildSurfaceRole {
                    surface_id,
                    node_key,
                    role: mesh_core_wayland::SurfaceRole::Window,
                },
                CoreRequest::SetChildSurfaceRole {
                    surface_id: second_surface_id,
                    node_key: second_node_key,
                    role: mesh_core_wayland::SurfaceRole::Layer,
                },
            ] if surface_id == "@mesh/panel"
                && node_key == "root/widgets/media"
                && second_surface_id == surface_id
                && second_node_key == node_key
        ));
    }

    #[test]
    fn typed_shell_effects_still_need_the_caller_capability() {
        let requests = typed(
            vec![FrontendEffect::Surface(SurfaceEffect::Show {
                surface_id: "@mesh/settings".into(),
            })],
            "@mesh/panel",
            &[],
        );
        assert!(
            matches!(&requests[..], [CoreRequest::PublishDiagnostics { .. }]),
            "{requests:?}"
        );
    }

    /// Composition and configuration writes are ordinary service commands, so
    /// a caller is admitted by the capability it holds. Any module granted
    /// `service.packages.control` can select a provider — being `@mesh/settings`
    /// is neither necessary nor sufficient.
    fn event_with(
        channel: &str,
        payload: serde_json::Value,
        module: &str,
        granted: &[&str],
    ) -> PublishedEvent {
        let capabilities = mesh_core_capability::CapabilitySet::from_ids(granted.iter().copied());
        PublishedEvent {
            channel: channel.into(),
            payload,
            source_module_id: module.into(),
            source_capabilities: capabilities,
            call_id: Some(1),
            source_instance_id: Some(format!("{module}#test")),
        }
    }

    #[test]
    fn provider_selection_is_a_capability_gated_service_command() {
        let requests = script_events_to_requests(vec![event_with(
            "mesh.packages.set_provider",
            serde_json::json!({
                "interface": "mesh.audio",
                "provider_id": "@mesh/pulseaudio-audio",
            }),
            "@mesh/some-third-party-settings",
            &["service.packages.control"],
        )]);

        assert!(matches!(
            requests.as_slice(),
            [CoreRequest::ServiceCall { interface, command, .. }]
                if interface == "mesh.packages" && command == "set_provider"
        ));
    }

    #[test]
    fn module_enabled_command_carries_its_payload_through() {
        let requests = script_events_to_requests(vec![event_with(
            "mesh.packages.set_module_enabled",
            serde_json::json!({ "module_id": "@mesh/audio-popover", "enabled": false }),
            "@mesh/settings",
            &["service.packages.control"],
        )]);

        assert!(matches!(
            requests.as_slice(),
            [CoreRequest::ServiceCall { interface, command, payload, .. }]
                if interface == "mesh.packages"
                    && command == "set_module_enabled"
                    && payload == &serde_json::json!({
                        "module_id": "@mesh/audio-popover",
                        "enabled": false,
                    })
        ));
    }

    #[test]
    fn prop_writes_are_settings_service_commands() {
        let requests = script_events_to_requests(vec![
            event_with(
                "mesh.settings.set_prop",
                serde_json::json!({
                    "module_id": "@mesh/navigation-bar",
                    "prop": "blur_enabled",
                    "value": false,
                }),
                "@mesh/settings",
                &["service.settings.control"],
            ),
            event_with(
                "mesh.settings.unset_prop",
                serde_json::json!({
                    "module_id": "@mesh/navigation-bar",
                    "prop": "blur_enabled",
                }),
                "@mesh/settings",
                &["service.settings.control"],
            ),
        ]);

        assert!(matches!(
            requests.as_slice(),
            [
                CoreRequest::ServiceCall { interface, command, .. },
                CoreRequest::ServiceCall { interface: unset_interface, command: unset_command, .. },
            ] if interface == "mesh.settings"
                && command == "set_prop"
                && unset_interface == interface
                && unset_command == "unset_prop"
        ));
    }

    #[test]
    fn appearance_writes_are_theme_service_commands() {
        let requests = script_events_to_requests(vec![
            event_with(
                "mesh.theme.set_icon_theme",
                serde_json::json!({ "theme_id": "Papirus-Dark" }),
                "@mesh/settings",
                &["service.theme.control"],
            ),
            event_with(
                "mesh.theme.set_font_family",
                serde_json::json!({ "family": "Noto Sans" }),
                "@mesh/settings",
                &["service.theme.control"],
            ),
        ]);

        assert!(matches!(
            requests.as_slice(),
            [
                CoreRequest::ServiceCall { command, .. },
                CoreRequest::ServiceCall { command: font_command, .. },
            ] if command == "set_icon_theme" && font_command == "set_font_family"
        ));
    }

    #[test]
    fn prop_writes_preserve_instance_scope() {
        let requests = script_events_to_requests(vec![event_with(
            "mesh.settings.set_prop",
            serde_json::json!({
                "module_id": "@mesh/navigation-bar",
                "instance_id": "@mesh/navigation-bar#bottom",
                "prop": "blur_enabled",
                "value": false,
            }),
            "@mesh/settings",
            &["service.settings.control"],
        )]);

        assert!(matches!(
            requests.as_slice(),
            [CoreRequest::ServiceCall { payload, .. }]
                if payload.get("instance_id")
                    == Some(&serde_json::json!("@mesh/navigation-bar#bottom"))
        ));
    }

    #[test]
    fn typed_shell_effects_map_popover_focus_option() {
        let requests = typed(
            vec![FrontendEffect::Surface(SurfaceEffect::ActivatePopover {
                surface_id: "@mesh/audio-popover".into(),
                trigger_surface: "@mesh/navigation-bar".into(),
                trigger_key: "volume-button".into(),
                focus: false,
            })],
            "@mesh/navigation-bar",
            &["shell.surface"],
        );

        match requests.as_slice() {
            [
                CoreRequest::ActivatePopover {
                    surface_id,
                    trigger_surface,
                    trigger_key,
                    focus,
                },
            ] => {
                assert_eq!(surface_id, "@mesh/audio-popover");
                assert_eq!(trigger_surface, "@mesh/navigation-bar");
                assert_eq!(trigger_key, "volume-button");
                assert!(!focus);
            }
            other => panic!("expected ActivatePopover request, got {other:?}"),
        }
    }

    #[test]
    fn typed_shell_effects_map_popover_hover_bridge_hide() {
        let requests = typed(
            vec![FrontendEffect::Surface(SurfaceEffect::HidePopover {
                surface_id: "@mesh/quick-settings".into(),
                defer_for_hover_bridge: true,
            })],
            "@mesh/quick-settings",
            &["shell.surface"],
        );

        match requests.as_slice() {
            [
                CoreRequest::HidePopover {
                    surface_id,
                    defer_for_hover_bridge,
                },
            ] => {
                assert_eq!(surface_id, "@mesh/quick-settings");
                assert!(*defer_for_hover_bridge);
            }
            other => panic!("expected HidePopover request, got {other:?}"),
        }
    }

    #[test]
    fn script_events_to_requests_rejects_unknown_shell_channels() {
        let caps = mesh_core_capability::CapabilitySet::from_ids(["service.shell.control"]);
        let requests = script_events_to_requests(vec![PublishedEvent {
            channel: "shell.brightness-down".into(),
            payload: serde_json::json!({ "step": 10 }),
            source_module_id: "@mesh/navigation-bar".into(),
            source_capabilities: caps,
            call_id: None,
            source_instance_id: None,
        }]);

        match requests.as_slice() {
            [CoreRequest::PublishDiagnostics { message }] => {
                assert!(message.contains("Unknown shell channel"));
                assert!(message.contains("shell.brightness-down"));
            }
            other => {
                panic!("unknown shell.* channels must never become service commands, got {other:?}")
            }
        }
    }

    #[test]
    fn script_events_to_requests_maps_debug_control_events() {
        let debug_capabilities = CapabilityCatalog::builtin()
            .capability_set(["service.debug.control"])
            .expect("debug control must be cataloged");
        let requests = script_events_to_requests(vec![
            PublishedEvent {
                channel: "shell.toggle-debug-overlay".into(),
                payload: serde_json::json!({}),
                source_module_id: "@mesh/debug-inspector".into(),
                source_capabilities: debug_capabilities.clone(),
                call_id: None,
                source_instance_id: None,
            },
            PublishedEvent {
                channel: "shell.toggle-debug-layout-bounds".into(),
                payload: serde_json::json!({}),
                source_module_id: "@mesh/debug-inspector".into(),
                source_capabilities: debug_capabilities.clone(),
                call_id: None,
                source_instance_id: None,
            },
            PublishedEvent {
                channel: "shell.toggle-debug-element-picker".into(),
                payload: serde_json::json!({}),
                source_module_id: "@mesh/debug-inspector".into(),
                source_capabilities: debug_capabilities.clone(),
                call_id: None,
                source_instance_id: None,
            },
            PublishedEvent {
                channel: "shell.open-debug-source".into(),
                payload: serde_json::json!({ "path": "/tmp/example.mesh", "line": 42 }),
                source_module_id: "@mesh/debug-inspector".into(),
                source_capabilities: debug_capabilities.clone(),
                call_id: None,
                source_instance_id: None,
            },
            PublishedEvent {
                channel: "shell.toggle-debug-profiling".into(),
                payload: serde_json::json!({}),
                source_module_id: "@mesh/debug-inspector".into(),
                source_capabilities: debug_capabilities,
                call_id: None,
                source_instance_id: None,
            },
        ]);

        assert!(matches!(
            requests.first(),
            Some(CoreRequest::ToggleDebugOverlay)
        ));
        assert!(matches!(
            requests.get(1),
            Some(CoreRequest::ToggleDebugLayoutBounds)
        ));
        assert!(matches!(
            requests.get(2),
            Some(CoreRequest::ToggleDebugElementPicker)
        ));
        assert!(matches!(
            requests.get(3),
            Some(CoreRequest::OpenDebugSource { path, line })
                if path == "/tmp/example.mesh" && *line == 42
        ));
        assert!(matches!(
            requests.get(4),
            Some(CoreRequest::ToggleDebugProfiling)
        ));
    }

    #[test]
    fn script_events_to_requests_rejects_debug_read_access_for_control() {
        let requests = script_events_to_requests(vec![PublishedEvent {
            channel: "shell.toggle-debug-overlay".into(),
            payload: serde_json::json!({}),
            source_module_id: "@mesh/debug-inspector".into(),
            source_capabilities: CapabilityCatalog::builtin()
                .capability_set(["service.debug.read"])
                .expect("debug read must be cataloged"),
            call_id: None,
            source_instance_id: None,
        }]);

        assert!(matches!(
            requests.as_slice(),
            [CoreRequest::PublishDiagnostics { message }]
                if message.contains("service.debug.control")
        ));
    }

    #[test]
    fn script_events_to_requests_rejects_removed_surface_channels() {
        let requests = script_events_to_requests(vec![event_with(
            "shell.position-surface",
            serde_json::json!({
                "surface_id": "@mesh/popover",
                "margin_top": 4,
                "margin_left": 24,
            }),
            "@mesh/navigation-bar",
            &["shell.surface"],
        )]);

        match requests.as_slice() {
            [CoreRequest::PublishDiagnostics { message }] => {
                assert!(message.contains("mesh.shell.position_surface"), "{message}");
            }
            other => panic!("removed channels must not lower, got {other:?}"),
        }
    }

    #[test]
    fn service_capabilities_include_control_capability() {
        let caps = service_capabilities("mesh.audio");

        assert_eq!(caps.service_name, "audio");
        assert_eq!(caps.control.id(), "service.audio.control");
    }

    #[test]
    fn fingerprinted_service_metadata_preserves_update_shape() {
        let payload = serde_json::json!({ "available": true });
        let fingerprint = mesh_core_scripting::ScriptContext::service_payload_fingerprint(&payload);
        let mut state = ScriptState::new();

        apply_service_update_with_name_and_fingerprint(
            &mut state,
            true,
            true,
            "audio",
            "@mesh/pipewire",
            &payload,
            fingerprint,
        );

        assert_eq!(state.get("audio"), Some(payload));
        assert_eq!(
            state.get("last_service_update"),
            Some(serde_json::json!({
                "name": "audio",
                "source_module": "@mesh/pipewire"
            }))
        );
    }

    // cargo test -p mesh-core-shell --release -- typed_shell_call_beats_publish_path --ignored --nocapture
    #[test]
    #[ignore = "release-only typed shell call vs publish path microbenchmark"]
    fn typed_shell_call_beats_publish_path() {
        // The publish side uses a debug channel only because every surface
        // channel is now removed; both run a handler with one shell request,
        // drain it and lower it to a `CoreRequest`.
        let capabilities = mesh_core_capability::CapabilitySet::from_ids([
            "shell.surface",
            "service.debug.control",
        ]);
        let mut ctx =
            mesh_core_scripting::ScriptContext::new("@mesh/debug-inspector", capabilities).unwrap();
        ctx.load_script(
            r#"
function published()
    mesh.events.publish("shell.toggle-debug-overlay", {})
end
function typed()
    mesh.shell.toggle_surface("@mesh/settings")
end
"#,
        )
        .unwrap();
        let iterations = 20_000;
        for _ in 0..1_000 {
            ctx.call_handler("published", &[]).unwrap();
            script_events_to_requests(ctx.drain_published_events());
            ctx.call_handler("typed", &[]).unwrap();
            context_shell_requests(&mut ctx);
        }

        let started = std::time::Instant::now();
        let mut published_requests = 0;
        for _ in 0..iterations {
            ctx.call_handler("published", &[]).unwrap();
            published_requests += script_events_to_requests(ctx.drain_published_events()).len();
        }
        let published_time = started.elapsed();

        let started = std::time::Instant::now();
        let mut typed_requests = 0;
        for _ in 0..iterations {
            ctx.call_handler("typed", &[]).unwrap();
            typed_requests += context_shell_requests(&mut ctx).len();
        }
        let typed_time = started.elapsed();

        eprintln!(
            "shell call x{iterations}: publish path {published_time:?}; typed call {typed_time:?}; ratio {:.2}x",
            published_time.as_secs_f64() / typed_time.as_secs_f64()
        );
        assert_eq!(published_requests, iterations);
        assert_eq!(typed_requests, iterations);
        assert!(typed_time < published_time);
    }
}
