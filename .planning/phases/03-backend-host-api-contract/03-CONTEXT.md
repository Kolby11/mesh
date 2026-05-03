# Phase 3: Backend Host API Contract - Context

**Gathered:** 2026-05-03
**Status:** Ready for planning

<domain>
## Phase Boundary

This phase stabilizes the backend Luau host APIs needed by MVP service plugins: structured command execution, plugin settings access, plugin-scoped logging, and poll interval control. Existing backend lifecycle from Phase 2 is already deterministic; Phase 3 should tighten the public author-facing contract and update existing providers/tests to match it.

This phase does not implement service provider contracts, generic service command response plumbing, reference backend proof docs, package download/install, sandboxing, or LSP/tooling. It should not add service-specific Rust branches.

</domain>

<decisions>
## Implementation Decisions

### Structured Command Execution
- **D-01:** `mesh.exec` should be strict structured-only. The public contract is `mesh.exec(program, args)` where `program` is the executable name/path and `args` is a Luau array/table of arguments.
- **D-02:** Phase 3 should remove or reject the legacy single-string splitting form such as `mesh.exec("program arg1 arg2")`. Documentation and tests must not promote the compatibility form.
- **D-03:** `mesh.exec` always returns a result table instead of throwing for process failures. Plugin scripts branch on `result.success`.
- **D-04:** Spawn failures return `{ success = false, stdout = "", stderr = "...", code = nil }`. Non-zero process exits also return normally with `success = false`, captured stdout/stderr, and the exit code when available.

### Shell Execution
- **D-05:** `mesh.exec_shell` is not part of the MVP host API. Phase 3 should remove it from the public backend API and migrate bundled providers away from shell-string pipelines.
- **D-06:** Shell-style parsing/pipelines should move into Luau logic using structured `mesh.exec` calls. Dynamic values must be passed as arguments rather than interpolated into shell strings.
- **D-07:** Planner/researcher should decide the safest compatibility path for existing bundled providers, but the end-state contract for Phase 3 is "keep only `mesh.exec`" as the documented backend command execution primitive.

### Plugin Settings
- **D-08:** `mesh.config()` is the Phase 3 public config contract. It returns the backend plugin's full settings table.
- **D-09:** Do not add `mesh.config.get(...)`, `mesh.config.get_all()`, or path lookup helpers in this phase.

### Logging
- **D-10:** `mesh.log` supports the fixed levels `debug`, `info`, `warn`, and `error`.
- **D-11:** Invalid log levels must not crash backend scripts. They should be logged as warnings so plugin authors can see the mistake.
- **D-12:** Both log call styles are public: `mesh.log("info", "message")` and named methods such as `mesh.log.info("message")`, `mesh.log.warn("message")`, `mesh.log.error("message")`, and `mesh.log.debug("message")`.

### Poll Interval Control
- **D-13:** `mesh.service.set_poll_interval(ms)` should clamp to sane bounds. Values below `50ms` become `50ms`.
- **D-14:** When the runtime clamps an interval, it should emit a warning so plugin authors can see the correction.
- **D-15:** Poll interval changes take effect after the current callback returns. This applies to changes made during `init()`, `on_poll()`, and command handlers; callbacks should not be interrupted mid-call.

### the agent's Discretion
- Planner/researcher may choose whether `mesh.exec` rejects malformed args by returning `success=false` or by raising a Luau argument/type error, as long as process spawn and exit failures return result tables.
- Planner/researcher may choose the exact warning text and tracing/diagnostic integration for invalid log levels and clamped poll intervals.
- Planner/researcher may choose whether to cap very large poll intervals if existing runtime constraints justify it, but the minimum bound and warning behavior are locked.
- Planner/researcher should decide the smallest safe migration path for bundled providers currently using `mesh.exec_shell`, while preserving the no-service-specific-Rust rule.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Milestone and Phase Scope
- `.planning/PROJECT.md` — Active v1.1 milestone intent, Phase 2 completion state, and locked architectural decisions.
- `.planning/REQUIREMENTS.md` — BHOST-01 through BHOST-05 requirements and traceability.
- `.planning/ROADMAP.md` — Phase 3 goal, dependencies, and success criteria.
- `.planning/phases/01-plugin-package-manifest-foundation/01-CONTEXT.md` — Package/module naming, installed graph, and source-of-truth decisions that remain active.
- `.planning/phases/02-backend-lifecycle-foundation/02-CONTEXT.md` — Locked lifecycle decisions Phase 3 builds on.
- `.planning/phases/02-backend-lifecycle-foundation/02-VERIFICATION.md` — Verified lifecycle behavior and residual manual risk.

### Existing Codebase Maps
- `.planning/codebase/STACK.md` — Rust/Luau/Tokio/mlua stack constraints.
- `.planning/codebase/ARCHITECTURE.md` — Shell/runtime/service layering, backend event flow, and no-service-logic-in-core rule.
- `.planning/codebase/INTEGRATIONS.md` — Existing bundled backend providers and current system tool usage.

### Backend Host API Code
- `crates/core/runtime/scripting/src/backend.rs` — Current `BackendScriptContext`, `mesh.exec`, `mesh.exec_shell`, `mesh.config`, `mesh.log`, `mesh.service.set_poll_interval`, and host API tests.
- `crates/core/runtime/backend/src/lib.rs` — Runtime poll loop, interval refresh timing, lifecycle events, command dispatch, and poll failure threshold.
- `crates/core/runtime/scripting/src/host_api.rs` — Shared host API documentation/comments that may need reconciliation with the backend public contract.
- `crates/core/shell/src/shell/mod.rs` — Backend runtime startup, settings injection, runtime status, and command channel ownership from Phase 2.

### Bundled Backend Providers to Migrate
- `packages/plugins/backend/core/pipewire-audio/src/main.luau` — Uses `mesh.exec_shell` for `wpctl` pipelines and string-interpolated commands; should migrate to structured `mesh.exec` plus Luau parsing where feasible.
- `packages/plugins/backend/core/pulseaudio-audio/src/main.luau` — Uses `mesh.exec_shell` for `pactl` commands; should migrate to structured `mesh.exec`.
- `packages/plugins/backend/core/networkmanager-network/src/main.luau` — Uses `mesh.exec_shell` for `nmcli` and Bluetooth query commands; likely the largest migration target.
- `packages/plugins/backend/core/upower-power/src/main.luau` — Uses a shell pipeline/awk snippet; planner should decide whether to rewrite parsing in Luau after structured command output.
- `packages/plugins/backend/core/mpris-media/src/main.luau` — Simple provider using logging/poll interval; useful for host API smoke coverage.
- `packages/plugins/backend/core/shell-theme/src/main.luau` — Simple config/polling-oriented backend; useful for non-exec host API coverage.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `BackendScriptContext` already injects most Phase 3 APIs in `crates/core/runtime/scripting/src/backend.rs`.
- `ExecOutcome` and `exec_outcome_to_lua()` already produce the desired result table shape for `mesh.exec` and can be tightened rather than replaced.
- Phase 2 `spawn_backend_service()` already refreshes interval timing after `init()`, `on_poll()`, and command handlers, matching the locked "after current callback" behavior.
- Existing backend tests in `backend.rs` cover config, exec, exec_shell, logging, poll interval, and bundled host API presence; these should be updated into contract tests.
- Bundled Luau providers are practical migration fixtures for proving structured `mesh.exec` can replace shell strings without adding Rust service logic.

### Established Patterns
- Rust core remains a generic wiring layer. It must not call `wpctl`, `pactl`, `nmcli`, `upower`, or branch on service names.
- Backend service behavior belongs in Luau providers under `packages/plugins/backend/core/**/src/main.luau`.
- Backend scripts communicate with the runtime through `BackendScriptContext` and service emissions; host API failures should be visible but should not crash the shell process.
- Phase 2 lifecycle decisions still apply: no automatic fallback provider, no lazy activation, no stale runtime slots, and lifecycle failures are visible.

### Integration Points
- `install_host_api()` in `BackendScriptContext` is the direct implementation point for the public backend host API.
- `run_exec()` should enforce structured args and remove/reject the single-string compatibility path.
- `run_exec_shell()` and `mesh.exec_shell` registration should be removed or quarantined behind a non-public compatibility path if planner finds a staged migration is necessary.
- `bounded_poll_interval_ms()` and `refresh_interval()` in the backend runtime are likely integration points for minimum interval clamping and warning behavior.
- Existing bundled provider scripts are the proof surface for removing shell-string command execution from the MVP API.

</code_context>

<specifics>
## Specific Ideas

- The user explicitly chose strict structured `mesh.exec` because parsing can be done in Luau.
- `mesh.exec_shell` should not remain as a public MVP API; structured command execution is the only documented process primitive.
- Plugin config should stay simple in Phase 3: `mesh.config()` returns the whole settings table.
- Logging should be forgiving for bad levels but visible through warnings.
- Poll interval clamping should warn plugin authors when a requested value is corrected.

</specifics>

<deferred>
## Deferred Ideas

- `mesh.config.get(...)` or path-based config helper APIs.
- Public shell-pipeline API for advanced providers.
- Sandboxing, command allowlists, or permission UI for process execution.
- Service command response/failure propagation to callers; that belongs to the later Service Provider Contract phase.
- Backend MVP reference plugin and author-facing documentation; that belongs to the later Diagnostics and MVP Proof phase.

</deferred>

---

*Phase: 3-Backend Host API Contract*
*Context gathered: 2026-05-03*
