# Section 15 — Shell core/orchestration

## Scope and coverage

Read-only scan. No implementation changes, builds, tests, live settings mutations or benchmarks were performed. Existing uncommitted changes were inspected as pending work. This section report records source evidence and explicit coverage limits; the whole-codebase audit is unfinished.


## Process tree

discovery/profile/settings input -> revisioned candidates and workers -> authoritative commit -> service publication/component notification -> scheduled effects -> serial root preparation -> rendering/presentation -> watcher/recovery/shutdown. Durable settings and profile activation have different authority/lifetime boundaries.

S15-LOGIC-21 / PERF: IPC completed client task records accumulate until shutdown.
Confidence: confirmed source behavior. New specific open item by backlog search.
Full read of ipc.rs: spawn_ipc_server creates JoinSet at :85; select loop only joins tasks in shutdown arm (:87–92). Every accepted connection spawns into it (:93–109), and there is no join_next/try_join_next during normal service. Completed task records remain resident, growing with short-lived IPC connections. In-flight clients also have no concurrency limit or read deadline; handle_ipc_client (:158–208) awaits commands indefinitely. The 4,096-byte command bound is real and useful but does not bound client count or completed JoinSet entries.
Recommendation: reap completions in normal select loop and define bounded concurrent clients/read-idle policy. Do not conflate completed-task metadata retention with retained active connection buffers. Benchmark 10k/100k short connections, JoinSet length/RSS and held-open clients; regress typed profile acknowledgements and shutdown cancellation. The owner-only socket is not evidence this can never exhaust resources under ordinary repeated automation.

Settings latency revalidation (existing audit overlap, no new timing claim): watcher/profile paths previously caused full authoring snapshot loads/hashes on each generation-only event; the current local diff retains paths and skips/defer known settings writes. The source still does broad graph loads for unknown, retired or module events. load_authoring_snapshot_retaining hashes every installed tree, and graph load remains synchronous before no-op comparison. Existing S02-PERF-003 and appearance-latency backlog cover broader revision/content-index work; do not mark all graph-reload optimization finished because of the local settings routing fix.
Also verified prepare_control_plane_settings reconstructs/validates graph schemas per request via discovery::register_graph_settings_schemas (:1444–1501). It builds a collection then replaces schemas transactionally ONCE; do not falsely claim repeated per-schema whole-store validation. Reuse by graph revision is unmeasured. commit_control_plane_batch applies settings to every root and appearance invalidation cold-resets retained trees, already recorded in section10. The healthy-watcher two-second maintenance interval is recovery; filesystem events and worker completions wake the loop. No source evidence establishes a fixed four-second delay.
Redundancy: no new safe shell API removal demonstrated. Shared policy projections, service authorities and orchestration adapters are real boundaries; do not remove them merely for wrapping lower-level calls. Large portions of activation/shutdown/service routing tests and production bodies remain unreviewed.

## Review layers and existing overlap

Performance, redundancy and correctness were assessed on the inspected seams above. Unmeasured work is distinguished from measured cost. Source-only hypotheses do not establish the reported four-second Settings delay. Existing broader backlog items remain open; current local implementation edits must not be counted as independently verified completed findings.

## Refuted suspicions

No fixed four-second settings debounce was identified. Unknown filesystem events retain conservative reconciliation. Caller-specific caveats and refuted or withheld suspicions are stated above; unavailable target behavior is not classified as a shipped defect.

## Tests and benchmarks needed

Run each finding's proposed deterministic regression and representative repeated release workload before changing code or claiming performance gains. None was run during this read-only scan.

## File coverage

## November 7 backend lifecycle continuation

### S15-LOGIC-22 — Successful supervised restarts erase failure and quarantine history

`backend/supervision.rs:69–126` counts failed cycles and intends to reset them only after sixty seconds of healthy uptime. Restart handling spawns a pending candidate (`:281–286`); readiness reaches `lifecycle.rs::finish_backend_runtime_switch`, which unconditionally removes the entire supervision entry (`:963`) before noting the new Running state. Every successfully initialized restart therefore loses its prior count immediately. Repeated short-lived runs can keep receiving the first restart delay without reaching quarantine. Successful failover also erases the session's quarantined-provider set.

**Confidence: confirmed control flow; no lifecycle reproduction.** Preserve failure/quarantine history through supervised readiness and distinguish explicit user selection from automatic recovery. Exercise four fail/restart/ready/fail cycles below healthy uptime through the real pending readiness path, then verify increasing backoff, quarantine and exclusion after failover. The existing quarantine test injects `replace_backend_runtime` directly and bypasses the offending completion path.

### S15-LOGIC-23 — Initialization failure of a supervised replacement ends recovery

The pending candidate branch in `lifecycle.rs:1135–1166` retires InitFailed/Failed/Stopped candidates and returns without scheduling another supervised cycle. A restart is staged as a pending candidate too. If it fails before readiness, the recovery chain stops instead of exhausting its bounded retry/failover policy. Keeping an older healthy runtime during a failed user-requested switch remains a distinct, valid rollback case.

**Confidence: confirmed missing recovery transition; no live reproduction.** Test an active provider failure followed by a replacement start failure, with generation-checked further recovery and eventual bounded failover. Do not infer that every initial load failure must retry.

### S15-PERF-21 — The bounded backend queue drains into an unbounded shell inbox

`backend/spawn.rs:193–265` consumes the bounded backend event queue and sends payload-bearing updates/results/events into `UnboundedSender<ShellMessage>`. `runtime/mod.rs:510` creates that inbox; `:597–606` drains a bounded number per frame and only then coalesces updates. Producer bursts during main-thread stalls can accumulate snapshots downstream even while the upstream queue and per-callback output budgets are respected. Ordered lifecycle/result/event barriers must remain intact.

**Confidence: confirmed queue topology; residency and latency unmeasured.** Bound ingress through the final consumer or combine revisioned latest-state slots with an ordered bounded event/result channel. Measure 1k/10k/100k producer bursts during controlled consumer pauses, recording occupancy, bytes/RSS, input latency and terminal result delivery. This locates a surviving seam within broader runtime resource-broker work; it does not establish the settings delay.

### S15-PERF-22 — Retired runtime and restart-timer task records accumulate

`lifecycle.rs:630–640` appends retired service/bridge task wrappers to `retiring_backend_runtimes`; normal service does not reap them. Shutdown drains the collection. `supervision.rs:163` similarly retains completed restart-timer handles until shutdown. Finished tasks are not assumed to retain their whole former runtime, but wrapper/task-record allocations grow with historical generations rather than active providers.

**Confidence: confirmed collection lifetime; memory cost unmeasured.** Reap completed records while preserving graceful stop, durable flush and bounded shutdown. Exercise 1,000 provider generations and timer cycles, asserting bookkeeping returns to live plus actively retiring work; measure retained records and RSS. This is separate from IPC client JoinSet retention.

### Coverage and refutations

### S15-LOGIC-24 — Bound-state publication discards settled hidden-component requests

`runtime/service_state.rs::publish_bound_service_state` (`:334`) ignores the result of `deliver_service_event`. Delivery (`:680–703`) settles hidden components' effects and returns their sourced requests. The actual frontend host's `settle_effects_offscreen` runs effects and drains their side channels, so those requests are consumed rather than left pending. `runtime/request.rs:2453` uses this publication for optimistic state-binding writes; failure rollback uses it too. Later identical provider confirmation may deduplicate the state and does not guarantee another effect transition.

**Confidence: confirmed discarded returned effects with shipped command caller; no UI reproduction.** Route requests through the same origin/generation-checked queue as ordinary service publication. Regress a real hidden component whose effect reads audio percent and requests showing a surface, then exercise successful state-binding commands, rollback and equal provider confirmation. Preserve exactly-once applicable actions and stale-effect rejection. Historical dropped theme/locale effects concerned another publication path; this is the current bound-state seam.

### Further settings preparation evidence and coverage

`surface_layout.rs:209–246` opens and parses inactive theme default CSS while rebuilding the theme catalog, contrary to its comment about opening inactive sources only when selected. This is further source evidence for existing theme-preparation work, not a separate measured explanation of the settings delay. Reuse must follow source identity and validation obligations.

Six further assigned files were fully read: reload, package, service_state, wayland, surface_layout and replay_hooks. Together this agent has 21 full reads; the earlier IPC full read is additional. Remaining profile, main runtime and request coverage stays partial. No source changes or execution checks were performed.

This pass fully read fifteen assigned files: the five backend orchestration files, shell manifests/lib, core provider, module configuration, services, signals/sounds and three integration files. The latter are test-only modules and were not reported as production hot paths. No new safe API removal or module-domain ownership violation was demonstrated. Shell runtime/request/profile callers remain partial. No repository source changes, tests, builds or benchmarks were performed.

Earlier combined checkpoint, superseded by the November 7 reads above: across sections 13–16, 166 assigned files had 10 full reads, 22 partial reads and 134 unreviewed. The [inventory](../00-coverage.md) contains current cumulative status; the checkpoint counts must not be added to later passes.
