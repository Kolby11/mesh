---
gsd_state_version: 1.0
milestone: v1.3
milestone_name: Performance Instrumentation and Responsiveness
status: ready_to_plan
stopped_at: Completed 17-04-PLAN.md
last_updated: "2026-05-09T08:30:30.420Z"
last_activity: 2026-05-09
progress:
  total_phases: 5
  completed_phases: 5
  total_plans: 16
  completed_plans: 16
  percent: 100
---

# State: MESH v1.3

## Project Reference

See: `.planning/PROJECT.md` (updated 2026-05-08)

**Core value:** MESH should let plugin authors build distinctive shell UI and service integrations while the shell stays observable, deterministic, and responsive on real interaction paths.
**Current focus:** Phase 17 — canonical-benchmark-scenarios-and-proof-flows

## Current Position

Phase: 18
Plan: Not started
Status: Ready to plan
Last activity: 2026-05-09

## Decisions

- Backend plugins use Luau for service logic; Rust core remains the wiring layer.
- `require('@mesh/service')` is the frontend/backend interface.
- Runtime correctness is in scope before LSP, distribution, or new surfaces.
- Phase numbering reset to 1 for v1.1 after archiving v1.0 planning artifacts.
- [v1.1]: Backend plugin MVP starts with a central package.json-like installed-plugin manifest that drives frontend/backend plugin installation, backend category/provider selection, and later runtime lifecycle.
- [v1.1 Phase 02]: Backend startup uses explicit installed-module graph active providers before runtime launch; legacy priority discovery is compatibility fallback only when the graph cannot load.
- [v1.1 Phase 02]: Backend runtime failures emit visible lifecycle status and diagnostics, but do not automatically switch to a fallback provider.
- [v1.1 Phase 02]: Shell owns backend runtime slots by interface and removes service command handlers before replacement or terminal cleanup.
- [Phase 02]: Service proxies are state-and-command surfaces only; callback-style bind/on_change APIs were removed from the public proxy path.
- [Phase 02]: Service update invalidation is based on tracked top-level field value changes, not whole-service emissions.
- [Phase 02]: Lookup diagnostics are recorded before InterfaceUnavailable or CapabilityDenied errors are returned, so pcall changes control flow without hiding visibility.
- [Phase 03]: Plan 02 handler failures are reported through component diagnostics handles and return non-fatal empty request lists.
- [Phase 03]: Plan 02 switch and checkbox state is tracked in shell input state so on_change receives a typed boolean.
- [Phase 03]: Plan 03 proof lives in the shipped navigation-bar volume widget with a typed onchange slider and audio:set_volume command path.
- [Phase 04]: Audio set_volume payload normalization remains in Luau providers; Rust core only verifies generic proxy publication and backend dispatch. — Preserves the Phase 4 architectural rule that service-specific command behavior stays out of Rust core.
- [Phase 04]: Bundled audio providers preserve legacy percent payload compatibility while accepting normalized volume payloads. — Keeps existing command callers working while quick settings moves to the finalized proxy payload shape.
- [Phase 04]: Quick settings audio uses the finalized direct proxy call `audio.set_volume("default", normalized)` for slider changes.
- [Phase 04]: Quick settings Wi-Fi rows remain guarded and display-only when provider data lacks a non-empty network id.
- [Phase 04]: Unavailable and permission-denied states are visible in quick settings while technical details stay in logs and diagnostics.
- [Phase 04]: The top panel remains a compact status and entry surface; direct service controls stay in quick settings.
- [Phase 04]: Final surface regressions use shipped panel source plus focused command snippets to prove callback-free proxy behavior.
- [Phase 04]: Frontend docs show service mutations through named proxy methods instead of legacy service event channels.
- [Phase 04]: Service proxy command methods require `service.<name>.control`; read capability remains state-only.
- [Phase 04]: Shell surface transitions use `shell.toggle-surface` and `shell.hide-surface` with `surface_id`, not quick-settings-specific shell event names.
- [Phase 03]: Plan 01 addresses BHOST-02 by removing mesh.exec_shell from the public backend MVP host API surface. — Phase 03 context explicitly overrides shell execution as non-MVP and requires structured command execution only.
- [Phase 03]: Plan 01 locks backend process execution to strict mesh.exec(program, args); legacy single-string splitting is removed. — Prevents unintended argv tokenization and matches Phase 03 D-01/D-02.
- [Phase 03]: Plan 03 keeps mesh.config() as the only backend config API and returns full plugin settings. — Matches D-08/D-09 and avoids premature config lookup helpers.
- [Phase 03]: Plan 03 locks backend log public levels to debug, info, warn, and error across both call styles. — The warning level remains only as an undocumented compatibility alias.
- [Phase 03]: Plan 03 keeps invalid backend log levels non-fatal and visible through warnings. — Matches D-11 so plugin author mistakes do not crash backend scripts.
- [Phase 03]: Bundled providers issue system commands only through structured mesh.exec(program, args). — Keeps the Phase 03 backend host API strict and removes provider dependency on exec_shell.
- [Phase 03]: Shell pipeline parsing for PipeWire and UPower lives in Luau provider code, not Rust core. — Preserves the no service-specific Rust command behavior rule.
- [Phase 03]: Plan 04 locks mesh.service.set_poll_interval(ms) to a 50ms minimum with plugin-scoped warnings and post-callback runtime refresh. — Covers BHOST-05 and D-13 through D-15.
- [Phase 04]: Plan 05 derives shell-theme backend settings from ThemeEngine.active().id so provider startup and restart match the shell's resolved theme authority.
- [Phase 04]: Plan 05 makes theme file-watch reload return pending CoreRequest queues and synchronize mesh.theme only when the resolved active theme id changes.
- [Phase 09]: Disabled pseudo state is derived from disabled and aria-disabled attributes during runtime annotation.
- [Phase 09]: Focus-visible remains mapped to focused state until a keyboard modality source exists.
- [Phase 10]: The first release is explicit opt-in selectable text only, limited to a single selectable text node with wrapped-line support inside that node.
- [Phase 10]: Interactive control labels, clipped or ellipsized text, and nested cross-node selection are deferred beyond the first Phase 10 release.
- [Phase 10]: Selection colors are shell/theme-owned through dedicated `color.selection-background` and `color.selection-foreground` tokens.
- [Phase 10]: Standard copy behavior routes through explicit `Ctrl+C` handling only when a Phase 10 selection exists.
- [v1.3]: Profiling is debug-only and should extend the existing debug overlay/debug command path instead of adding end-user settings.
- [v1.3]: Performance acceptance is based on canonical shipped interaction benchmarks, not qualitative impressions or synthetic aggregate metrics alone.
- [v1.3]: The first profiling inspector is live and rolling, not a full trace capture/replay system.
- [Phase 17]: Benchmark rows derive from live rolling profiling snapshots only; no history, trace export, or persistence was added.
- [Phase 17]: Profiling-disabled benchmark rows remain visible but inert with Profiling off status and profiling payload stays null.
- [Phase 17]: Benchmark launch requests accept only the five canonical scenario ids and report unknown ids through non-fatal diagnostics.
- [Phase 17]: Surface open/close benchmark launch reuses normal ShowSurface and HideSurface requests for @mesh/audio-popover.
- [Phase 17]: Benchmark launch requests record session-local debug state but never toggle debug profiling.
- [Phase 17]: Benchmark view rows consume primitive normalized props from the inspector parent rather than reading mesh.debug directly. — Matches existing debug inspector parent-owned payload normalization and keeps child view render-only.
- [Phase 17]: Benchmark action buttons always publish shell.run-debug-benchmark with fixed canonical scenario ids. — Preserves debug-scoped run routing from Plan 17-02 and avoids inspector-local scenario aliases.
- [Phase 17]: Backend-driven benchmark completion requires both provider-stage timing and frontend surface render timing. — Satisfies BACK-03/BENCH-05 using generic profiling snapshots only.
- [Phase 17]: Backend benchmark target text is derived from existing profiling/runtime identities while preserving the canonical mesh.audio -> @mesh/pipewire-audio fallback. — Keeps Rust benchmark correlation generic and avoids audio payload parsing.
- [Phase 17]: Task 17-04-03 is recorded with an empty verification commit because the final proof suite passed without code changes. — Preserves per-task atomic commit history without changing unrelated files.

## Performance Metrics

| Plan | Duration | Tasks | Files |
|------|----------|-------|-------|
| Phase 02 P01 | 7min | 3 tasks | 4 files |
| Phase 03 P02 | 9min | 3 tasks | 7 files |
| Phase 03 P03 | 5min | 3 tasks | 4 files |
| Phase 04 P01 | 3min | 3 tasks | 5 files |
| Phase 04 P02 | 6min | 3 tasks | 5 files |
| Phase 04 P03 | 4min | 3 tasks | 2 files |
| Phase 04 P04 | 12min | 3 tasks | 6 files |
| Phase 04 P05 | 4min | 3 tasks | 3 files |
| Phase 04 P06 | 4min | 3 tasks | 4 files |
| Phase 01 P01 | 35min | 3 tasks | 2 files |
| Phase 01 P02 | 30min | 3 tasks | 1 files |
| Phase 01 P03 | 25min | 3 tasks | 12 files |
| Phase 03 P01 | 4min | 3 tasks | 2 files |
| Phase 03 P03 | 4min | 3 tasks | 3 files |
| Phase 03 P02 | 6min | 3 tasks | 5 files |
| Phase 03 P04 | 6min | 3 tasks | 4 files |
| Phase 04 P05 | 4min | 3 tasks | 2 files |
| Phase 08 P01 | 8min | 3 tasks | 2 files |
| Phase 08 P02 | 10min | 3 tasks | 1 files |
| Phase 08 P03 | 12min | 3 tasks | 3 files |
| Phase 08 P04 | 10min | 3 tasks | 2 files |
| Phase 08 P05 | 9min | 3 tasks | 4 files |
| Phase 09 P01 | 5min | 2 tasks | 2 files |
| Phase 10 P01 | 8 min | 2 tasks | 9 files |
| Phase 10 P02 | 9 min | 2 tasks | 4 files |
| Phase 17 P01 | 6min | 3 tasks | 3 files |
| Phase 17 P02 | 6min | 3 tasks | 7 files |
| Phase 17 P03 | 9min | 3 tasks | 3 files |
| Phase 17 P04 | 7min | 3 tasks | 2 files |

## Session

Last session: 2026-05-09T08:30:30.411Z
Stopped At: Completed 17-04-PLAN.md
Resume File: None

## Accumulated Context

### Roadmap Evolution

- v1.0 planning artifacts archived to `.planning/milestones/v1.0-reset-2026-05-03-*`.
- v1.1 reset roadmap focuses on backend plugin MVP stability.

## Deferred Items

Items accepted at `v1.1` close:

| Category | Item | Status |
|----------|------|--------|
| validation | Phase 01-05 Nyquist metadata remains partial rather than finalized | deferred |
| validation | Live PipeWire or PulseAudio backend startup remains manual-only confirmation | deferred |
| verification | Obsolete `latest_service_events` note still needs retirement from archived validation metadata | deferred |

Items acknowledged and deferred at `v1.2` close on 2026-05-08:

| Category | Item | Status |
|----------|------|--------|
| todo | 2026-05-08-create-unified-package-and-module-manifest-phase.md | deferred |
| uat | Phase 11 / 11-HUMAN-UAT.md | partial |
| verification | Phase 11 / 11-VERIFICATION.md | human_needed |
| planning | v1.2 requirements checkbox and traceability drift for KEY-01 through KEY-04 and ANIM-01 through ANIM-05 despite completed phases | deferred |

## Blockers

(None)

## Pending Todos

- Create unified package and module manifest phase — captured 2026-05-08; separate future phase for package/module manifest structure, module management, icon pack installation, and interface declarations.

## Artifact Index

| Artifact | Path |
|----------|------|
| Project context | `.planning/PROJECT.md` |
| Active requirements | `.planning/REQUIREMENTS.md` |
| Roadmap | `.planning/ROADMAP.md` |
| State | `.planning/STATE.md` |
| Research | `.planning/research/v1.3-performance-instrumentation-and-responsiveness.md` |
| Codebase map | `.planning/codebase/` |

---
*State updated: 2026-05-08 after starting milestone v1.3*
