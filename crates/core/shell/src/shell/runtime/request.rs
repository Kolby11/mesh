use super::super::*;
use super::service_state;
use crate::shell::types::{
    DurableProfileWriteResult, PopoverSurfaceRelationship, PopoverTriggerReference,
    ProfileWriteOperation, TabFocusTarget,
};
use mesh_core_debug::{
    BenchmarkScenarioId, BenchmarkScenarioStatus, DebugBenchmarkRunState, ProfilingBackendStage,
};
use mesh_core_presentation::{
    ContentExtent, LayerSurfaceSizePolicy, LayerWireSize, PopupAnchor, PopupConfig,
    PopupConstraint, PopupGravity, PopupPlacement, SurfaceConfig,
    SurfaceExtent as ConfiguredSurfaceExtent, SurfacePadding,
};
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};

/// Coalescable commands fire on the leading edge; further calls within the
/// interval park as `pending` and are flushed on the trailing edge. This is
/// intentionally longer than one render tick because shipped providers use
/// these setters for external commands such as `wpctl` and `brightnessctl`.
/// Their optimistic state bindings keep slider visuals smooth while the
/// backend command rate stays below the process-launch cost.
pub(in crate::shell) const COMMAND_THROTTLE_INTERVAL: std::time::Duration =
    std::time::Duration::from_millis(100);
const SERVICE_CALL_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(100);
const MAX_COALESCED_COMMAND_KEYS: usize = 256;
const POPOVER_HOVER_BRIDGE_DELAY: std::time::Duration = std::time::Duration::from_millis(180);
const DEBUG_INSPECTOR_SURFACE_ID: &str = "@mesh/debug-inspector";
const MAX_EFFECTS_PER_FRAME: usize = 512;
const MAX_EFFECT_BYTES_PER_FRAME: usize = 512 * 1024;
const MAX_EFFECTS_PER_SOURCE_PER_FRAME: usize = 64;
const MAX_EFFECT_BYTES_PER_SOURCE_PER_FRAME: usize = 64 * 1024;
const MAX_EFFECTS_PER_TRANSACTION: usize = 256;
const MAX_REPEATED_CAUSAL_EFFECTS: usize = 4;
const MAX_SOURCE_BUDGET_VIOLATIONS: u32 = 3;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(in crate::shell) struct EffectSource {
    module_id: String,
    runtime_id: String,
    generation: u64,
}

#[derive(Debug, Clone)]
struct EffectContext {
    source: EffectSource,
    transaction_id: u64,
    depth: u16,
}

#[derive(Debug, Clone)]
struct ScheduledEffect {
    request: CoreRequest,
    context: EffectContext,
    fingerprint: u64,
}

#[derive(Debug, Default)]
struct EffectFrameBudget {
    processed: usize,
    bytes: usize,
    source_counts: HashMap<EffectSource, usize>,
    source_bytes: HashMap<EffectSource, usize>,
}

#[derive(Debug, Default)]
pub(in crate::shell) struct EffectSchedulerReport {
    deferred: usize,
    dropped: usize,
    cycle_breaks: usize,
    transaction_budget_exceeded: usize,
    source_budget_exceeded: usize,
    quarantined_sources: Vec<EffectSource>,
}

#[derive(Debug)]
pub(in crate::shell) struct EffectScheduler {
    queues: HashMap<EffectSource, VecDeque<ScheduledEffect>>,
    ready_sources: VecDeque<EffectSource>,
    deferred: VecDeque<ScheduledEffect>,
    next_transaction_id: u64,
    transaction_counts: HashMap<u64, usize>,
    causal_counts: HashMap<(u64, EffectSource, u64), usize>,
    source_budget_violations: HashMap<EffectSource, u32>,
    quarantined_sources: HashSet<EffectSource>,
    blocked_causal_chains: HashSet<(u64, EffectSource)>,
    active_context: Option<EffectContext>,
    frame: Option<EffectFrameBudget>,
    report: EffectSchedulerReport,
}

impl Default for EffectScheduler {
    fn default() -> Self {
        Self {
            queues: HashMap::new(),
            ready_sources: VecDeque::new(),
            deferred: VecDeque::new(),
            next_transaction_id: 1,
            transaction_counts: HashMap::new(),
            causal_counts: HashMap::new(),
            source_budget_violations: HashMap::new(),
            quarantined_sources: HashSet::new(),
            blocked_causal_chains: HashSet::new(),
            active_context: None,
            frame: None,
            report: EffectSchedulerReport::default(),
        }
    }
}

impl EffectScheduler {
    fn next_transaction_id(&mut self) -> u64 {
        let transaction_id = self.next_transaction_id;
        self.next_transaction_id = self.next_transaction_id.wrapping_add(1).max(1);
        transaction_id
    }

    fn enqueue_batch(&mut self, effects: impl IntoIterator<Item = (CoreRequest, EffectSource)>) {
        let transaction_id = self.next_transaction_id();
        for (request, source) in effects {
            self.enqueue_with_context(ScheduledEffect {
                fingerprint: effect_fingerprint(&request),
                request,
                context: EffectContext {
                    source,
                    transaction_id,
                    depth: 0,
                },
            });
        }
    }

    fn enqueue_followups(&mut self, requests: impl IntoIterator<Item = CoreRequest>) {
        let Some(context) = self.active_context.clone() else {
            return;
        };
        let mut followup_context = context;
        followup_context.depth = followup_context.depth.saturating_add(1);
        for request in requests {
            self.enqueue_with_context(ScheduledEffect {
                fingerprint: effect_fingerprint(&request),
                request,
                context: followup_context.clone(),
            });
        }
    }

    fn enqueue_with_context(&mut self, effect: ScheduledEffect) {
        let source = effect.context.source.clone();
        if self.quarantined_sources.contains(&source)
            || self
                .blocked_causal_chains
                .contains(&(effect.context.transaction_id, source.clone()))
        {
            self.report.dropped = self.report.dropped.saturating_add(1);
            return;
        }
        let queue = self.queues.entry(source.clone()).or_default();
        let was_empty = queue.is_empty();
        queue.push_back(effect);
        if was_empty {
            self.ready_sources.push_back(source);
        }
    }

    fn begin_frame(&mut self) {
        self.frame = Some(EffectFrameBudget::default());
        self.report = EffectSchedulerReport::default();
        let deferred = std::mem::take(&mut self.deferred);
        for effect in deferred {
            self.enqueue_with_context(effect);
        }
    }

    fn next_effect(&mut self) -> Option<ScheduledEffect> {
        loop {
            let source = self.ready_sources.pop_front()?;
            let Some(mut queue) = self.queues.remove(&source) else {
                continue;
            };
            let Some(effect) = queue.pop_front() else {
                continue;
            };
            if !queue.is_empty() {
                self.ready_sources.push_back(source.clone());
                self.queues.insert(source.clone(), queue);
            }

            let weight = effect_weight(&effect.request);
            let frame = self.frame.as_mut().expect("scheduler frame must be active");
            if frame.processed >= MAX_EFFECTS_PER_FRAME {
                self.deferred.push_back(effect);
                self.defer_ready_queues();
                self.report.deferred = self.deferred.len();
                return None;
            }
            if weight > MAX_EFFECT_BYTES_PER_FRAME {
                self.drop_causal_chain(&effect.context);
                self.report.dropped = self.report.dropped.saturating_add(1);
                continue;
            }
            if frame.bytes.saturating_add(weight) > MAX_EFFECT_BYTES_PER_FRAME {
                self.deferred.push_back(effect);
                self.defer_ready_queues();
                self.report.deferred = self.deferred.len();
                return None;
            }

            let source_count = frame
                .source_counts
                .get(&source)
                .copied()
                .unwrap_or_default();
            let source_bytes = frame.source_bytes.get(&source).copied().unwrap_or_default();
            if source_count >= MAX_EFFECTS_PER_SOURCE_PER_FRAME
                || source_bytes.saturating_add(weight) > MAX_EFFECT_BYTES_PER_SOURCE_PER_FRAME
            {
                self.deferred.push_back(effect);
                self.defer_source(&source);
                self.report.source_budget_exceeded =
                    self.report.source_budget_exceeded.saturating_add(1);
                let violations = self
                    .source_budget_violations
                    .entry(source.clone())
                    .or_default();
                *violations = violations.saturating_add(1);
                if *violations >= MAX_SOURCE_BUDGET_VIOLATIONS
                    && self.quarantined_sources.insert(source.clone())
                {
                    self.report.quarantined_sources.push(source.clone());
                    self.drop_source(&source);
                }
                self.report.deferred = self.deferred.len();
                continue;
            }

            let transaction_count = self
                .transaction_counts
                .entry(effect.context.transaction_id)
                .or_default();
            if *transaction_count >= MAX_EFFECTS_PER_TRANSACTION {
                self.drop_causal_chain(&effect.context);
                self.report.transaction_budget_exceeded =
                    self.report.transaction_budget_exceeded.saturating_add(1);
                self.report.dropped = self.report.dropped.saturating_add(1);
                continue;
            }

            if effect.context.depth > 0 {
                let causal_key = (
                    effect.context.transaction_id,
                    effect.context.source.clone(),
                    effect.fingerprint,
                );
                let causal_count = self.causal_counts.entry(causal_key).or_default();
                *causal_count = causal_count.saturating_add(1);
                if *causal_count > MAX_REPEATED_CAUSAL_EFFECTS {
                    self.drop_causal_chain(&effect.context);
                    self.blocked_causal_chains
                        .insert((effect.context.transaction_id, effect.context.source.clone()));
                    self.report.cycle_breaks = self.report.cycle_breaks.saturating_add(1);
                    self.report.dropped = self.report.dropped.saturating_add(1);
                    continue;
                }
            }

            *frame.source_counts.entry(source.clone()).or_default() += 1;
            *frame.source_bytes.entry(source).or_default() += weight;
            *transaction_count += 1;
            frame.processed += 1;
            frame.bytes = frame.bytes.saturating_add(weight);
            return Some(effect);
        }
    }

    fn finish_frame(&mut self) -> EffectSchedulerReport {
        self.report.deferred = self.deferred.len();
        let live_transactions = self
            .queues
            .values()
            .chain(std::iter::once(&self.deferred))
            .flat_map(|queue| queue.iter().map(|effect| effect.context.transaction_id))
            .collect::<HashSet<_>>();
        self.transaction_counts
            .retain(|transaction_id, _| live_transactions.contains(transaction_id));
        self.causal_counts
            .retain(|(transaction_id, _, _), _| live_transactions.contains(transaction_id));
        self.frame = None;
        std::mem::take(&mut self.report)
    }

    fn active_context(&self) -> Option<EffectContext> {
        self.active_context.clone()
    }

    fn set_active_context(&mut self, context: EffectContext) {
        self.active_context = Some(context);
    }

    fn clear_active_context(&mut self) {
        self.active_context = None;
    }

    fn defer_ready_queues(&mut self) {
        while let Some(source) = self.ready_sources.pop_front() {
            if let Some(queue) = self.queues.remove(&source) {
                self.deferred.extend(queue);
            }
        }
    }

    fn defer_source(&mut self, source: &EffectSource) {
        self.ready_sources.retain(|queued| queued != source);
        if let Some(queue) = self.queues.remove(source) {
            self.deferred.extend(queue);
        }
    }

    fn drop_source(&mut self, source: &EffectSource) {
        self.ready_sources.retain(|queued| queued != source);
        self.queues.remove(source);
        self.deferred
            .retain(|effect| &effect.context.source != source);
    }

    fn drop_causal_chain(&mut self, context: &EffectContext) {
        self.ready_sources
            .retain(|source| source != &context.source);
        if let Some(queue) = self.queues.remove(&context.source) {
            self.report.dropped = self.report.dropped.saturating_add(
                queue
                    .iter()
                    .filter(|effect| effect.context.transaction_id == context.transaction_id)
                    .count(),
            );
            self.deferred.extend(
                queue
                    .into_iter()
                    .filter(|effect| effect.context.transaction_id != context.transaction_id),
            );
            if self
                .deferred
                .iter()
                .any(|effect| effect.context.source == context.source)
            {
                self.ready_sources.push_back(context.source.clone());
            }
        }
        self.deferred.retain(|effect| {
            effect.context.transaction_id != context.transaction_id
                || effect.context.source != context.source
        });
    }

    fn pending_len(&self) -> usize {
        self.queues.values().map(VecDeque::len).sum::<usize>() + self.deferred.len()
    }

    fn discard_pending(&mut self) -> usize {
        let count = self.pending_len();
        self.queues.clear();
        self.ready_sources.clear();
        self.deferred.clear();
        count
    }
}

fn effect_fingerprint(request: &CoreRequest) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    format!("{request:?}").hash(&mut hasher);
    hasher.finish()
}

fn effect_weight(request: &CoreRequest) -> usize {
    64usize.saturating_add(format!("{request:?}").len())
}

/// Canonical failure payload when a service command cannot be delivered (no
/// backend channel, send failure, or unregistered interface).
fn service_unavailable_response() -> serde_json::Value {
    serde_json::json!({
        "ok": false,
        "error": "service_unavailable",
        "status": "service_unavailable",
    })
}

fn service_queue_full_response() -> serde_json::Value {
    serde_json::json!({
        "ok": false,
        "error": "command_queue_full",
        "status": "queue_full",
    })
}

fn command_coalescing_key(command: &str, payload: &serde_json::Value) -> String {
    let identity = payload.as_object().map(|object| {
        object
            .iter()
            .filter(|(name, _)| {
                *name == "id"
                    || name.ends_with("_id")
                    || name.contains("target")
                    || name.contains("device")
                    || name.contains("player")
                    || name.contains("output")
                    || name.contains("sink")
            })
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect::<std::collections::BTreeMap<_, _>>()
    });
    format!(
        "{}:{}",
        command,
        serde_json::to_string(&identity.unwrap_or_default()).unwrap_or_default()
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ServiceCommandSendError {
    Closed,
    Full,
}

impl Shell {
    fn effect_source_for_surface(&mut self, surface_id: &str) -> (String, String) {
        self.component_index_for_surface(surface_id)
            .map(|index| {
                (
                    self.components[index].component.id().to_string(),
                    surface_id.to_string(),
                )
            })
            .unwrap_or_else(|| ("@mesh/shell".to_string(), surface_id.to_string()))
    }

    fn effect_source_for_request(&mut self, request: &CoreRequest) -> EffectSource {
        let (module_id, runtime_id) = match request {
            CoreRequest::ToggleSurface { surface_id }
            | CoreRequest::ShowSurface { surface_id }
            | CoreRequest::HideSurface { surface_id }
            | CoreRequest::HidePopover { surface_id, .. }
            | CoreRequest::SetSurfaceRole { surface_id, .. }
            | CoreRequest::ToggleSurfaceRole { surface_id }
            | CoreRequest::SetChildSurfaceRole { surface_id, .. }
            | CoreRequest::PositionSurface { surface_id, .. }
            | CoreRequest::ActivatePopover { surface_id, .. } => {
                self.effect_source_for_surface(surface_id)
            }
            CoreRequest::TransferTabFocus { from_surface, .. } => {
                self.effect_source_for_surface(from_surface)
            }
            CoreRequest::ServiceCommand {
                source_module_id, ..
            } => (source_module_id.clone(), source_module_id.clone()),
            CoreRequest::ServiceCall {
                source_module_id,
                source_instance_id,
                ..
            }
            | CoreRequest::CancelServiceCall {
                source_module_id,
                source_instance_id,
                ..
            } => (source_module_id.clone(), source_instance_id.clone()),
            _ => ("@mesh/shell".to_string(), "shell".to_string()),
        };
        EffectSource {
            module_id,
            runtime_id,
            generation: self.activation_generation,
        }
    }

    pub(in crate::shell) fn enqueue_effects(
        &mut self,
        requests: impl IntoIterator<Item = CoreRequest>,
    ) {
        let requests = requests.into_iter().collect::<Vec<_>>();
        if requests.is_empty() {
            return;
        }
        if self.effect_scheduler.active_context().is_some() {
            self.effect_scheduler.enqueue_followups(requests);
            return;
        }
        let sourced = requests
            .into_iter()
            .map(|request| {
                let source = self.effect_source_for_request(&request);
                (request, source)
            })
            .collect::<Vec<_>>();
        self.effect_scheduler.enqueue_batch(sourced);
    }

    pub(in crate::shell) fn process_effects(
        &mut self,
    ) -> Result<EffectSchedulerReport, ShellRunError> {
        self.effect_scheduler.begin_frame();
        while let Some(effect) = self.effect_scheduler.next_effect() {
            let context = effect.context.clone();
            self.effect_scheduler.set_active_context(context);
            let result = self.apply_request(effect.request);
            match result {
                Ok(followups) => {
                    self.effect_scheduler.enqueue_followups(followups);
                    self.effect_scheduler.clear_active_context();
                }
                Err(error) => {
                    self.effect_scheduler.clear_active_context();
                    let report = self.effect_scheduler.finish_frame();
                    self.report_effect_scheduler(&report);
                    return Err(error);
                }
            }
        }
        let report = self.effect_scheduler.finish_frame();
        self.report_effect_scheduler(&report);
        Ok(report)
    }

    fn report_effect_scheduler(&mut self, report: &EffectSchedulerReport) {
        if report.deferred > 0 {
            tracing::debug!(count = report.deferred, "deferred residual shell effects");
        }
        if report.cycle_breaks > 0 {
            self.diagnostics.record_lifecycle_error(
                "@mesh/shell",
                "effect_scheduler_cycle_detected",
                format!(
                    "stopped {} repeated causal shell effect cycle(s)",
                    report.cycle_breaks
                ),
            );
        }
        if report.transaction_budget_exceeded > 0 {
            self.diagnostics.record_lifecycle_error(
                "@mesh/shell",
                "effect_scheduler_transaction_budget_exceeded",
                format!(
                    "dropped {} shell effect causal chain(s) after exceeding the transaction budget",
                    report.transaction_budget_exceeded
                ),
            );
        }
        if report.source_budget_exceeded > 0 {
            self.diagnostics.record_lifecycle_error(
                "@mesh/shell",
                "effect_scheduler_source_budget_exceeded",
                format!(
                    "deferred shell effects from {} source(s) after exceeding the per-frame budget",
                    report.source_budget_exceeded
                ),
            );
        }
        for source in &report.quarantined_sources {
            let message = format!(
                "quarantined effect producer '{}' ({}) after repeated scheduler budget violations",
                source.module_id, source.runtime_id
            );
            self.diagnostics.record_lifecycle_error(
                source.module_id.clone(),
                "effect_scheduler_quarantined",
                message.clone(),
            );
            if let Some(index) = self.components.iter().position(|runtime| {
                runtime.component.id() == source.module_id
                    || runtime.surface_id == source.runtime_id
            }) {
                while !self.components[index].quarantined {
                    self.components[index].note_failure();
                }
                self.components[index]
                    .component
                    .isolate_runtime_failure("effect_scheduler", &message);
            }
        }
    }

    pub(in crate::shell) fn discard_scheduled_effects(&mut self) -> usize {
        self.effect_scheduler.discard_pending()
    }

    pub(in crate::shell) fn complete_profile_write(
        &mut self,
        operation: ProfileWriteOperation,
        result: Result<DurableProfileWriteResult, String>,
    ) -> VecDeque<CoreRequest> {
        let result = match result {
            Ok(result) => result,
            Err(error) => {
                if let ProfileWriteOperation::BackendProviderSelection {
                    interface,
                    provider_id,
                } = &operation
                {
                    self.fail_backend_runtime_switch(
                        interface,
                        provider_id,
                        format!(
                            "provider {provider_id} became ready for {interface}, but its selection could not be saved: {error}"
                        ),
                    );
                }
                self.diagnostics.record_lifecycle_error(
                    "@mesh/settings",
                    "profile_write_rejected",
                    error,
                );
                return VecDeque::new();
            }
        };
        match operation {
            ProfileWriteOperation::NodeSlot {
                profile_id,
                apply_switch,
            } => {
                if apply_switch {
                    self.apply_switch_profile(&profile_id)
                } else {
                    VecDeque::new()
                }
            }
            ProfileWriteOperation::BackendProviderSelection {
                interface,
                provider_id,
            } => {
                if !matches!(&result, DurableProfileWriteResult::Complete) {
                    self.fail_backend_runtime_switch(
                        &interface,
                        &provider_id,
                        format!(
                            "provider {provider_id} became ready for {interface}, but its selection worker returned an invalid result"
                        ),
                    );
                    return VecDeque::new();
                }
                self.complete_backend_runtime_switch_after_persistence(&interface, &provider_id);
                VecDeque::new()
            }
            ProfileWriteOperation::ProviderSelection {
                graph_path: _,
                interface,
                provider_id,
            } => {
                if !matches!(result, DurableProfileWriteResult::Complete) {
                    self.diagnostics.record_lifecycle_error(
                        "@mesh/settings",
                        "provider_selection_write_invalid_result",
                        format!(
                            "provider selection for {interface} returned an unexpected rollback"
                        ),
                    );
                    return VecDeque::new();
                }
                let candidate = match self.load_installed_module_graph_candidate() {
                    Ok(candidate) => candidate,
                    Err(error) => {
                        let message = format!(
                            "provider selection for {interface} was saved but the candidate graph could not be loaded: {error}"
                        );
                        self.diagnostics.record_lifecycle_error(
                            "@mesh/settings",
                            "provider_selection_graph_reload_failed",
                            message,
                        );
                        return VecDeque::new();
                    }
                };
                if let Err(error) = self.commit_installed_module_graph(candidate) {
                    let message = format!(
                        "provider selection for {interface} was saved but locale catalogs could not be committed: {error}"
                    );
                    self.diagnostics.record_lifecycle_error(
                        "@mesh/settings",
                        "provider_selection_locale_reload_failed",
                        message,
                    );
                }
                tracing::info!(
                    interface,
                    provider_id,
                    "saved selection for the already-running provider"
                );
                VecDeque::new()
            }
            ProfileWriteOperation::ModuleEnabled {
                graph_path: _,
                module_id,
                enabled: _,
                active_profile_id,
            } => {
                let DurableProfileWriteResult::Rollback(rollback) = result else {
                    self.diagnostics.record_lifecycle_error(
                        "@mesh/settings",
                        "module_enabled_write_invalid_result",
                        format!("enabled state write for {module_id} returned no rollback"),
                    );
                    return VecDeque::new();
                };
                if let Some(profile_id) = active_profile_id {
                    return self.begin_profile_activation(&profile_id, Some(rollback));
                }
                let graph = match self.load_installed_module_graph_candidate() {
                    Ok(graph) => graph,
                    Err(error) => {
                        let restore_error = rollback.restore().err();
                        let message = format!(
                            "module {module_id} update produced an unloadable graph: {error}{}",
                            restore_error
                                .map(|error| format!("; rollback also failed: {error}"))
                                .unwrap_or_default()
                        );
                        self.diagnostics.record_lifecycle_error(
                            "@mesh/settings",
                            "module_enabled_graph_reload_failed",
                            message,
                        );
                        return VecDeque::new();
                    }
                };
                self.begin_legacy_graph_activation(graph, Some(rollback))
            }
        }
    }

    fn apply_set_module_enabled(
        &mut self,
        module_id: &str,
        enabled: bool,
    ) -> VecDeque<CoreRequest> {
        if self.composition_mode.is_recovery() {
            let message = format!(
                "cannot change module {module_id} while the shell is in configured composition recovery"
            );
            tracing::warn!(module_id, enabled, "{message}");
            self.diagnostics.record_lifecycle_error(
                "@mesh/settings",
                "configured_composition_recovery",
                message,
            );
            return VecDeque::new();
        }
        if self.profile_transition_pending() {
            tracing::warn!(
                module_id,
                enabled,
                "module change rejected during profile switch"
            );
            return VecDeque::new();
        }
        let graph_path = self.installed_module_graph_path();
        let active_profile_id =
            mesh_core_module::package::ProfilePaths::from_root_graph(&graph_path)
                .and_then(|paths| paths.active_profile_id())
                .ok()
                .flatten();
        let uses_profile = active_profile_id.is_some();
        if !enabled
            && self
                .pending_backend_runtimes
                .values()
                .any(|pending| pending.slot.provider_id == module_id)
        {
            let message = format!(
                "cannot disable {module_id} while it is being prepared as an active provider"
            );
            tracing::warn!(module_id, enabled, "{message}");
            self.diagnostics.record_lifecycle_error(
                "@mesh/settings".to_string(),
                "invalid_module_enabled_selection",
                message,
            );
            return VecDeque::new();
        }
        let rejection = match self.load_installed_module_graph_cached() {
            Err(error) => Some(format!(
                "cannot update module {module_id}: failed to load {}: {error}",
                graph_path.display()
            )),
            Ok(graph) if graph.module(module_id).is_none() => {
                Some(format!("cannot update unknown module {module_id}"))
            }
            Ok(_) if !enabled && module_id == "@mesh/settings" => {
                Some("cannot disable @mesh/settings from its own settings surface".into())
            }
            Ok(graph)
                if !uses_profile
                    && !enabled
                    && graph
                        .layout_entrypoint()
                        .is_some_and(|layout| layout.module_id == module_id) =>
            {
                Some(format!(
                    "cannot disable {module_id} while it is the active root layout"
                ))
            }
            Ok(graph) if !enabled => graph
                .backend_provider_contributions()
                .into_iter()
                .filter(|provider| provider.module_id == module_id)
                .find_map(|provider| {
                    graph
                        .active_provider(&provider.interface)
                        .filter(|active| active.module_id == module_id)
                        .map(|_| {
                            format!(
                                "cannot disable {module_id} while it is the active provider for {}; select another provider first",
                                provider.interface
                            )
                        })
                }),
            Ok(_) => None,
        };
        if let Some(message) = rejection {
            tracing::warn!(module_id, enabled, "{message}");
            self.diagnostics.record_lifecycle_error(
                "@mesh/settings".to_string(),
                "invalid_module_enabled_selection",
                message,
            );
            return VecDeque::new();
        }

        let module_kind = self
            .installed_module_graph
            .as_ref()
            .and_then(|graph| graph.module(module_id))
            .map(|module| module.kind)
            .expect("module existence was validated above");
        if self.file_watcher_tx.is_some() {
            let operation = ProfileWriteOperation::ModuleEnabled {
                graph_path: graph_path.clone(),
                module_id: module_id.to_string(),
                enabled,
                active_profile_id: active_profile_id.clone(),
            };
            let write_path = graph_path.clone();
            let write_module_id = module_id.to_string();
            if let Err(error) = self.start_profile_write(operation, move || {
                crate::shell::module_config::write_composed_module_enabled(
                    &write_path,
                    &write_module_id,
                    module_kind,
                    enabled,
                )
                .map(DurableProfileWriteResult::Rollback)
                .map_err(|error| error.to_string())
            }) {
                self.diagnostics.record_lifecycle_error(
                    "@mesh/settings",
                    "module_enabled_write_failed",
                    error.to_string(),
                );
            }
            return VecDeque::new();
        }
        let rollback = match crate::shell::module_config::write_composed_module_enabled(
            &graph_path,
            module_id,
            module_kind,
            enabled,
        ) {
            Ok(rollback) => rollback,
            Err(error) => {
                let message = format!("failed to save enabled state for {module_id}: {error}");
                tracing::warn!(module_id, enabled, "{message}");
                self.diagnostics.record_lifecycle_error(
                    "@mesh/settings".to_string(),
                    "module_enabled_write_failed",
                    message,
                );
                return VecDeque::new();
            }
        };

        if let Some(profile_id) = active_profile_id {
            // Profile-owned enablement changes are graph deltas too. Let the
            // profile coordinator rebuild the composition closure and stage
            // every affected root/provider before commit.
            return self.begin_profile_activation(&profile_id, Some(rollback));
        }

        let graph = match self.load_installed_module_graph_candidate() {
            Ok(graph) => graph,
            Err(error) => {
                let restore_error = rollback.restore().err();
                let message = format!(
                    "module {module_id} update produced an unloadable graph: {error}{}",
                    restore_error
                        .map(|error| format!("; rollback also failed: {error}"))
                        .unwrap_or_default()
                );
                tracing::warn!(module_id, enabled, "{message}");
                self.diagnostics.record_lifecycle_error(
                    "@mesh/settings".to_string(),
                    "module_enabled_graph_reload_failed",
                    message,
                );
                return VecDeque::new();
            }
        };
        self.begin_legacy_graph_activation(graph, Some(rollback))
    }

    /// Interfaces where `module_id` is the graph's active provider but has no
    /// live runtime yet — i.e. what enabling `module_id` must spawn so it
    /// doesn't wait for a restart. Pure graph/state query, no spawning.
    pub(in crate::shell) fn newly_active_backend_interfaces(
        &self,
        graph: &InstalledModuleGraph,
        module_id: &str,
    ) -> Vec<String> {
        graph
            .backend_provider_contributions()
            .into_iter()
            .filter(|provider| provider.module_id == module_id)
            .filter(|provider| {
                graph
                    .active_provider(&provider.interface)
                    .is_some_and(|active| active.module_id == module_id)
            })
            .map(|provider| provider.interface.clone())
            .filter(|interface| {
                !self
                    .backend_runtimes
                    .get(interface)
                    .is_some_and(|slot| slot.provider_id == module_id)
            })
            .collect()
    }

    fn apply_set_provider(&mut self, interface: &str, provider_id: &str) {
        if self.profile_transition_pending() {
            tracing::warn!(
                interface,
                provider_id,
                "provider change rejected during profile switch"
            );
            return;
        }
        let graph_path = self.installed_module_graph_path();
        let (graph, provider) = match self.load_installed_module_graph_cached() {
            Ok(graph) => {
                let provider = graph
                    .backend_providers_for_interface(interface)
                    .iter()
                    .find(|provider| provider.module_id == provider_id)
                    .cloned();
                (graph.clone(), provider)
            }
            Err(error) => {
                let message = format!(
                    "cannot select provider {provider_id} for {interface}: failed to load {}: {error}",
                    graph_path.display()
                );
                tracing::warn!(interface, provider_id, "{message}");
                self.diagnostics.record_lifecycle_error(
                    "@mesh/settings".to_string(),
                    "provider_selection_graph_load_failed",
                    message,
                );
                return;
            }
        };
        let Some(provider) = provider else {
            let message = format!(
                "cannot select provider {provider_id}: it is not an enabled provider for {interface}"
            );
            tracing::warn!(interface, provider_id, "{message}");
            self.diagnostics.record_lifecycle_error(
                "@mesh/settings".to_string(),
                "invalid_provider_selection",
                message,
            );
            return;
        };

        if self
            .pending_backend_runtimes
            .get(interface)
            .is_some_and(|pending| pending.slot.provider_id == provider_id)
        {
            tracing::debug!(
                interface,
                provider_id,
                "backend provider switch is already pending"
            );
            return;
        }
        if self
            .backend_runtimes
            .get(interface)
            .is_some_and(|slot| slot.provider_id == provider_id)
        {
            if let Some(pending) = self.pending_backend_runtimes.remove(interface) {
                let interface_name = pending.slot.interface.clone();
                let provider_name = pending.slot.provider_id.clone();
                self.retire_backend_runtime_slot(pending.slot);
                self.record_backend_runtime_status(
                    interface_name,
                    provider_name,
                    BackendRuntimeStatus::Stopped,
                    "provider switch cancelled".to_string(),
                );
            }
            if graph
                .active_provider(interface)
                .is_some_and(|active| active.module_id == provider_id)
            {
                tracing::debug!(interface, provider_id, "backend provider is already active");
                return;
            }
            if self.file_watcher_tx.is_some() {
                let operation = ProfileWriteOperation::ProviderSelection {
                    graph_path: graph_path.clone(),
                    interface: interface.to_string(),
                    provider_id: provider_id.to_string(),
                };
                let write_path = graph_path.clone();
                let write_interface = interface.to_string();
                let write_provider_id = provider_id.to_string();
                if let Err(error) = self.start_profile_write(operation, move || {
                    crate::shell::module_config::write_composed_provider_selection(
                        &write_path,
                        &write_interface,
                        &write_provider_id,
                    )
                    .map(|()| DurableProfileWriteResult::Complete)
                    .map_err(|error| error.to_string())
                }) {
                    self.diagnostics.record_lifecycle_error(
                        "@mesh/settings",
                        "provider_selection_write_failed",
                        error.to_string(),
                    );
                }
                return;
            }
            match crate::shell::module_config::write_composed_provider_selection(
                &graph_path,
                interface,
                provider_id,
            ) {
                Ok(()) => {
                    match self.load_installed_module_graph_candidate() {
                        Ok(candidate) => {
                            if let Err(error) = self.commit_installed_module_graph(candidate) {
                                let message = format!(
                                    "provider selection for {interface} was saved but locale catalogs could not be committed: {error}"
                                );
                                tracing::warn!(interface, provider_id, "{message}");
                                self.diagnostics.record_lifecycle_error(
                                    "@mesh/settings",
                                    "provider_selection_locale_reload_failed",
                                    message,
                                );
                            }
                        }
                        Err(error) => {
                            let message = format!(
                                "provider selection for {interface} was saved but the candidate graph could not be loaded: {error}"
                            );
                            tracing::warn!(interface, provider_id, "{message}");
                            self.diagnostics.record_lifecycle_error(
                                "@mesh/settings".to_string(),
                                "provider_selection_graph_reload_failed",
                                message,
                            );
                        }
                    }
                    tracing::info!(
                        interface,
                        provider_id,
                        "saved selection for the already-running provider"
                    );
                }
                Err(error) => {
                    let message =
                        format!("failed to save provider {provider_id} for {interface}: {error}");
                    tracing::warn!(interface, provider_id, "{message}");
                    self.diagnostics.record_lifecycle_error(
                        "@mesh/settings".to_string(),
                        "provider_selection_write_failed",
                        message,
                    );
                }
            }
            return;
        }
        let Some(ctx) = self.backend_respawn.clone() else {
            let message = format!(
                "cannot switch provider {provider_id} for {interface}: backend runtime is unavailable"
            );
            tracing::warn!(interface, provider_id, "{message}");
            self.diagnostics.record_lifecycle_error(
                "@mesh/settings".to_string(),
                "provider_switch_runtime_unavailable",
                message,
            );
            return;
        };
        let mut candidate =
            match crate::shell::backend::launch_candidate_for_provider_with_capabilities(
                &graph,
                &self.modules,
                &self.settings_store,
                self.interfaces.snapshot().as_ref(),
                &provider,
                &self.effective_capabilities,
            ) {
                Ok(candidate) => candidate,
                Err(status) => {
                    self.record_backend_runtime_status(
                        status.interface.clone(),
                        status
                            .provider_id
                            .clone()
                            .unwrap_or_else(|| provider_id.to_string()),
                        BackendRuntimeStatus::from_str(status.status),
                        status.message.clone(),
                    );
                    tracing::warn!(
                        interface,
                        provider_id,
                        "provider switch rejected: {}",
                        status.message
                    );
                    return;
                }
            };
        self.apply_shell_runtime_settings(&mut candidate);
        let slot = self.start_backend_candidate(&ctx.handle, ctx.tx, candidate, ctx.wake);
        self.stage_backend_runtime_switch(interface.to_string(), slot, graph_path);
        tracing::info!(
            interface,
            provider_id,
            "started provider candidate; current provider remains active until readiness"
        );
    }

    pub(in crate::shell) fn invalidate_debug_layout_bounds_targets(&mut self) {
        for runtime in &mut self.components {
            runtime.component.request_paint();
            runtime.parent.force_full_present = true;
            for child in &mut runtime.children {
                child.target.force_full_present = true;
            }
        }
    }

    fn open_debug_source_in_editor(&self, path: &str, line: u32) {
        let requested = match std::path::Path::new(path).canonicalize() {
            Ok(path) if path.extension().and_then(|ext| ext.to_str()) == Some("mesh") => path,
            _ => {
                tracing::warn!(path, "refusing to open invalid debug source path");
                return;
            }
        };
        let is_loaded_source = self.components.iter().any(|runtime| {
            runtime
                .component
                .watched_source_paths()
                .into_iter()
                .filter_map(|candidate| candidate.canonicalize().ok())
                .any(|candidate| candidate == requested)
        });
        if !is_loaded_source {
            tracing::warn!(path = %requested.display(), "refusing to open source outside loaded frontend modules");
            return;
        }

        let target = format!("{}:{}", requested.display(), line.max(1));
        if let Err(error) = std::process::Command::new("code")
            .arg("--goto")
            .arg(&target)
            .spawn()
        {
            tracing::warn!(%error, %target, "failed to launch VS Code for inspected element");
        }
    }

    fn clear_transfer_owned_keyboard_mode(
        &mut self,
        surface_id: &str,
    ) -> Option<mesh_core_wayland::KeyboardMode> {
        let previous_mode = self
            .transfer_owned_keyboard_modes
            .remove(surface_id)
            .or_else(|| {
                self.surfaces
                    .get(surface_id)
                    .map(|surface| surface.keyboard_mode)
            });
        let Some(previous_mode) = previous_mode else {
            return None;
        };
        if previous_mode == mesh_core_wayland::KeyboardMode::OnDemand {
            self.configure_surface_keyboard_mode(surface_id, mesh_core_wayland::KeyboardMode::None);
        }
        if let Some(surface) = self.surfaces.get_mut(surface_id) {
            surface.keyboard_mode = previous_mode;
        }
        if let Some(index) = self.component_index_for_surface(surface_id) {
            let runtime = &mut self.components[index];
            runtime.component.set_keyboard_mode_override(None);
        }
        Some(previous_mode)
    }

    fn configure_surface_keyboard_mode(
        &mut self,
        surface_id: &str,
        keyboard_mode: mesh_core_wayland::KeyboardMode,
    ) {
        let size_policy = self
            .component_index_for_surface(surface_id)
            .map(|index| self.components[index].parent.surface_size_policy)
            .unwrap_or(LayerSurfaceSizePolicy::Fixed);

        let (surface, visible) = match self.surfaces.get(surface_id) {
            Some(surface) => {
                let visible = self
                    .core
                    .surfaces
                    .get(surface_id)
                    .map(|state| state.visible)
                    .unwrap_or(surface.visible);
                (surface, visible)
            }
            None => return,
        };

        // Keyboard interactivity is a layer-shell request; a toplevel receives
        // keyboard focus by activation instead, so there is nothing to apply.
        // Skipping also protects the window from the synthetic 1x1 hidden
        // geometry below, which on a non-resizable window would be sent to the
        // compositor as a 1x1 min *and* max size.
        if surface.role == mesh_core_wayland::SurfaceRole::Window {
            return;
        }

        let cfg = if visible {
            let content = ContentExtent::from_size((surface.width.max(1), surface.height.max(1)))
                .expect("positive keyboard-mode surface extent");
            let extent = ConfiguredSurfaceExtent::from_content_and_padding(
                content,
                SurfacePadding::default(),
            )
            .expect("keyboard-mode surface extent has no padding");
            let wire_size =
                LayerWireSize::from_requested((surface.width, surface.height), extent.surface())
                    .expect("keyboard-mode surface wire extent is positive");
            SurfaceConfig {
                role: surface.role,
                window: surface.window.clone(),
                edge: surface.edge,
                layer: surface.layer.unwrap_or(Layer::Top),
                size_policy,
                // Content-sized: this path re-sends placement/keyboard state
                // for an already-sized surface and never applies the tooltip
                // overlay reserve, so nothing of it is input-inert padding.
                extent,
                wire_size,
                exclusive_zone: surface.exclusive_zone,
                keyboard_mode,
                namespace: surface_id.to_string(),
                margin_top: surface.margin_top,
                margin_right: surface.margin_right,
                margin_bottom: surface.margin_bottom,
                margin_left: surface.margin_left,
                blur: surface.blur,
                policy_revision: 0,
            }
        } else {
            SurfaceConfig {
                role: surface.role,
                window: surface.window.clone(),
                edge: surface.edge,
                layer: surface.layer.unwrap_or(Layer::Top),
                size_policy: LayerSurfaceSizePolicy::Fixed,
                extent: ConfiguredSurfaceExtent::from_content_and_padding(
                    ContentExtent::from_size((1, 1)).expect("positive hidden extent"),
                    SurfacePadding::default(),
                )
                .expect("hidden surface extent has no padding"),
                wire_size: LayerWireSize::fixed(1, 1)
                    .expect("hidden surface wire extent is positive"),
                exclusive_zone: 0,
                keyboard_mode: mesh_core_wayland::KeyboardMode::None,
                namespace: surface_id.to_string(),
                margin_top: 0,
                margin_right: 0,
                margin_bottom: 0,
                margin_left: 0,
                blur: surface.blur,
                policy_revision: 0,
            }
        };
        let cfg = super::render::revisioned_surface_config(
            self.component_index_for_surface(surface_id)
                .and_then(|index| self.components[index].parent.last_surface_config.as_ref()),
            cfg,
        );

        if let Err(error) = self.presentation_engine.configure(surface_id, cfg.clone()) {
            tracing::warn!(%error, %surface_id, "failed to configure surface keyboard mode");
            return;
        }
        if let Some(index) = self.component_index_for_surface(surface_id) {
            self.components[index].parent.last_surface_config = Some(cfg);
        }
    }

    pub(in crate::shell) fn claim_keyboard_focus_for_surface(&mut self, surface_id: &str) {
        let previous_focus = self.keyboard_focus_surface.clone();
        if let Some(previous) = previous_focus.as_deref()
            && previous != surface_id
        {
            if let Some(runtime) = self
                .components
                .iter_mut()
                .find(|runtime| runtime.surface_id == previous)
            {
                runtime.component.set_keyboard_mode_override(None);
            }
            if self.transfer_owned_keyboard_modes.contains_key(previous) {
                self.clear_transfer_owned_keyboard_mode(previous);
            }
        } else if previous_focus.as_deref() == Some(surface_id)
            && self.transfer_owned_keyboard_modes.contains_key(surface_id)
        {
            self.clear_transfer_owned_keyboard_mode(surface_id);
        }

        self.keyboard_focus_surface = Some(surface_id.to_string());
        if previous_focus.as_deref() != Some(surface_id) {
            if let Some(runtime) = self
                .components
                .iter_mut()
                .find(|runtime| runtime.surface_id == surface_id)
            {
                runtime.component.set_keyboard_mode_override(None);
            }
            if self.transfer_owned_keyboard_modes.contains_key(surface_id) {
                self.clear_transfer_owned_keyboard_mode(surface_id);
            }
        }
    }

    pub(in crate::shell) fn broadcast_core_event(
        &mut self,
        event: CoreEvent,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let mut requests = VecDeque::new();
        for component_index in 0..self.components.len() {
            if self.component_is_quarantined(component_index) {
                continue;
            }
            match self.components[component_index]
                .component
                .handle_core_event(&event)
            {
                Ok(component_requests) => requests.extend(component_requests),
                Err(error) => {
                    self.contain_component_failure(component_index, "callback", &error);
                }
            }
        }
        Ok(requests)
    }

    pub(in crate::shell) fn drain_requests(
        &mut self,
        requests: &mut VecDeque<CoreRequest>,
    ) -> Result<(), ShellRunError> {
        self.enqueue_effects(std::mem::take(requests));
        let report = self.process_effects()?;
        if report.deferred > 0
            || report.cycle_breaks > 0
            || report.transaction_budget_exceeded > 0
            || report.source_budget_exceeded > 0
        {
            let dropped = self.discard_scheduled_effects();
            let message = format!(
                "dropped {dropped} shell effects from a direct drain after exceeding scheduler policy"
            );
            tracing::error!("{message}");
            self.diagnostics.record_lifecycle_error(
                "@mesh/shell",
                "request_drain_budget_exceeded",
                message,
            );
        }
        Ok(())
    }

    pub(in crate::shell) fn drain_request(
        &mut self,
        request: CoreRequest,
    ) -> Result<(), ShellRunError> {
        self.enqueue_effects(std::iter::once(request));
        self.process_effects().map(|_| ())
    }

    pub(in crate::shell) fn apply_request(
        &mut self,
        request: CoreRequest,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        if !self.shutdown_phase.accepts_external_work()
            && !self.shutdown_effects_allowed
            && !matches!(&request, CoreRequest::Shutdown)
        {
            tracing::debug!(phase = ?self.shutdown_phase, "rejected request after shutdown quiescing");
            return Ok(VecDeque::new());
        }
        if request_changes_debug_snapshot(&request) {
            self.invalidate_debug_snapshot_cache();
        }
        let trigger_kind = profiling_trigger_for_request(&request);
        let profiling_started = self.profiling_enabled().then(std::time::Instant::now);
        let result = match request {
            CoreRequest::PositionSurface {
                surface_id,
                margin_top,
                margin_left,
            } => {
                if let Some(runtime) = self
                    .components
                    .iter_mut()
                    .find(|r| r.surface_id == surface_id)
                {
                    runtime.component.apply_position(margin_top, margin_left);
                }
                Ok(VecDeque::new())
            }
            CoreRequest::ToggleSurface { surface_id } => {
                let visible = self
                    .core
                    .surfaces
                    .get(&surface_id)
                    .map(|state| !state.visible)
                    .unwrap_or(true);
                self.set_surface_visibility(surface_id, visible)
            }
            CoreRequest::ShowSurface { surface_id } => {
                self.set_surface_visibility(surface_id, true)
            }
            CoreRequest::HideSurface { surface_id } => {
                self.pending_popover_hides.remove(&surface_id);
                self.set_surface_visibility(surface_id, false)
            }
            CoreRequest::HidePopover {
                surface_id,
                defer_for_hover_bridge,
            } => self.hide_popover(surface_id, defer_for_hover_bridge),
            CoreRequest::SetSurfaceRole { surface_id, role } => {
                self.set_surface_role(surface_id, role)
            }
            CoreRequest::ToggleSurfaceRole { surface_id } => {
                let role = match self
                    .component_index_for_surface(&surface_id)
                    .map(|index| self.components[index].component.surface_role())
                {
                    Some(mesh_core_wayland::SurfaceRole::Window) => {
                        mesh_core_wayland::SurfaceRole::Layer
                    }
                    Some(mesh_core_wayland::SurfaceRole::Layer) => {
                        mesh_core_wayland::SurfaceRole::Window
                    }
                    None => {
                        tracing::warn!(%surface_id, "cannot toggle surface role: no such surface");
                        return Ok(VecDeque::new());
                    }
                };
                self.set_surface_role(surface_id, role)
            }
            CoreRequest::SetChildSurfaceRole {
                surface_id,
                node_key,
                role,
            } => self.set_child_surface_role(surface_id, node_key, role),
            CoreRequest::PublishDiagnostics { message } => {
                tracing::info!("diagnostic: {message}");
                Ok(VecDeque::new())
            }
            CoreRequest::WriteClipboard { text } => {
                if let Err(err) = self.clipboard.write_text(&text) {
                    tracing::warn!(error = %err, "failed to write selection to clipboard");
                }
                Ok(VecDeque::new())
            }
            CoreRequest::ServiceCommand {
                interface,
                command,
                payload,
                source_module_id,
                source_capabilities,
            } => {
                let _ = self.dispatch_service_command(
                    &interface,
                    &command,
                    &payload,
                    &source_module_id,
                    &source_capabilities,
                );
                Ok(VecDeque::new())
            }
            CoreRequest::ServiceCall {
                interface,
                command,
                payload,
                call_id,
                source_instance_id,
                source_module_id,
                source_capabilities,
            } => {
                let interface_canonical = canonical_interface_name_cow(&interface);
                let generation = self
                    .backend_runtimes
                    .get(interface_canonical.as_ref())
                    .map(|slot| slot.generation)
                    .unwrap_or(0);
                let identity = self.backend_identity_for_interface(interface_canonical.as_ref());
                let route = ServiceCallRoute {
                    interface: interface_canonical.into_owned(),
                    instance_id: source_instance_id,
                    module_id: source_module_id.clone(),
                    generation,
                    identity,
                };
                self.pending_service_call_routes.insert(call_id, route);
                let result = self.dispatch_service_command_with_call_id(
                    &interface,
                    &command,
                    &payload,
                    &source_module_id,
                    &source_capabilities,
                    mesh_core_backend::CallId::from_raw(call_id),
                );
                if result.get("queued").and_then(|value| value.as_bool()) != Some(true) {
                    let status = result
                        .get("status")
                        .and_then(|value| value.as_str())
                        .unwrap_or("failed")
                        .to_string();
                    self.complete_service_call_route(
                        mesh_core_backend::CallId::from_raw(call_id),
                        &status,
                        &result,
                    );
                }
                Ok(VecDeque::new())
            }
            CoreRequest::CancelServiceCall {
                interface,
                call_id,
                source_instance_id,
                source_module_id,
                source_capabilities,
            } => {
                self.cancel_service_call(
                    &interface,
                    call_id,
                    &source_instance_id,
                    &source_module_id,
                    &source_capabilities,
                );
                Ok(VecDeque::new())
            }
            CoreRequest::ActivatePopover {
                surface_id,
                trigger_surface,
                trigger_key,
                focus,
            } => {
                self.cancel_pending_popover_hide(&surface_id);
                if let Some(index) = self.component_index_for_surface(&trigger_surface) {
                    let runtime = &mut self.components[index];
                    if !trigger_key.is_empty() {
                        runtime
                            .component
                            .register_popover_trigger(trigger_key.clone(), surface_id.clone());
                    }
                }
                let target_runtime_found = self.component_index_for_surface(&surface_id).is_some();

                // Promote to xdg_popup when the compositor supports it and the
                // trigger surface is known. The anchor rect is built from the
                // trigger surface's exclusive-zone (bar height) and the popover's
                // current margin-left (set by the preceding shell.position-surface
                // event), both of which are in the parent surface's coordinate
                // space and available before the next render frame.
                // A popover re-activates itself from its own `onpointerenter`
                // (a keep-alive that cancels the trigger's pending close while
                // the cursor travels onto it). That re-entrant call must not
                // re-promote the popup: `trigger_surface` would be the popover
                // itself, so it would be re-parented to itself and its
                // anchor_rect (in the original parent's space) would be
                // reinterpreted against the tiny popover surface, sliding it to
                // a screen edge. Only promote when the trigger is a *different*
                // surface (the real anchor).
                if self.presentation_engine.popup_supported()
                    && !trigger_surface.is_empty()
                    && trigger_surface != surface_id
                    && target_runtime_found
                {
                    let trigger_exclusive_zone = self
                        .surfaces
                        .get(&trigger_surface)
                        .map(|s| s.exclusive_zone)
                        .unwrap_or(40);
                    // Anchor the popup to the trigger element's *real* rect in
                    // the parent surface, then let the compositor center it
                    // (anchor = bottom-center of the trigger, gravity = down).
                    // This is width-agnostic: the popover may measure wider than
                    // any value the component could predict, and the compositor
                    // keeps it centered and on-screen (flip/slide) as it grows.
                    // The component must NOT compute its own left edge from an
                    // assumed popover width — that hardcoding caused near-edge
                    // popovers to overflow and get slid sideways once their
                    // measured size exceeded the assumption.
                    let trigger_rect =
                        self.component_index_for_surface(&trigger_surface)
                            .and_then(|idx| {
                                self.components[idx]
                                    .component
                                    .node_bounds_by_key(&trigger_key)
                            });
                    let (anchor_x, anchor_w) = match trigger_rect {
                        Some((left, _top, right, _bottom)) => {
                            (left.round() as i32, ((right - left).round() as i32).max(1))
                        }
                        // Fall back to the component-reported margin-left (the
                        // legacy single-edge anchor) when the trigger rect is
                        // unavailable, e.g. activation without an event.
                        None => (
                            self.component_index_for_surface(&surface_id)
                                .map(|idx| self.components[idx].component.popover_margin_left())
                                .unwrap_or(0),
                            1,
                        ),
                    };
                    let popup_config = PopupConfig {
                        parent_surface_id: trigger_surface.clone(),
                        // A promoted popover surface carries no overshoot ring:
                        // its size is its content.
                        padding: SurfacePadding::default(),
                        placement: PopupPlacement {
                            anchor_rect: (anchor_x, 0, anchor_w, trigger_exclusive_zone),
                            size: (1, 1),
                            anchor: PopupAnchor::Bottom,
                            gravity: PopupGravity::Bottom,
                            constraint: PopupConstraint::default(),
                            offset: (0, 0),
                        },
                        grab: false,
                        grab_identity: None,
                    };
                    // Legacy path: this promotes a *separate* popover module's
                    // own parent surface into an xdg_popup. The newer model
                    // (auto-derived child surfaces from in-tree `<popover open>`
                    // nodes of a single component VM) supersedes this; kept for
                    // shipped separate-module popovers during the transition.
                    if let Some(idx) = self.component_index_for_surface(&surface_id) {
                        self.components[idx].parent.popup_parent_surface =
                            Some(trigger_surface.clone());
                        self.components[idx].parent.popover_relationship =
                            (!trigger_key.is_empty()).then(|| PopoverSurfaceRelationship {
                                trigger_surface_id: trigger_surface.clone(),
                                trigger_reference: PopoverTriggerReference {
                                    reference: trigger_key.clone(),
                                },
                                popup_surface_id: surface_id.clone(),
                                popup_node_key: "root".to_string(),
                            });
                        self.components[idx].parent.popup_config = Some(popup_config);
                        self.components[idx].parent.last_popup_size = None;
                        self.components[idx].component.set_popup_promoted(true);
                    }
                    tracing::info!(
                        "ActivatePopover: promoting {surface_id} as xdg_popup child of {trigger_surface} trigger_rect=({anchor_x},{anchor_w}) bar_h={trigger_exclusive_zone}"
                    );
                }

                let mut emitted = self.sibling_popover_hides(&surface_id, &trigger_surface);
                emitted.push_back(CoreRequest::ShowSurface {
                    surface_id: surface_id.clone(),
                });
                if focus && !trigger_surface.is_empty() && !trigger_key.is_empty() {
                    emitted.push_back(CoreRequest::TransferTabFocus {
                        from_surface: trigger_surface.clone(),
                        to_surface: surface_id.clone(),
                        target: TabFocusTarget::First,
                        return_target: Some((trigger_surface, trigger_key)),
                        target_closes_on_leave: true,
                        close_source: None,
                    });
                }
                Ok(emitted)
            }
            CoreRequest::TransferTabFocus {
                from_surface,
                to_surface,
                target,
                return_target,
                target_closes_on_leave,
                close_source,
            } => self.apply_transfer_tab_focus(
                &from_surface,
                &to_surface,
                target,
                return_target,
                target_closes_on_leave,
                close_source,
            ),
            CoreRequest::SetTheme { theme_id } => self.apply_set_theme(&theme_id),
            CoreRequest::SetThemeMode { mode } => self.apply_set_theme_mode(&mode),
            CoreRequest::SetLocale { locale } => self.apply_set_locale(&locale),
            CoreRequest::SetIconTheme { theme_id } => self.apply_set_icon_theme(&theme_id),
            CoreRequest::SetFontFamily { family } => self.apply_set_font_family(&family),
            CoreRequest::SetProvider {
                interface,
                provider_id,
            } => {
                self.apply_set_provider(&interface, &provider_id);
                Ok(VecDeque::new())
            }
            CoreRequest::SetModuleEnabled { module_id, enabled } => {
                Ok(self.apply_set_module_enabled(&module_id, enabled))
            }
            CoreRequest::InstallModule {
                source,
                profile_id,
                available_only,
                allow_elevated,
                allow_high,
            } => self.apply_install_module(
                &source,
                profile_id.as_deref(),
                available_only,
                allow_elevated,
                allow_high,
            ),
            CoreRequest::UninstallModule { module_id, force } => {
                self.apply_uninstall_module(&module_id, force)
            }
            CoreRequest::SetModuleProp {
                module_id,
                instance_id,
                prop,
                value,
            } => self.apply_set_module_prop(&module_id, instance_id.as_deref(), &prop, Some(value)),
            CoreRequest::UnsetModuleProp {
                module_id,
                instance_id,
                prop,
            } => self.apply_set_module_prop(&module_id, instance_id.as_deref(), &prop, None),
            CoreRequest::ApplyNodeSlot {
                profile_id,
                root_instance,
                slot,
                nodes,
                expected_generation,
            } => self.apply_node_slot_edit(
                &profile_id,
                &root_instance,
                &slot,
                Some(nodes),
                &expected_generation,
            ),
            CoreRequest::ResetNodeSlot {
                profile_id,
                root_instance,
                slot,
                expected_generation,
            } => self.apply_node_slot_edit(
                &profile_id,
                &root_instance,
                &slot,
                None,
                &expected_generation,
            ),
            CoreRequest::SwitchProfile { profile_id } => Ok(self.apply_switch_profile(&profile_id)),
            CoreRequest::ToggleDebugOverlay => {
                self.debug.toggle();
                tracing::debug!(
                    "debug overlay: {}",
                    if self.debug.enabled { "on" } else { "off" }
                );
                self.set_surface_visibility(
                    DEBUG_INSPECTOR_SURFACE_ID.to_string(),
                    self.debug.enabled,
                )
            }
            CoreRequest::ToggleDebugLayoutBounds => {
                self.debug.toggle_layout_bounds();
                tracing::debug!(
                    "debug layout bounds: {}",
                    if self.debug.show_layout_bounds {
                        "on"
                    } else {
                        "off"
                    }
                );
                self.invalidate_debug_layout_bounds_targets();
                Ok(VecDeque::new())
            }
            CoreRequest::ToggleDebugElementPicker => {
                self.debug.toggle_element_picker();
                self.invalidate_debug_layout_bounds_targets();
                if self.debug.element_picker_enabled && !self.debug.enabled {
                    self.debug.enabled = true;
                    self.set_surface_visibility(DEBUG_INSPECTOR_SURFACE_ID.to_string(), true)
                } else {
                    Ok(VecDeque::new())
                }
            }
            CoreRequest::OpenDebugSource { path, line } => {
                self.open_debug_source_in_editor(&path, line);
                Ok(VecDeque::new())
            }
            CoreRequest::ToggleDebugProfiling => {
                let enabled = self.debug.toggle_profiling();
                if enabled {
                    self.profiling
                        .reset_for_new_session(self.debug.profiling_session_id);
                }
                tracing::debug!("debug profiling: {}", if enabled { "on" } else { "off" });
                Ok(VecDeque::new())
            }
            CoreRequest::RunDebugBenchmark { scenario_id } => {
                self.apply_run_debug_benchmark(&scenario_id)
            }
            CoreRequest::CycleDebugTab => {
                self.debug.cycle_tab();
                Ok(VecDeque::new())
            }
            CoreRequest::Shutdown => {
                self.begin_shutdown();
                Ok(VecDeque::new())
            }
        };
        if let Some(started) = profiling_started
            && result.is_ok()
        {
            self.record_shell_profiling_stage(
                mesh_core_debug::ProfilingStage::RuntimeUpdateHandling,
                started.elapsed(),
                Some(trigger_kind),
            );
        }
        result
    }

    pub(in crate::shell) fn dispatch_service_command(
        &mut self,
        interface: &str,
        command: &str,
        payload: &serde_json::Value,
        source_module_id: &str,
        source_capabilities: &mesh_core_capability::CapabilitySet,
    ) -> serde_json::Value {
        self.dispatch_service_command_with_call_id(
            interface,
            command,
            payload,
            source_module_id,
            source_capabilities,
            mesh_core_backend::CallId::next(),
        )
    }

    fn dispatch_service_command_with_call_id(
        &mut self,
        interface: &str,
        command: &str,
        payload: &serde_json::Value,
        source_module_id: &str,
        source_capabilities: &mesh_core_capability::CapabilitySet,
        call_id: mesh_core_backend::CallId,
    ) -> serde_json::Value {
        let interface_canonical = canonical_interface_name_cow(interface);
        let service_caps = service_capabilities(interface_canonical.as_ref());
        let required = &service_caps.control;
        let contract = self
            .interfaces
            .resolve(interface_canonical.as_ref(), None)
            .contract;
        let authorized = contract.as_ref().map_or_else(
            || source_capabilities.is_granted(required),
            |contract| {
                mesh_core_scripting::host_api::InterfaceProxy::can_call_contract_method(
                    source_capabilities,
                    contract,
                    command,
                )
            },
        );
        if !authorized {
            tracing::warn!(
                source_module_id,
                interface,
                command,
                required_capability = %required,
                "denied unauthorized service command dispatch"
            );
            self.record_method_call(mesh_core_debug::MethodCallEntry {
                call_id: call_id.raw(),
                interface: interface_canonical.to_string(),
                provider_id: None,
                source_module_id: source_module_id.to_string(),
                command: command.to_string(),
                status: "capability_denied".to_string(),
                queued: false,
                result: Some(serde_json::json!({
                    "ok": false,
                    "error": "capability_denied",
                    "status": "capability_denied",
                })),
                error: Some("capability_denied".to_string()),
            });
            return serde_json::json!({
                "ok": false,
                "error": "capability_denied",
                "status": "capability_denied",
                "call_id": call_id.raw(),
            });
        }

        if !self.service_command_is_supported(interface_canonical.as_ref(), command) {
            let message = format!("unsupported_service_command: {interface}.{command}");
            tracing::warn!(
                source_module_id,
                interface,
                command,
                "unsupported_service_command"
            );
            self.diagnostics.record_lifecycle_error(
                source_module_id.to_string(),
                "unsupported_service_command",
                message.clone(),
            );
            self.record_method_call(mesh_core_debug::MethodCallEntry {
                call_id: call_id.raw(),
                interface: interface_canonical.to_string(),
                provider_id: None,
                source_module_id: source_module_id.to_string(),
                command: command.to_string(),
                status: "unsupported_service_command".to_string(),
                queued: false,
                result: Some(serde_json::json!({
                    "ok": false,
                    "error": message,
                    "status": "unsupported_service_command",
                })),
                error: Some(message.clone()),
            });
            return serde_json::json!({
                "ok": false,
                "error": message,
                "status": "unsupported_service_command",
                "call_id": call_id.raw(),
            });
        }

        if let Err(message) = mesh_core_backend::validate_command_payload(payload) {
            let result = serde_json::json!({
                "ok": false,
                "error": message,
                "status": "invalid_arguments",
                "call_id": call_id.raw(),
            });
            self.record_method_call(mesh_core_debug::MethodCallEntry {
                call_id: call_id.raw(),
                interface: interface_canonical.to_string(),
                provider_id: None,
                source_module_id: source_module_id.to_string(),
                command: command.to_string(),
                status: "invalid_arguments".to_string(),
                queued: false,
                result: Some(result.clone()),
                error: result
                    .get("error")
                    .and_then(|value| value.as_str())
                    .map(ToOwned::to_owned),
            });
            return result;
        }

        if let Some(contract) = contract.as_ref() {
            let warnings =
                service_state::service_method_input_contract_warnings(contract, command, payload);
            if !warnings.is_empty() {
                let message = warnings.join("; ");
                tracing::warn!(
                    source_module_id,
                    interface,
                    command,
                    error = %message,
                    "rejected service command with invalid contract payload"
                );
                self.diagnostics.record_lifecycle_error(
                    source_module_id.to_string(),
                    "invalid_service_command_payload",
                    message.clone(),
                );
                let result = serde_json::json!({
                    "ok": false,
                    "error": message,
                    "status": "invalid_service_command_payload",
                    "call_id": call_id.raw(),
                });
                self.record_method_call(mesh_core_debug::MethodCallEntry {
                    call_id: call_id.raw(),
                    interface: interface_canonical.to_string(),
                    provider_id: None,
                    source_module_id: source_module_id.to_string(),
                    command: command.to_string(),
                    status: "invalid_service_command_payload".to_string(),
                    queued: false,
                    result: Some(result.clone()),
                    error: result
                        .get("error")
                        .and_then(|value| value.as_str())
                        .map(ToOwned::to_owned),
                });
                return result;
            }
        }

        let interface = interface_canonical.as_ref();
        // Interfaces the shell provides itself answer here rather than on a
        // backend command queue. The capability check above already ran, so a
        // caller reaches this point exactly when it holds
        // `service.<name>.control` — no module id is consulted.
        let core_request = self
            .core_service_providers
            .request(interface, command, payload)
            .or_else(|| core_service_request(interface, command, payload));
        let mut dispatch_result = if let Some(request) = core_request {
            let result = if self.effect_scheduler.active_context().is_some() {
                self.effect_scheduler
                    .enqueue_followups(std::iter::once(request));
                Ok(VecDeque::new())
            } else {
                self.apply_request(request)
            };
            match result {
                Ok(follow_ups) => {
                    self.enqueue_effects(follow_ups);
                    serde_json::json!({ "ok": true, "status": "applied" })
                }
                Err(error) => {
                    let message = error.to_string();
                    tracing::warn!(
                        interface,
                        command,
                        error = %message,
                        "core service command failed"
                    );
                    serde_json::json!({
                        "ok": false,
                        "error": message,
                        "status": "failed",
                    })
                }
            }
        } else if self.service_handlers.contains_key(interface) {
            let coalesce = self.service_command_is_coalescable(interface, command);
            if coalesce {
                let coalesce_key = command_coalescing_key(command, payload);
                let key = (
                    interface.to_string(),
                    command.to_string(),
                    coalesce_key.clone(),
                );
                let now = std::time::Instant::now();
                let entry = self.command_throttle.get(&key);
                let allow_send = entry
                    .map(|state| now.duration_since(state.last_send) >= COMMAND_THROTTLE_INTERVAL)
                    .unwrap_or(true);
                if !self.command_throttle.contains_key(&key)
                    && self.command_throttle.len() >= MAX_COALESCED_COMMAND_KEYS
                {
                    service_queue_full_response()
                } else if allow_send {
                    self.command_throttle.insert(
                        key,
                        CommandThrottleState {
                            last_send: now,
                            pending: None,
                        },
                    );
                    match self.send_service_command_message(
                        interface, command, payload, coalesce, call_id,
                    ) {
                        Some(Ok(())) => serde_json::json!({ "ok": true, "queued": true }),
                        Some(Err(ServiceCommandSendError::Full)) => service_queue_full_response(),
                        Some(Err(ServiceCommandSendError::Closed)) | None => {
                            service_unavailable_response()
                        }
                    }
                } else {
                    let state =
                        self.command_throttle
                            .entry(key)
                            .or_insert_with(|| CommandThrottleState {
                                last_send: now,
                                pending: None,
                            });
                    let superseded = state.pending.replace(PendingServiceCommand {
                        call_id,
                        payload: payload.clone(),
                    });
                    if let Some(superseded) = superseded {
                        self.complete_service_call_route(
                            superseded.call_id,
                            "superseded",
                            &serde_json::json!({
                                "ok": false,
                                "status": "superseded",
                                "error": "throttled by a newer invocation",
                            }),
                        );
                    }
                    serde_json::json!({ "ok": true, "queued": true, "throttled": true })
                }
            } else {
                match self
                    .send_service_command_message(interface, command, payload, coalesce, call_id)
                {
                    Some(Ok(())) => serde_json::json!({ "ok": true, "queued": true }),
                    Some(Err(ServiceCommandSendError::Full)) => service_queue_full_response(),
                    Some(Err(ServiceCommandSendError::Closed)) | None => {
                        service_unavailable_response()
                    }
                }
            }
        } else {
            tracing::debug!("no handler registered for service: {interface}");
            service_unavailable_response()
        };

        dispatch_result["call_id"] = serde_json::json!(call_id.raw());

        if dispatch_result
            .get("ok")
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
        {
            let state_binding = self
                .interfaces
                .resolve(interface_canonical.as_ref(), None)
                .contract
                .as_ref()
                .and_then(|contract| {
                    contract
                        .methods
                        .iter()
                        .find(|method| method.name == command)
                        .and_then(|method| method.state_binding.clone())
                });
            if let Some(binding) = state_binding
                && let Some(value) =
                    self.bound_value_for_command(interface_canonical.as_ref(), &binding, payload)
            {
                self.apply_bound_service_state(
                    interface_canonical.as_ref(),
                    &binding.field,
                    value,
                    dispatch_result
                        .get("queued")
                        .and_then(|queued| queued.as_bool())
                        .unwrap_or(false)
                        .then_some(call_id),
                );
                dispatch_result["state_bound"] = serde_json::json!(true);
            }
        }

        let provider_id = self
            .backend_runtimes
            .get(interface)
            .map(|slot| slot.provider_id.clone());
        let queued = dispatch_result
            .get("queued")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let status = dispatch_result
            .get("status")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| {
                if queued {
                    "queued".to_string()
                } else {
                    "failed".to_string()
                }
            });
        let error = dispatch_result
            .get("error")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned);
        self.record_method_call(mesh_core_debug::MethodCallEntry {
            call_id: call_id.raw(),
            interface: interface.to_string(),
            provider_id,
            source_module_id: source_module_id.to_string(),
            command: command.to_string(),
            status,
            queued,
            result: Some(dispatch_result.clone()),
            error,
        });

        dispatch_result
    }

    pub(in crate::shell) fn complete_service_call_route(
        &mut self,
        call_id: mesh_core_backend::CallId,
        status: &str,
        result: &serde_json::Value,
    ) {
        self.settle_bound_service_state(call_id, status);
        let Some(route) = self.pending_service_call_routes.remove(&call_id.raw()) else {
            return;
        };
        let delivered = self.components.iter_mut().any(|runtime| {
            runtime.component.deliver_service_call_result(
                &route.instance_id,
                call_id.raw(),
                status,
                result,
            )
        });
        if !delivered {
            self.diagnostics.record_lifecycle_error(
                route.module_id,
                "service_call_result_dropped",
                format!(
                    "dropped {status} result for {} call {}: originating instance '{}' is no longer mounted",
                    route.interface,
                    call_id.raw(),
                    route.instance_id
                ),
            );
        }
    }

    fn cancel_service_call(
        &mut self,
        interface: &str,
        call_id: u64,
        source_instance_id: &str,
        source_module_id: &str,
        source_capabilities: &mesh_core_capability::CapabilitySet,
    ) {
        let call = mesh_core_backend::CallId::from_raw(call_id);
        let Some(route) = self.pending_service_call_routes.get(&call_id) else {
            return;
        };
        let capabilities = service_capabilities(interface);
        let required = &capabilities.control;
        if !source_capabilities.is_granted(required)
            || route.instance_id != source_instance_id
            || route.module_id != source_module_id
        {
            self.diagnostics.record_lifecycle_error(
                source_module_id.to_string(),
                "service_call_cancel_denied",
                format!("denied cancellation for {interface} call {call_id}"),
            );
            return;
        }

        let mut removed_from_throttle = false;
        let mut empty_throttle_keys = Vec::new();
        for (key, state) in &mut self.command_throttle {
            if state
                .pending
                .as_ref()
                .is_some_and(|pending| pending.call_id == call)
            {
                state.pending = None;
                removed_from_throttle = true;
                empty_throttle_keys.push(key.clone());
            }
        }
        for key in empty_throttle_keys {
            self.command_throttle.remove(&key);
        }

        if removed_from_throttle {
            self.complete_service_call_route(
                call,
                "cancelled",
                &serde_json::json!({
                    "ok": false,
                    "status": "cancelled",
                    "error": "cancelled before dispatch",
                }),
            );
        } else if !mesh_core_backend::cancel_call(call) {
            // A call-control entry disappears only after the backend has
            // emitted its terminal result. If it is not present here, the
            // call cannot produce another useful result, so close the ticket
            // instead of leaving Luau polling forever.
            self.complete_service_call_route(
                call,
                "cancelled",
                &serde_json::json!({
                    "ok": false,
                    "status": "cancelled",
                    "error": "cancelled",
                }),
            );
        }
    }

    fn send_service_command_message(
        &mut self,
        interface: &str,
        command: &str,
        payload: &serde_json::Value,
        coalesce: bool,
        call_id: mesh_core_backend::CallId,
    ) -> Option<Result<(), ServiceCommandSendError>> {
        let tx = self.service_handlers.get(interface).cloned()?;
        let active_provider = self.backend_runtimes.get(interface).map(|slot| {
            (
                slot.provider_id.clone(),
                slot.generation,
                *slot
                    .identity
                    .read()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()),
            )
        });
        let generation = active_provider
            .as_ref()
            .map(|(_, generation, _)| *generation)
            .unwrap_or(0);
        let identity = active_provider
            .as_ref()
            .map(|(_, _, identity)| *identity)
            .unwrap_or_default();
        mesh_core_backend::register_call_for_generation_and_identity(
            call_id,
            SERVICE_CALL_TIMEOUT,
            generation,
            identity,
        );
        let active_provider_id = active_provider.map(|(provider_id, _, _)| provider_id);
        let profiling_started = (self.profiling_enabled() && active_provider_id.is_some())
            .then(std::time::Instant::now);
        let result = tx
            .try_send(ServiceCommandMsg {
                call_id,
                command: command.to_string(),
                payload: payload.clone(),
                coalesce,
            })
            .map_err(|error| match error {
                tokio::sync::mpsc::error::TrySendError::Full(_) => ServiceCommandSendError::Full,
                tokio::sync::mpsc::error::TrySendError::Closed(_) => {
                    ServiceCommandSendError::Closed
                }
            });
        if result.is_err() {
            mesh_core_backend::finish_call(call_id);
        }
        if let (Some(provider_id), Some(started)) = (active_provider_id, profiling_started) {
            self.record_backend_profiling_stage(
                interface,
                &provider_id,
                ProfilingBackendStage::CommandHandling,
                started.elapsed(),
                Some("service_command"),
            );
        }
        Some(result)
    }

    fn apply_run_debug_benchmark(
        &mut self,
        scenario_id: &str,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let Some(scenario) = benchmark_scenario_id(scenario_id) else {
            let mut emitted = VecDeque::new();
            emitted.push_back(CoreRequest::PublishDiagnostics {
                message: format!("unknown debug benchmark scenario: {scenario_id}"),
            });
            return Ok(emitted);
        };

        self.debug.latest_benchmark_run = Some(DebugBenchmarkRunState {
            scenario_id: scenario,
            status: BenchmarkScenarioStatus::WaitingForSamples,
        });

        let mut emitted = VecDeque::new();
        if scenario == BenchmarkScenarioId::SurfaceOpenClose {
            emitted.push_back(CoreRequest::ToggleSurface {
                surface_id: "@mesh/audio-popover".to_string(),
            });
        }
        Ok(emitted)
    }

    /// Apply a cross-surface tab focus transfer. Clears focus on the
    /// source surface, hands focus to the target with the requested
    /// position, swaps `keyboard_mode` so the compositor delivers keys to
    /// the new owner, and emits HideSurface for `close_source` if set.
    fn apply_transfer_tab_focus(
        &mut self,
        from_surface: &str,
        to_surface: &str,
        target: TabFocusTarget,
        return_target: Option<(SurfaceId, String)>,
        target_closes_on_leave: bool,
        close_source: Option<SurfaceId>,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        tracing::info!(
            "apply_transfer_tab_focus from={from_surface} to={to_surface} return_target={return_target:?} target_closes_on_leave={target_closes_on_leave} close_source={close_source:?}"
        );
        if let Some(index) = self.component_index_for_surface(from_surface) {
            {
                let runtime = &mut self.components[index];
                runtime.component.release_focus_for_transfer();
                runtime
                    .component
                    .set_keyboard_mode_override(Some(mesh_core_wayland::KeyboardMode::None));
            }
            // Source: clear any prior override and force None so the
            // compositor can hand keyboard delivery to the target.
            self.clear_transfer_owned_keyboard_mode(from_surface);
        }
        if let Some(surface) = self.surfaces.get_mut(from_surface) {
            surface.keyboard_mode = mesh_core_wayland::KeyboardMode::None;
        }

        let target_owns_keyboard = target_closes_on_leave || close_source.is_some();
        let target_restore_keyboard_mode = self
            .transfer_owned_keyboard_modes
            .remove(to_surface)
            .or_else(|| {
                self.surfaces
                    .get(to_surface)
                    .map(|surface| surface.keyboard_mode)
            });

        let target_found = if let Some(index) = self.component_index_for_surface(to_surface) {
            let runtime = &mut self.components[index];
            runtime.component.receive_focus_transfer(
                &target,
                return_target,
                target_closes_on_leave,
            );
            // Target: Exclusive while it owns cross-surface keyboard focus.
            // This includes the return leg from a closing popover; falling
            // back to OnDemand there leaves some compositors with no concrete
            // surface delivering subsequent key events.
            let mode = if target_owns_keyboard {
                Some(mesh_core_wayland::KeyboardMode::Exclusive)
            } else {
                None
            };
            runtime.component.set_keyboard_mode_override(mode);
            if target_owns_keyboard {
                if let Some(restore_mode) = target_restore_keyboard_mode {
                    self.transfer_owned_keyboard_modes
                        .insert(to_surface.to_string(), restore_mode);
                } else {
                    self.transfer_owned_keyboard_modes.insert(
                        to_surface.to_string(),
                        mesh_core_wayland::KeyboardMode::None,
                    );
                }
                if let Some(surface) = self.surfaces.get_mut(to_surface) {
                    surface.keyboard_mode = mesh_core_wayland::KeyboardMode::Exclusive;
                }
            } else {
                self.transfer_owned_keyboard_modes.remove(to_surface);
                if let Some(surface) = self.surfaces.get_mut(to_surface) {
                    surface.keyboard_mode = mesh_core_wayland::KeyboardMode::OnDemand;
                }
            }
            true
        } else {
            tracing::warn!(
                to_surface,
                "TransferTabFocus target component not found; ignoring"
            );
            if let Some(restore_mode) = target_restore_keyboard_mode {
                if let Some(surface) = self.surfaces.get_mut(to_surface) {
                    surface.keyboard_mode = restore_mode;
                }
            } else {
                self.transfer_owned_keyboard_modes.remove(to_surface);
            }
            false
        };

        tracing::info!("apply_transfer_tab_focus target_found={target_found} to={to_surface}");
        if !target_found {
            return Ok(VecDeque::new());
        }

        self.keyboard_focus_surface = Some(to_surface.to_string());

        let mut emitted = VecDeque::new();
        if let Some(close) = close_source {
            emitted.push_back(CoreRequest::HideSurface { surface_id: close });
        }
        Ok(emitted)
    }

    fn service_command_is_supported(&self, interface: &str, command: &str) -> bool {
        let resolution = self.interfaces.resolve(interface, None);
        let Some(contract) = resolution.contract.as_ref() else {
            return true;
        };
        contract.methods.iter().any(|method| method.name == command)
    }

    /// Trailing-edge flush. Called once per main-loop tick: any throttled
    /// command whose interval has elapsed since its last send is dispatched
    /// now with the most recent payload. Stale entries (no pending payload
    /// and well past their interval) are pruned to keep the map bounded.
    pub(in crate::shell) fn flush_throttled_commands(&mut self) {
        if self.command_throttle.is_empty() {
            return;
        }
        let now = std::time::Instant::now();
        let mut to_send: Vec<(String, String, String, PendingServiceCommand)> = Vec::new();
        let mut to_remove: Vec<(String, String, String)> = Vec::new();
        for (key, state) in self.command_throttle.iter_mut() {
            if now.duration_since(state.last_send) < COMMAND_THROTTLE_INTERVAL {
                continue;
            }
            if let Some(command) = state.pending.take() {
                to_send.push((key.0.clone(), key.1.clone(), key.2.clone(), command));
                state.last_send = now;
            } else if now.duration_since(state.last_send)
                >= COMMAND_THROTTLE_INTERVAL.saturating_mul(8)
            {
                to_remove.push(key.clone());
            }
        }
        for (interface, command, _coalesce_key, pending) in to_send {
            let sent = self.send_service_command_message(
                &interface,
                &command,
                &pending.payload,
                true,
                pending.call_id,
            );
            if !matches!(sent, Some(Ok(()))) {
                let failure = match sent {
                    Some(Err(ServiceCommandSendError::Full)) => service_queue_full_response(),
                    _ => service_unavailable_response(),
                };
                self.complete_service_call_route(
                    pending.call_id,
                    failure
                        .get("status")
                        .and_then(|value| value.as_str())
                        .unwrap_or("service_unavailable"),
                    &failure,
                );
            }
        }
        for key in to_remove {
            self.command_throttle.remove(&key);
        }
    }

    fn service_command_is_coalescable(&self, interface: &str, command: &str) -> bool {
        let resolution = self.interfaces.resolve(interface, None);
        resolution
            .contract
            .as_ref()
            .and_then(|contract| contract.methods.iter().find(|m| m.name == command))
            .is_some_and(|method| method.coalesce)
    }

    /// Move a live surface between shell chrome and a window.
    ///
    /// Everything above the presentation layer survives: the component runtime,
    /// its Lua VM, retained tree, and service subscriptions are all kept, and
    /// only the compositor object is swapped (`PresentationEngine::configure`
    /// destroys and recreates it when the role in the config differs). What this
    /// function owns is the shell-side bookkeeping that would otherwise describe
    /// the surface that no longer exists: the cached surface config, the cached
    /// size, and any popovers parented to the old object.
    pub(in crate::shell) fn set_surface_role(
        &mut self,
        surface_id: SurfaceId,
        role: mesh_core_wayland::SurfaceRole,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        self.invalidate_debug_snapshot_cache();
        let Some(index) = self.component_index_for_surface(&surface_id) else {
            tracing::warn!(%surface_id, "cannot change surface role: no such surface");
            return Ok(VecDeque::new());
        };

        let current_role = self.components[index].component.surface_role();
        if current_role == role {
            return Ok(VecDeque::new());
        }

        // Opt-in: a component laid out as a 32px-tall panel widget is not
        // automatically a sensible window, so the author declares that both
        // roles were designed for.
        if !mesh_core_surface_config::surface_role_change_allowed(
            current_role,
            role,
            self.components[index].component.surface_promotable(),
        ) {
            tracing::warn!(
                %surface_id,
                "cannot change surface role: surface does not declare mesh.surface.promotable"
            );
            return Ok(VecDeque::new());
        }

        // A promoted `<popover>` is an xdg_popup positioned against its parent;
        // it has no independent placement to become a window's, and demoting it
        // would leave the trigger pointing at a destroyed surface.
        if self.components[index].parent.popup_parent_surface.is_some() {
            tracing::warn!(
                %surface_id,
                "cannot change surface role: surface is realized as a popover"
            );
            return Ok(VecDeque::new());
        }

        // Checked before anything is torn down: window creation failure happens
        // inside `configure`, after the old layer surface is already gone, which
        // would leave the surface unmapped with no way back.
        if role == mesh_core_wayland::SurfaceRole::Window
            && !self.presentation_engine.window_role_supported()
        {
            tracing::warn!(
                %surface_id,
                "cannot promote surface to a window: the compositor does not expose xdg_wm_base"
            );
            return Ok(VecDeque::new());
        }

        tracing::info!(%surface_id, ?role, "changing surface role");

        // Tear the old compositor object down *now*, not lazily inside the next
        // `configure`. `configure` would swap it too, but it runs partway through
        // the render frame — after the loop has already read
        // `window_configured_size` and laid content out against it. Destroying
        // here makes that query report `None` from this point on, so the frame
        // that installs the new role sizes it like a first-ever show (unmeasured
        // configure, then the corrective retry once paint has built the tree)
        // instead of measuring against the size the *other* role was given. A
        // demotion left to the lazy path lays out against the old window's size,
        // finds its measurement already agrees with it, skips the corrective
        // configure, and strands the new layer surface at the 1x1 the layer-shell
        // backend clamped its 0x0 request to.
        //
        // `destroy_surface` also drops every popup parented to it; the shell's own
        // child bookkeeping is dropped alongside so a reopened popover is created
        // against the new object.
        self.destroy_all_child_surfaces(index);
        self.presentation_engine.destroy_surface(&surface_id);

        // Keyboard interactivity is a layer-shell request that a toplevel has no
        // equivalent for — it is focused by activation instead. Drop any override
        // the old role accumulated so the new one starts from its manifest value.
        if role == mesh_core_wayland::SurfaceRole::Window {
            self.components[index]
                .component
                .set_keyboard_mode_override(None);
            self.transfer_owned_keyboard_modes.remove(&surface_id);
            if self.keyboard_focus_surface.as_deref() == Some(surface_id.as_str()) {
                self.keyboard_focus_surface = None;
            }
        }

        self.components[index].component.surface_role_changed(role);

        // `last_surface_config` describes the destroyed object, and
        // `known_surface_size` is the size it was configured at. Left in place,
        // the render loop would compare against them and skip the configure that
        // creates the replacement. Sizing also inverts with the role, so the new
        // surface must go through a first-configure pass rather than inherit a
        // size measured under the old one.
        let target = &mut self.components[index].parent;
        target.last_surface_config = None;
        target.known_surface_size = None;
        target.force_full_present = true;

        Ok(VecDeque::new())
    }

    pub(in crate::shell) fn set_child_surface_role(
        &mut self,
        surface_id: SurfaceId,
        node_key: String,
        role: mesh_core_wayland::SurfaceRole,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        if node_key.is_empty() || node_key == "root" {
            tracing::warn!(%surface_id, %node_key, "cannot change the role of the surface root as a widget");
            return Ok(VecDeque::new());
        }
        let Some(index) = self.component_index_for_surface(&surface_id) else {
            tracing::warn!(%surface_id, "cannot change embedded widget role: no such surface");
            return Ok(VecDeque::new());
        };
        if role == mesh_core_wayland::SurfaceRole::Window
            && !self.presentation_engine.window_role_supported()
        {
            tracing::warn!(%surface_id, %node_key, "cannot promote embedded widget: the compositor does not expose xdg_wm_base");
            return Ok(VecDeque::new());
        }

        let promoted = role == mesh_core_wayland::SurfaceRole::Window;
        if !self.components[index]
            .component
            .set_child_surface_promoted(&node_key, promoted)
        {
            return Ok(VecDeque::new());
        }

        // A role switch destroys the old child compositor object before the
        // next parent frame requests its replacement. The component VM and
        // retained node are deliberately untouched.
        if let Some(child_index) = self.components[index]
            .children
            .iter()
            .position(|child| child.node_key == node_key)
        {
            self.destroy_child_surface_at(index, child_index);
        }
        Ok(VecDeque::new())
    }

    fn set_surface_visibility(
        &mut self,
        surface_id: SurfaceId,
        visible: bool,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        tracing::info!("set_surface_visibility surface_id={surface_id} visible={visible}");
        let current_visible = self
            .core
            .surfaces
            .get(&surface_id)
            .map(|state| state.visible)
            .or_else(|| {
                self.surfaces
                    .get(&surface_id)
                    .map(|surface| surface.visible)
            })
            .unwrap_or(false);
        let already_closing = self
            .core
            .surfaces
            .get(&surface_id)
            .and_then(|state| state.closing_until)
            .is_some();
        let pending_popover_hide = self.pending_popover_hides.contains_key(&surface_id);

        // Visibility commands are often produced by two independent paths in
        // one interaction: a direct shell event and a retained portal update.
        // Re-applying ShowSurface to an already visible surface restarts its
        // entrance state, which makes the surface visibly pop twice. Likewise,
        // a second HideSurface must not skip the exit transition and tear down
        // a surface that is already closing. Treat both cases as idempotent.
        if visible && current_visible && !already_closing && !pending_popover_hide {
            return Ok(VecDeque::new());
        }
        if !visible && already_closing {
            return Ok(VecDeque::new());
        }
        if !visible && !current_visible {
            return Ok(VecDeque::new());
        }

        if visible {
            self.pending_popover_hides.remove(&surface_id);
            if let Some(state) = self.core.surfaces.get_mut(&surface_id) {
                state.closing_until = None;
            }
            if let Some(runtime) = self
                .components
                .iter_mut()
                .find(|runtime| runtime.surface_id == surface_id.as_str())
            {
                runtime.component.set_surface_exiting(false);
            }
            return self.set_surface_visibility_now(surface_id, true);
        }

        let hide_transition = self
            .components
            .iter()
            .find(|runtime| runtime.surface_id == surface_id.as_str())
            .map(|runtime| runtime.component.hide_transition_ms())
            .unwrap_or(0);
        let is_visible = self
            .core
            .surfaces
            .get(&surface_id)
            .map(|state| state.visible)
            .unwrap_or_else(|| {
                self.surfaces
                    .get(&surface_id)
                    .map(|surface| surface.visible)
                    .unwrap_or(true)
            });
        if hide_transition > 0 && is_visible && !already_closing {
            let until =
                std::time::Instant::now() + std::time::Duration::from_millis(hide_transition);
            self.core
                .surfaces
                .entry(surface_id.clone())
                .and_modify(|state| {
                    state.visible = true;
                    state.closing_until = Some(until);
                })
                .or_insert(SurfaceState {
                    visible: true,
                    closing_until: Some(until),
                });
            if self.keyboard_focus_surface.as_deref() == Some(surface_id.as_str()) {
                self.keyboard_focus_surface = None;
            }
            if let Some(runtime) = self
                .components
                .iter_mut()
                .find(|runtime| runtime.surface_id == surface_id.as_str())
            {
                runtime.component.set_keyboard_mode_override(None);
                runtime.component.set_surface_exiting(true);
            }
            if let Some(previous_mode) = self.transfer_owned_keyboard_modes.remove(&surface_id) {
                if let Some(surface) = self.surfaces.get_mut(&surface_id) {
                    surface.keyboard_mode = previous_mode;
                }
            }
            return Ok(VecDeque::new());
        }

        self.set_surface_visibility_now(surface_id, false)
    }

    fn sibling_popover_hides(
        &self,
        surface_id: &str,
        trigger_surface: &str,
    ) -> VecDeque<CoreRequest> {
        if trigger_surface.is_empty() {
            return VecDeque::new();
        }
        self.components
            .iter()
            .filter(|runtime| {
                runtime.surface_id != surface_id
                    && runtime.parent.popup_parent_surface.as_deref() == Some(trigger_surface)
                    && self.surface_is_effectively_visible(runtime.surface_id.as_str())
            })
            .map(|runtime| CoreRequest::HidePopover {
                surface_id: runtime.surface_id.clone(),
                defer_for_hover_bridge: false,
            })
            .collect()
    }

    pub(in crate::shell) fn hide_popover(
        &mut self,
        surface_id: SurfaceId,
        defer_for_hover_bridge: bool,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        if let Some((index, crate::shell::types::TargetRef::Child(child_index))) =
            self.component_target_for_surface(&surface_id)
            && self.components[index].children[child_index].kind
                == crate::shell::types::ChildSurfaceKind::Overflow
        {
            // Overflow is derived from current geometry and has no authored
            // open state for HidePopover to mutate.
            self.pending_popover_hides.remove(&surface_id);
            return Ok(VecDeque::new());
        }
        if defer_for_hover_bridge && self.surface_is_promoted_popover(&surface_id) {
            self.pending_popover_hides.insert(
                surface_id.clone(),
                std::time::Instant::now() + POPOVER_HOVER_BRIDGE_DELAY,
            );
            if let Some(state) = self.core.surfaces.get_mut(&surface_id) {
                state.visible = true;
                state.closing_until = None;
            }
            if let Some(index) = self.component_index_for_surface(&surface_id) {
                self.components[index].component.set_surface_exiting(false);
            }
            return Ok(VecDeque::new());
        }

        self.pending_popover_hides.remove(&surface_id);
        self.set_surface_visibility(surface_id, false)
    }

    pub(in crate::shell) fn cancel_pending_popover_hide(&mut self, surface_id: &str) -> bool {
        let cancelled = self.pending_popover_hides.remove(surface_id).is_some();
        if cancelled {
            if let Some(state) = self.core.surfaces.get_mut(surface_id) {
                state.closing_until = None;
                state.visible = true;
            }
            // Only clear surface_exiting on top-level promoted surfaces; in-tree child
            // surfaces don't have their own ComponentRuntime so calling set_surface_exiting
            // on the parent would incorrectly affect the parent surface's animation state.
            let target = self.component_target_for_surface(surface_id);
            if let Some((index, crate::shell::types::TargetRef::Parent)) = target {
                self.components[index].component.set_surface_exiting(false);
            }
        }
        cancelled
    }

    pub(in crate::shell) fn defer_child_popover_hides_for_parent(
        &mut self,
        parent_surface_id: &str,
    ) {
        let Some(index) = self.component_index_for_surface(parent_surface_id) else {
            return;
        };
        let hide_at = std::time::Instant::now() + POPOVER_HOVER_BRIDGE_DELAY;
        let child_surface_ids: Vec<_> = self.components[index]
            .children
            .iter()
            .filter(|child| {
                child.kind == crate::shell::types::ChildSurfaceKind::Popover
                    && child.target.popup_parent_surface.as_deref() == Some(parent_surface_id)
            })
            .map(|child| child.target.surface_id.clone())
            .collect();
        for surface_id in child_surface_ids {
            self.pending_popover_hides.insert(surface_id, hide_at);
        }
    }

    pub(in crate::shell) fn cancel_pending_child_popover_hides_at(
        &mut self,
        parent_surface_id: &str,
        x: f32,
        y: f32,
    ) {
        let Some(index) = self.component_index_for_surface(parent_surface_id) else {
            return;
        };
        let child_surface_ids: Vec<_> = self.components[index]
            .children
            .iter()
            .filter(|child| {
                child.kind == crate::shell::types::ChildSurfaceKind::Popover
                    && child.target.popup_parent_surface.as_deref() == Some(parent_surface_id)
                    && point_in_rect(x, y, child.anchor_rect)
            })
            .map(|child| child.target.surface_id.clone())
            .collect();
        for surface_id in child_surface_ids {
            self.cancel_pending_popover_hide(&surface_id);
        }
    }

    fn child_popover_pointer_leave_requests(
        &mut self,
        surface_id: &str,
    ) -> Result<Option<VecDeque<CoreRequest>>, ShellRunError> {
        let Some((index, crate::shell::types::TargetRef::Child(child_index))) =
            self.component_target_for_surface(surface_id)
        else {
            return Ok(None);
        };
        if self.component_is_quarantined(index) {
            return Ok(None);
        }
        if self.components[index].children[child_index].kind
            != crate::shell::types::ChildSurfaceKind::Popover
            || self.components[index].children[child_index]
                .target
                .popup_parent_surface
                .is_none()
        {
            return Ok(None);
        }

        let node_key = self.components[index].children[child_index]
            .node_key
            .clone();
        let content_padding = self.components[index].children[child_index].content_padding;
        let target_surface_size = self.components[index].children[child_index]
            .target
            .known_surface_size
            .or_else(|| {
                self.components[index].children[child_index]
                    .target
                    .paint_buffer
                    .as_ref()
                    .map(|buffer| (buffer.width().max(1), buffer.height().max(1)))
            })
            .or_else(|| self.presentation_engine.surface_size_if_known(surface_id))
            .unwrap_or((1, 1));
        let component_surface_size = self.components[index]
            .parent
            .known_surface_size
            .or_else(|| {
                self.surfaces
                    .get(&self.components[index].surface_id)
                    .map(|surface| (surface.width.max(1), surface.height.max(1)))
            })
            .unwrap_or(target_surface_size);
        self.components[index]
            .target_mut(crate::shell::types::TargetRef::Child(child_index))
            .known_surface_size = Some(target_surface_size);

        let emitted = match self.components[index].component.handle_child_surface_input(
            &node_key,
            self.theme.active(),
            component_surface_size.0,
            component_surface_size.1,
            (content_padding.0 as f32, content_padding.1 as f32),
            ComponentInput::PointerLeave,
        ) {
            Ok(emitted) => emitted,
            Err(error) => {
                self.contain_component_failure(index, "callback", &error);
                return Ok(None);
            }
        };
        Ok(Some(VecDeque::from(emitted)))
    }

    fn surface_is_promoted_popover(&mut self, surface_id: &str) -> bool {
        let Some((index, target)) = self.component_target_for_surface(surface_id) else {
            return false;
        };
        match target {
            crate::shell::types::TargetRef::Parent => {
                self.components[index].parent.popup_parent_surface.is_some()
            }
            crate::shell::types::TargetRef::Child(child_index) => {
                self.components[index].children[child_index].kind
                    == crate::shell::types::ChildSurfaceKind::Popover
                    && self.components[index].children[child_index]
                        .target
                        .popup_parent_surface
                        .is_some()
            }
        }
    }

    pub(in crate::shell) fn set_surface_visibility_now(
        &mut self,
        surface_id: SurfaceId,
        visible: bool,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        self.invalidate_debug_snapshot_cache();
        if !visible {
            let is_window = self
                .surfaces
                .get(&surface_id)
                .is_some_and(|surface| surface.role == mesh_core_wayland::SurfaceRole::Window);
            if let Some(index) = self
                .components
                .iter()
                .position(|runtime| runtime.surface_id == surface_id.as_str())
            {
                {
                    let runtime = &mut self.components[index];
                    runtime.component.set_keyboard_mode_override(None);
                    runtime.component.set_surface_exiting(false);
                    if runtime.parent.popup_parent_surface.is_some() {
                        // Tear down the xdg_popup surface so it can be recreated
                        // fresh on the next ActivatePopover call.
                        runtime.parent.popup_parent_surface = None;
                        runtime.parent.popover_relationship = None;
                        runtime.parent.popup_config = None;
                        runtime.parent.last_popup_size = None;
                        runtime.component.set_popup_promoted(false);
                        self.presentation_engine.destroy_popup(&surface_id);
                        tracing::info!(
                            "set_surface_visibility_now: destroyed xdg_popup for {surface_id}"
                        );
                    }
                }
                self.destroy_all_child_surfaces(index);
                if is_window {
                    // A hidden xdg_toplevel is destroyed rather than detached.
                    // Invalidate the shell-side description of that object at
                    // the same boundary so the next show must configure and
                    // fully present a replacement.
                    self.presentation_engine.destroy_surface(&surface_id);
                    let target = &mut self.components[index].parent;
                    target.last_surface_config = None;
                    target.known_surface_size = None;
                    target.force_full_present = true;
                }
            }
            if let Some(previous_mode) = self.transfer_owned_keyboard_modes.remove(&surface_id) {
                if let Some(surface) = self.surfaces.get_mut(&surface_id) {
                    surface.keyboard_mode = previous_mode;
                }
            }
        }
        if surface_id == DEBUG_INSPECTOR_SURFACE_ID {
            self.debug.enabled = visible;
        }
        self.core
            .surfaces
            .entry(surface_id.clone())
            .and_modify(|state| {
                state.visible = visible;
                state.closing_until = None;
            })
            .or_insert(SurfaceState {
                visible,
                closing_until: None,
            });
        if !visible && self.keyboard_focus_surface.as_deref() == Some(surface_id.as_str()) {
            self.keyboard_focus_surface = None;
        }

        self.broadcast_core_event(CoreEvent::SurfaceVisibilityChanged {
            surface_id,
            visible,
        })
    }

    pub(in crate::shell) fn complete_due_surface_transitions(
        &mut self,
    ) -> Result<VecDeque<CoreRequest>, ShellRunError> {
        let now = std::time::Instant::now();
        let due_popovers: Vec<_> = self
            .pending_popover_hides
            .iter()
            .filter_map(|(surface_id, hide_at)| (*hide_at <= now).then(|| surface_id.clone()))
            .collect();
        let due: Vec<_> = self
            .core
            .surfaces
            .iter()
            .filter_map(|(surface_id, state)| {
                state
                    .closing_until
                    .is_some_and(|until| until <= now)
                    .then(|| surface_id.clone())
            })
            .collect();
        let mut emitted = VecDeque::new();
        for surface_id in due_popovers {
            self.pending_popover_hides.remove(&surface_id);
            if let Some(requests) = self.child_popover_pointer_leave_requests(&surface_id)? {
                emitted.extend(requests);
            } else {
                emitted.extend(self.set_surface_visibility(surface_id, false)?);
            }
        }
        for surface_id in due {
            emitted.extend(self.set_surface_visibility_now(surface_id, false)?);
        }
        Ok(emitted)
    }
}

/// Translate a command on one of the remaining shell-owned configuration
/// interfaces into the core request that performs it. Debug, theme, and locale
/// providers use [`CoreServiceRegistry`] so this compatibility adapter stays
/// limited to the older settings/package/composition seam.
///
/// Returns `None` for every interface a backend module owns, which is what
/// routes those to the ordinary command queue instead. Argument extraction is
/// strict: a command whose payload does not match the declared contract yields
/// `None` and is reported as an unsupported command rather than applied with a
/// silently defaulted value.
fn core_service_request(
    interface: &str,
    command: &str,
    payload: &serde_json::Value,
) -> Option<CoreRequest> {
    let text = |key: &str| {
        payload
            .get(key)
            .and_then(|value| value.as_str())
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string)
    };
    let optional_text = |key: &str| {
        payload
            .get(key)
            .and_then(|value| value.as_str())
            .map(str::to_string)
    };

    match (interface, command) {
        ("mesh.settings", "set_prop") => Some(CoreRequest::SetModuleProp {
            module_id: text("module_id")?,
            instance_id: optional_text("instance_id"),
            prop: text("prop")?,
            value: payload.get("value")?.clone(),
        }),
        ("mesh.settings", "unset_prop") => Some(CoreRequest::UnsetModuleProp {
            module_id: text("module_id")?,
            instance_id: optional_text("instance_id"),
            prop: text("prop")?,
        }),
        ("mesh.packages", "set_module_enabled") => Some(CoreRequest::SetModuleEnabled {
            module_id: text("module_id")?,
            enabled: payload.get("enabled")?.as_bool()?,
        }),
        ("mesh.packages", "install") => Some(CoreRequest::InstallModule {
            source: text("source")?,
            profile_id: optional_text("profile_id").filter(|value| !value.trim().is_empty()),
            available_only: payload
                .get("available_only")
                .and_then(|value| value.as_bool())
                .unwrap_or(false),
            allow_elevated: payload
                .get("allow_elevated")
                .and_then(|value| value.as_bool())
                .unwrap_or(false),
            allow_high: payload
                .get("allow_high")
                .and_then(|value| value.as_bool())
                .unwrap_or(false),
        }),
        ("mesh.packages", "uninstall") => Some(CoreRequest::UninstallModule {
            module_id: text("module_id")?,
            force: payload
                .get("force")
                .and_then(|value| value.as_bool())
                .unwrap_or(false),
        }),
        ("mesh.packages", "set_provider") => Some(CoreRequest::SetProvider {
            interface: text("interface")?,
            provider_id: text("provider_id")?,
        }),
        ("mesh.packages", "switch_profile") => Some(CoreRequest::SwitchProfile {
            profile_id: text("profile_id")?,
        }),
        ("mesh.composition", "apply_node_slot") => Some(CoreRequest::ApplyNodeSlot {
            profile_id: text("profile_id")?,
            root_instance: text("root_instance")?,
            slot: text("slot")?,
            nodes: payload.get("nodes")?.clone(),
            expected_generation: text("expected_generation")?,
        }),
        ("mesh.composition", "reset_node_slot") => Some(CoreRequest::ResetNodeSlot {
            profile_id: text("profile_id")?,
            root_instance: text("root_instance")?,
            slot: text("slot")?,
            expected_generation: text("expected_generation")?,
        }),
        _ => None,
    }
}

fn request_changes_debug_snapshot(request: &CoreRequest) -> bool {
    !matches!(
        request,
        CoreRequest::PositionSurface { .. }
            | CoreRequest::PublishDiagnostics { .. }
            | CoreRequest::WriteClipboard { .. }
            | CoreRequest::OpenDebugSource { .. }
            | CoreRequest::SetChildSurfaceRole { .. }
            | CoreRequest::TransferTabFocus { .. }
            | CoreRequest::Shutdown
    )
}

fn profiling_trigger_for_request(request: &CoreRequest) -> &'static str {
    match request {
        CoreRequest::PositionSurface { .. } => "position_surface",
        CoreRequest::ToggleSurface { .. } => "toggle_surface",
        CoreRequest::SetSurfaceRole { .. } => "set_surface_role",
        CoreRequest::ToggleSurfaceRole { .. } => "toggle_surface_role",
        CoreRequest::SetChildSurfaceRole { .. } => "set_child_surface_role",
        CoreRequest::ShowSurface { .. } => "show_surface",
        CoreRequest::HideSurface { .. } => "hide_surface",
        CoreRequest::HidePopover { .. } => "hide_popover",
        CoreRequest::PublishDiagnostics { .. } => "publish_diagnostics",
        CoreRequest::ServiceCommand { .. } => "service_command",
        CoreRequest::ServiceCall { .. } => "service_call",
        CoreRequest::CancelServiceCall { .. } => "cancel_service_call",
        CoreRequest::WriteClipboard { .. } => "write_clipboard",
        CoreRequest::SetTheme { .. } => "set_theme",
        CoreRequest::SetThemeMode { .. } => "set_mode",
        CoreRequest::SetIconTheme { .. } => "set_icon_theme",
        CoreRequest::SetFontFamily { .. } => "set_font_family",
        CoreRequest::SetLocale { .. } => "set_locale",
        CoreRequest::SetProvider { .. } => "set_provider",
        CoreRequest::SetModuleEnabled { .. } => "set_module_enabled",
        CoreRequest::InstallModule { .. } => "install_module",
        CoreRequest::UninstallModule { .. } => "uninstall_module",
        CoreRequest::SetModuleProp { .. } => "set_module_prop",
        CoreRequest::ApplyNodeSlot { .. } => "apply_node_slot",
        CoreRequest::ResetNodeSlot { .. } => "reset_node_slot",
        CoreRequest::UnsetModuleProp { .. } => "unset_module_prop",
        CoreRequest::SwitchProfile { .. } => "switch_profile",
        CoreRequest::ActivatePopover { .. } => "activate_popover",
        CoreRequest::TransferTabFocus { .. } => "transfer_tab_focus",
        CoreRequest::ToggleDebugOverlay => "toggle_debug_overlay",
        CoreRequest::ToggleDebugLayoutBounds => "toggle_debug_layout_bounds",
        CoreRequest::ToggleDebugElementPicker => "toggle_debug_element_picker",
        CoreRequest::OpenDebugSource { .. } => "open_debug_source",
        CoreRequest::ToggleDebugProfiling => "toggle_debug_profiling",
        CoreRequest::RunDebugBenchmark { .. } => "run_debug_benchmark",
        CoreRequest::CycleDebugTab => "cycle_debug_tab",
        CoreRequest::Shutdown => "shutdown",
    }
}

fn point_in_rect(x: f32, y: f32, rect: (i32, i32, i32, i32)) -> bool {
    let (left, top, width, height) = rect;
    let right = left.saturating_add(width.max(0));
    let bottom = top.saturating_add(height.max(0));
    x >= left as f32 && x < right as f32 && y >= top as f32 && y < bottom as f32
}

fn benchmark_scenario_id(scenario_id: &str) -> Option<BenchmarkScenarioId> {
    match scenario_id {
        "idle" => Some(BenchmarkScenarioId::Idle),
        "hover" => Some(BenchmarkScenarioId::Hover),
        "surface_open_close" => Some(BenchmarkScenarioId::SurfaceOpenClose),
        "pointer_update" => Some(BenchmarkScenarioId::PointerUpdate),
        "text_update" => Some(BenchmarkScenarioId::TextUpdate),
        "scroll" => Some(BenchmarkScenarioId::Scroll),
        "icon_grid" => Some(BenchmarkScenarioId::IconGrid),
        "animation" => Some(BenchmarkScenarioId::Animation),
        "theme_reload" => Some(BenchmarkScenarioId::ThemeReload),
        "resize" => Some(BenchmarkScenarioId::Resize),
        "keyboard_traversal" => Some(BenchmarkScenarioId::KeyboardTraversal),
        "backend_update" => Some(BenchmarkScenarioId::BackendUpdate),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composition_slot_commands_become_typed_requests() {
        let apply = core_service_request(
            "mesh.composition",
            "apply_node_slot",
            &serde_json::json!({
                "profile_id": "desk",
                "root_instance": "@mesh/navigation-bar#default",
                "slot": "end",
                "nodes": [{"id":"clock","use":"@mesh/navigation-bar:clock","props":{}}],
                "expected_generation": "abc",
            }),
        )
        .unwrap();
        assert!(matches!(
            apply,
            CoreRequest::ApplyNodeSlot {
                profile_id,
                root_instance,
                slot,
                expected_generation,
                ..
            } if profile_id == "desk"
                && root_instance == "@mesh/navigation-bar#default"
                && slot == "end"
                && expected_generation == "abc"
        ));
    }

    #[test]
    fn package_install_and_uninstall_commands_become_typed_requests() {
        let install = core_service_request(
            "mesh.packages",
            "install",
            &serde_json::json!({
                "source": "/tmp/example-module",
                "profile_id": "desktop",
                "available_only": true,
                "allow_elevated": true,
                "allow_high": false,
            }),
        )
        .expect("install should be a core-provided package method");
        assert!(matches!(
            install,
            CoreRequest::InstallModule {
                source,
                profile_id: Some(profile_id),
                available_only: true,
                allow_elevated: true,
                allow_high: false,
            } if source == "/tmp/example-module" && profile_id == "desktop"
        ));

        let uninstall = core_service_request(
            "mesh.packages",
            "uninstall",
            &serde_json::json!({
                "module_id": "@example/module",
                "force": true,
            }),
        )
        .expect("uninstall should be a core-provided package method");
        assert!(matches!(
            uninstall,
            CoreRequest::UninstallModule { module_id, force }
                if module_id == "@example/module" && force
        ));
    }

    fn scheduler_source(module_id: &str, runtime_id: &str) -> EffectSource {
        EffectSource {
            module_id: module_id.to_string(),
            runtime_id: runtime_id.to_string(),
            generation: 4,
        }
    }

    fn position_request(surface_id: &str) -> CoreRequest {
        CoreRequest::PositionSurface {
            surface_id: surface_id.to_string(),
            margin_top: 0,
            margin_left: 0,
        }
    }

    #[test]
    fn effect_scheduler_round_robins_sources() {
        let mut scheduler = EffectScheduler::default();
        let source_a = scheduler_source("@test/a", "a");
        let source_b = scheduler_source("@test/b", "b");
        scheduler.enqueue_batch([
            (position_request("a-1"), source_a.clone()),
            (position_request("a-2"), source_a.clone()),
            (position_request("a-3"), source_a),
            (position_request("b-1"), source_b.clone()),
            (position_request("b-2"), source_b.clone()),
            (position_request("b-3"), source_b),
        ]);
        scheduler.begin_frame();

        let order = (0..6)
            .map(|_| scheduler.next_effect().unwrap().context.source.module_id)
            .collect::<Vec<_>>();

        assert_eq!(
            order,
            vec![
                "@test/a", "@test/b", "@test/a", "@test/b", "@test/a", "@test/b"
            ]
        );
        assert_eq!(scheduler.finish_frame().deferred, 0);
    }

    #[test]
    fn effect_scheduler_defers_residual_work_after_source_budget() {
        let mut scheduler = EffectScheduler::default();
        let source_a = scheduler_source("@test/a", "a");
        let source_b = scheduler_source("@test/b", "b");
        scheduler.enqueue_batch(
            (0..100)
                .map(|index| (position_request(&format!("a-{index}")), source_a.clone()))
                .chain(
                    (0..100)
                        .map(|index| (position_request(&format!("b-{index}")), source_b.clone())),
                ),
        );
        scheduler.begin_frame();

        let mut processed = 0;
        while scheduler.next_effect().is_some() {
            processed += 1;
        }
        let report = scheduler.finish_frame();

        assert_eq!(processed, MAX_EFFECTS_PER_SOURCE_PER_FRAME * 2);
        assert_eq!(report.deferred, 200 - processed);
        assert_eq!(scheduler.pending_len(), report.deferred);
    }

    #[test]
    fn effect_scheduler_breaks_repeated_causal_effects() {
        let mut scheduler = EffectScheduler::default();
        let source = scheduler_source("@test/cycle", "cycle");
        scheduler.enqueue_batch([(position_request("cycle"), source)]);
        scheduler.begin_frame();

        while let Some(effect) = scheduler.next_effect() {
            scheduler.set_active_context(effect.context.clone());
            scheduler.enqueue_followups(std::iter::once(effect.request));
            scheduler.clear_active_context();
        }
        let report = scheduler.finish_frame();

        assert_eq!(report.cycle_breaks, 1);
        assert_eq!(scheduler.pending_len(), 0);
    }
}
