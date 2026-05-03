# Phase 02: Backend Lifecycle Foundation - Pattern Map

**Mapped:** 2026-05-03
**Source:** `02-CONTEXT.md` and `02-RESEARCH.md`

## Target Files and Closest Analogs

| Target | Role | Closest Existing Analog | Pattern to Reuse |
|--------|------|-------------------------|------------------|
| `crates/core/shell/src/shell/mod.rs` | Shell startup, backend selection, runtime slot ownership | Existing `spawn_backend_plugins()`, `backend_plugin_settings_json()`, `binary_exists()`, shell tests near `installed_module_graph_exposes_shell_package_choices` | Keep shell as generic wiring; add small helper functions and focused unit tests instead of driving `Shell::run()`. |
| `crates/core/shell/src/shell/types.rs` | Shared shell message/status types | Existing `ShellMessage`, `ServiceCommandMsg`, `ServiceEvent` | Add lifecycle status/message types near current shell event contracts if shell needs to route runtime lifecycle events. |
| `crates/core/shell/src/shell/component.rs` | Current backend candidate struct location | `BackendServiceCandidate` | Either replace this with a graph-derived candidate or move candidate type closer to shell lifecycle helpers. |
| `crates/core/runtime/backend/src/lib.rs` | Async backend runtime loop | `spawn_backend_service()`, `BackendServiceCommand`, `BackendServiceUpdate`, interval tests | Keep Tokio channel pattern; add typed lifecycle events and tests using `tokio::time::timeout`. |
| `crates/core/runtime/scripting/src/backend.rs` | Luau script execution and host API | `BackendScriptContext`, `BackendScriptError`, tests for `MissingEntrypoint`, `run_poll`, `run_command` | Preserve real mlua execution; return typed errors from poll/command handlers where runtime needs lifecycle decisions. |
| `crates/core/extension/plugin/src/package.rs` | Installed module graph source of truth | `InstalledModuleGraph::active_provider`, `backend_providers_for_interface`, `unresolved_backend_requirements` | Use existing graph APIs; do not duplicate package parsing in shell. Add graph helpers only if resolver needs explicit active provider iteration. |
| `crates/core/foundation/diagnostics/src/lib.rs` | Health and dedupe behavior | `Diagnostics::record_handler_error`, `record_missing_icon` | Add lifecycle diagnostic dedupe by stable key if shell cannot express lifecycle failures with existing handles. |
| `config/package.json` and `config/modules/@mesh/*/package.json` | Package graph fixtures | Phase 1 repo-local package graph | Use active `mesh.audio` provider and alternative provider fixtures for resolver tests. |

## Concrete Code Patterns

### Candidate Derivation

Current shell selection groups all discovered backend plugins by service and chooses highest priority. Phase 2 should instead derive candidates from `InstalledModuleGraph::active_provider("mesh.audio")`.

Use the existing helper pattern in `spawn_backend_plugins()` for capabilities/settings/script reads, but move selection before channel creation and avoid `fallback_provider()` in normal graph-driven startup.

### Async Runtime Tests

Existing backend tests spawn the runtime with:

- `let (update_tx, mut update_rx) = mpsc::unbounded_channel();`
- `let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();`
- `tokio::spawn(spawn_backend_service(...))`
- `tokio::time::timeout(Duration::from_secs(1), update_rx.recv())`

Reuse this shape for init failure, poll failure threshold, interval refresh, and command receiver close behavior.

### Diagnostics Dedupe

Existing diagnostics dedupe stores `HashSet` keys in `DiagnosticsState`. A lifecycle equivalent should use a key with provider, stage, and message. If count/timestamp fields are added, keep tests exact:

- first record returns `true`
- repeated same stage/message returns `false` or increments count without increasing error count
- changed stage/message records a distinct item

## Data Flow to Preserve

1. Load package graph.
2. Determine explicit active provider per interface.
3. Validate backend module and entrypoint.
4. Create command channel only for validated provider.
5. Load script.
6. Run `init()` exactly once.
7. Start poll loop and accept commands.
8. Stop loop/receiver on failure, explicit stop, shell shutdown, or replacement.
9. Publish lifecycle status and service updates without service-specific Rust branches.

## Landmines

- Do not convert `mesh.audio` to `audio` too early. Interface identity and service display names are both used; tests should assert exact strings.
- Do not register `service_handlers[interface]` before validation and successful lifecycle start are represented.
- Do not use `fallback_provider()` for Phase 2 normal startup.
- Do not implement automatic restart or provider fallback on init/poll failure.
- Do not add audio/network/power-specific Rust behavior.
