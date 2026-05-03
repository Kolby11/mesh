# Phase 2: Backend Lifecycle Foundation - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-05-03
**Phase:** 2-Backend Lifecycle Foundation
**Areas discussed:** Runtime selection, Backend stability contract, Lifecycle control, Failure behavior

---

## Area Selection

| Option | Description | Selected |
|--------|-------------|----------|
| All three | Covers provider selection, lifecycle semantics, and failure behavior so planning has no major blind spots. | ✓ |
| Runtime selection | Focuses on graph-driven active provider choice, disabled modules, missing binaries, and fallback rules. | |
| Lifecycle control | Focuses on init, polling, command dispatch ordering, restart, and stale task shutdown semantics. | |

**User's choice:** All three.
**Notes:** The user chose to discuss all major lifecycle gray areas.

---

## Runtime Selection

| Option | Description | Selected |
|--------|-------------|----------|
| Fall back to next available provider | Keeps the shell functional when explicit provider fails, but may ignore user choice. | |
| Do not start any provider | Strictly honors explicit provider choice. | ✓ |
| You decide | Planner chooses based on existing fallback model. | |

**User's choice:** Do not start any provider if explicit provider cannot launch.
**Notes:** The user clarified that the ideal concept is no fallback service; fallback should not be treated as normal.

| Option | Description | Selected |
|--------|-------------|----------|
| Disabled modules invisible and unavailable | Disabled means no runtime; unmet requirements surface as diagnostics. | ✓ |
| Auto-enable if required | Frontend requirements can pull backend providers online automatically. | |
| You decide | Planner preserves a clear override model. | |

**User's choice:** Disabled backend modules are invisible to runtime creation.

| Option | Description | Selected |
|--------|-------------|----------|
| No implicit provider | Start nothing and surface no active provider selected. | ✓ |
| Auto-select if exactly one provider exists | Convenient when there is no ambiguity. | |
| Temporary legacy fallback only | Keep priority fallback internally for old fixtures. | |

**User's choice:** No implicit provider when none is configured.

---

## Backend Stability Contract

| Option | Description | Selected |
|--------|-------------|----------|
| Contract-first backend modules | Every backend module declares entrypoint and provided interfaces; runtime validates before launch. | ✓ |
| Core strict, custom loose | Built-ins are strict; custom modules launch with fewer declarations. | |
| Docs-guided only | Best practices live in docs/tests only. | |

**User's choice:** The user leaned toward contract-first but asked how a frontend module that needs custom backend functionality can guarantee users receive the same backend functionality.
**Notes:** Discussion clarified that custom functionality should flow through named interface requirements and provider declarations.

| Option | Description | Selected |
|--------|-------------|----------|
| Frontend declares required interfaces, backend provides them | Custom frontend depends on a named interface; backend provider supplies it. | ✓ |
| Frontend depends on exact backend module ID | Guarantees one implementation but weakens alternative providers. | |
| Frontend bundles backend entrypoint directly | Simple but weakens frontend/backend module boundary. | |

**User's choice:** Frontend declares required interfaces and backend provides them.

| Option | Description | Selected |
|--------|-------------|----------|
| Readiness gate before frontend activation | Frontend modules with unmet backend interfaces degrade/disable. | |
| Mount frontend with missing-service fallback | Frontend loads and handles proxy errors. | |
| Auto-install/auto-select provider | Future package manager behavior. | ✓ |

**User's choice:** Auto-install/auto-select was initially requested, then refined.
**Notes:** The discussion resolved that Phase 2 should not implement auto-install, but metadata should support future install/lazy activation flows.

| Option | Description | Selected |
|--------|-------------|----------|
| Dependency-driven lazy backend activation | Frontend can cause installed provider activation on demand. | |
| Interface-only lazy activation | Prompt/error unless active provider already selected. | |
| No lazy activation in Phase 2 | Validate only; defer lazy activation. | ✓ |

**User's choice:** No lazy activation in Phase 2.

---

## Lifecycle Control

| Option | Description | Selected |
|--------|-------------|----------|
| Allow init emission | `init()` may publish immediate boot state. | ✓ |
| Poll/command only | First state only comes from `on_poll()` or commands. | |
| You decide | Planner chooses based on runtime behavior. | |

**User's choice:** Allow `init()` emission.

| Option | Description | Selected |
|--------|-------------|----------|
| Accept after init | Commands can run once `init()` succeeds. | ✓ |
| Require first poll | Commands wait/fail until initial poll state exists. | |
| You decide | Planner chooses. | |

**User's choice:** Accept commands after successful `init()`.
**Notes:** User clarified that `init()` should load the initial state.

| Option | Description | Selected |
|--------|-------------|----------|
| Keep runtime alive with degraded health | Continue polling with rate-limited diagnostics. | |
| Stop runtime after threshold | Stop after repeated poll failures and mark provider failed. | ✓ |
| You decide | Planner chooses threshold and behavior. | |

**User's choice:** Stop runtime after a repeated poll failure threshold.

| Option | Description | Selected |
|--------|-------------|----------|
| No auto-restart yet | Stop cleanly and require explicit restart later. | ✓ |
| One bounded restart | Try once after failure threshold. | |
| You decide | Planner chooses. | |

**User's choice:** No automatic restart in Phase 2.

| Option | Description | Selected |
|--------|-------------|----------|
| No stale tasks or receivers | Shutdown closes poll loop and command receiver before replacement. | ✓ |
| Preserve latest state until replacement emits | Consumers keep old state during stop/restart. | |
| Fast restart | Minimize downtime over strict cleanup. | |

**User's choice:** No stale tasks or receivers is the top guarantee.

---

## Failure Behavior

| Option | Description | Selected |
|--------|-------------|----------|
| Diagnostics only | Developer-facing health record; frontend sees missing service. | |
| Diagnostics plus module status | Diagnostics and runtime/package state mark provider unavailable/failed. | ✓ |
| You decide | Planner chooses. | |

**User's choice:** Diagnostics plus module/provider runtime status.

| Option | Description | Selected |
|--------|-------------|----------|
| Stage-specific status | Distinguish invalid manifest, missing entrypoint, init failed, poll failed, stopped, etc. | ✓ |
| Simple failed/unavailable | Fewer states with less detail. | |
| You decide | Planner chooses. | |

**User's choice:** Stage-specific lifecycle status.

| Option | Description | Selected |
|--------|-------------|----------|
| Deduplicate by provider and stage | Repeated failures update count/timestamp instead of spamming. | ✓ |
| Log every failure | Maximum raw detail, but noisy. | |
| You decide | Planner chooses. | |

**User's choice:** Deduplicate diagnostics by provider and lifecycle stage.

---

## the agent's Discretion

- Planner may choose exact lifecycle status enum/type names.
- Planner may choose the repeated poll failure threshold.
- Planner may decide how to preserve legacy fallback compatibility in tests/fixtures without making it the product model.

## Deferred Ideas

- Lazy backend provider activation when a frontend module is enabled or mounted.
- Auto-installing backend providers for frontend module dependencies.
- Provider selection UI.
- Automatic restart/backoff after provider failure.
