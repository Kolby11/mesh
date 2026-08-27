use super::super::*;
use super::candidates::launch_candidate_for_provider_with_capabilities;
use rustix::fd::BorrowedFd;
use std::collections::HashSet;
use std::time::{Duration, Instant};

/// First restart delay; each consecutive failed cycle doubles it.
const RESTART_BASE_DELAY: Duration = Duration::from_secs(1);
/// Upper bound for the exponential restart backoff.
const RESTART_MAX_DELAY: Duration = Duration::from_secs(60);
/// Failed restart cycles tolerated before the provider is quarantined and the
/// supervisor fails over to the next-priority provider.
const MAX_RESTART_CYCLES: u32 = 3;
/// A provider running this long is considered healthy; its failure history
/// resets so an old crash does not count against a much later one.
const HEALTHY_UPTIME: Duration = Duration::from_secs(60);

/// Per-interface supervision state for backend provider runtimes.
#[derive(Debug, Default)]
pub(in crate::shell) struct BackendSupervisionState {
    /// Consecutive failed run cycles of the currently supervised provider.
    pub(in crate::shell) restart_count: u32,
    /// A restart wake-up has been scheduled and not yet handled.
    pub(in crate::shell) restart_pending: bool,
    /// Provider identity and monotonic token for the pending wake-up. A
    /// delayed task is not allowed to resurrect a provider after a graph or
    /// runtime replacement has superseded it.
    pub(in crate::shell) pending_provider_id: Option<String>,
    pub(in crate::shell) pending_identity: mesh_core_backend::BackendIdentity,
    pub(in crate::shell) restart_generation: u64,
    /// When the current provider last reported `running`.
    pub(in crate::shell) running_since: Option<Instant>,
    /// Providers benched for this session after exhausting restart cycles.
    pub(in crate::shell) quarantined_providers: HashSet<String>,
}

impl BackendSupervisionState {
    pub(in crate::shell) fn invalidate_pending_restart(&mut self) {
        self.restart_pending = false;
        self.pending_provider_id = None;
        self.pending_identity = mesh_core_backend::BackendIdentity::default();
        self.restart_generation = self.restart_generation.saturating_add(1);
    }
}

/// Handles the shell loop needs to respawn backend runtimes outside the
/// startup path: the Tokio handle backends run on, the shell message sender,
/// and the eventfd that wakes the main loop.
#[derive(Clone)]
pub(in crate::shell) struct BackendRespawnContext {
    pub(in crate::shell) handle: tokio::runtime::Handle,
    pub(in crate::shell) tx: mpsc::UnboundedSender<ShellMessage>,
    pub(in crate::shell) eventfd_fd: std::os::unix::io::RawFd,
}

impl Shell {
    /// Record a healthy runtime start for supervision bookkeeping.
    pub(in crate::shell) fn note_backend_running(&mut self, interface: &str) {
        let state = self
            .backend_supervision
            .entry(interface.to_string())
            .or_default();
        state.running_since = Some(Instant::now());
    }

    /// React to a terminal failure of the interface's current provider:
    /// schedule a supervised restart with exponential backoff, or quarantine
    /// the provider and fail over once its restart budget is exhausted.
    pub(in crate::shell) fn supervise_backend_failure(
        &mut self,
        interface: &str,
        provider_id: &str,
    ) {
        let identity = self.backend_identity_for_interface(interface);
        self.supervise_backend_failure_at_identity(interface, provider_id, identity);
    }

    pub(in crate::shell) fn supervise_backend_failure_at_identity(
        &mut self,
        interface: &str,
        provider_id: &str,
        identity: mesh_core_backend::BackendIdentity,
    ) {
        let state = self
            .backend_supervision
            .entry(interface.to_string())
            .or_default();
        if state.restart_pending {
            return;
        }
        if state
            .running_since
            .take()
            .is_some_and(|since| since.elapsed() >= HEALTHY_UPTIME)
        {
            state.restart_count = 0;
        }

        if state.restart_count >= MAX_RESTART_CYCLES {
            state.quarantined_providers.insert(provider_id.to_string());
            state.restart_count = 0;
            let message = format!(
                "backend provider {provider_id} for {interface} failed {MAX_RESTART_CYCLES} supervised restarts; quarantined for this session, failing over"
            );
            tracing::warn!(interface, provider_id, "{message}");
            self.record_backend_runtime_status_at_identity(
                interface.to_string(),
                provider_id.to_string(),
                identity,
                BackendRuntimeStatus::Quarantined,
                message,
            );
            self.schedule_backend_restart_at_identity(
                interface,
                provider_id,
                identity,
                RESTART_BASE_DELAY,
            );
            return;
        }

        let delay = RESTART_BASE_DELAY
            .saturating_mul(1u32 << state.restart_count.min(6))
            .min(RESTART_MAX_DELAY);
        let state = self
            .backend_supervision
            .get_mut(interface)
            .expect("supervision state was just created");
        state.restart_count += 1;
        let attempt = state.restart_count;
        tracing::info!(
            interface,
            provider_id,
            attempt,
            delay_ms = delay.as_millis() as u64,
            "scheduling supervised backend restart"
        );
        self.schedule_backend_restart_at_identity(interface, provider_id, identity, delay);
    }

    fn schedule_backend_restart_at_identity(
        &mut self,
        interface: &str,
        provider_id: &str,
        identity: mesh_core_backend::BackendIdentity,
        delay: Duration,
    ) {
        if self.shutdown_started || self.shutdown_complete {
            return;
        }
        let Some(ctx) = self.backend_respawn.clone() else {
            tracing::debug!(interface, "no respawn context; skipping supervised restart");
            return;
        };
        let restart_generation = if let Some(state) = self.backend_supervision.get_mut(interface) {
            state.restart_pending = true;
            state.pending_provider_id = Some(provider_id.to_string());
            state.pending_identity = identity;
            state.restart_generation = state.restart_generation.saturating_add(1);
            state.restart_generation
        } else {
            return;
        };
        let interface = interface.to_string();
        let provider_id = provider_id.to_string();
        let task = ctx.handle.spawn(async move {
            tokio::time::sleep(delay).await;
            let _ = ctx.tx.send(ShellMessage::BackendRestartDue {
                interface,
                provider_id,
                identity,
                restart_generation,
            });
            let evfd = unsafe { BorrowedFd::borrow_raw(ctx.eventfd_fd) };
            let _ = rustix::io::write(&evfd, &1u64.to_ne_bytes());
        });
        self.backend_restart_tasks.push(task.abort_handle());
    }

    /// A supervised restart came due: pick the best non-quarantined provider
    /// for the interface (config-selected first, then priority order) and
    /// respawn it. With every provider quarantined, the interface stays down
    /// with a health record until the shell restarts or config changes.
    pub(in crate::shell) fn handle_backend_restart_due(
        &mut self,
        interface: &str,
        provider_id: &str,
        restart_generation: u64,
    ) {
        self.handle_backend_restart_due_at_identity(
            interface,
            provider_id,
            mesh_core_backend::BackendIdentity::default(),
            restart_generation,
        );
    }

    pub(in crate::shell) fn handle_backend_restart_due_at_identity(
        &mut self,
        interface: &str,
        provider_id: &str,
        identity: mesh_core_backend::BackendIdentity,
        restart_generation: u64,
    ) {
        let Some(state) = self.backend_supervision.get_mut(interface) else {
            return;
        };
        if !state.restart_pending
            || state.restart_generation != restart_generation
            || state.pending_provider_id.as_deref() != Some(provider_id)
            || (identity != mesh_core_backend::BackendIdentity::default()
                && state.pending_identity != identity)
            || (identity != mesh_core_backend::BackendIdentity::default()
                && self.activation_generation != identity.activation_generation)
        {
            tracing::debug!(
                interface,
                provider_id,
                restart_generation,
                "ignored stale backend restart deadline"
            );
            return;
        }
        state.restart_pending = false;
        state.pending_provider_id = None;
        state.pending_identity = mesh_core_backend::BackendIdentity::default();
        let Some(ctx) = self.backend_respawn.clone() else {
            return;
        };
        let graph = match self.load_installed_module_graph_cached() {
            Ok(graph) => graph.clone(),
            Err(err) => {
                tracing::warn!(
                    interface,
                    "supervised restart aborted; module graph unavailable: {err}"
                );
                return;
            }
        };
        let quarantined = self
            .backend_supervision
            .get(interface)
            .map(|state| state.quarantined_providers.clone())
            .unwrap_or_default();
        let provider = graph
            .active_provider(interface)
            .filter(|provider| !quarantined.contains(&provider.module_id))
            .or_else(|| {
                graph
                    .backend_providers_for_interface(interface)
                    .iter()
                    .find(|provider| !quarantined.contains(&provider.module_id))
            })
            .cloned();
        let Some(provider) = provider else {
            let message = format!(
                "all providers for {interface} are quarantined after repeated failures; interface is down until shell restart or provider change"
            );
            tracing::error!(interface, "{message}");
            self.record_backend_runtime_status(
                interface.to_string(),
                "<none>".to_string(),
                BackendRuntimeStatus::NoActiveProvider,
                message,
            );
            return;
        };

        match launch_candidate_for_provider_with_capabilities(
            &graph,
            &self.modules,
            &self.settings_store,
            &self.interfaces,
            &provider,
            Some(&self.effective_capabilities),
        ) {
            Ok(mut candidate) => {
                tracing::info!(
                    interface,
                    provider_id = %candidate.module_id,
                    "supervised backend restart"
                );
                self.apply_shell_runtime_settings(&mut candidate);
                self.spawn_backend_candidate(
                    &ctx.handle,
                    ctx.tx.clone(),
                    candidate,
                    ctx.eventfd_fd,
                );
            }
            Err(status) => {
                self.record_backend_runtime_status(
                    status.interface.clone(),
                    status
                        .provider_id
                        .clone()
                        .unwrap_or_else(|| "<none>".to_string()),
                    BackendRuntimeStatus::from_str(status.status),
                    status.message.clone(),
                );
                tracing::warn!(
                    interface = status.interface,
                    provider_id = status.provider_id.as_deref().unwrap_or("<none>"),
                    status = status.status,
                    "supervised restart failed: {}",
                    status.message
                );
            }
        }
    }
}
