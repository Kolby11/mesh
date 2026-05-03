# MESH - Backend Plugin MVP

## What This Is

MESH is a Rust-based, Wayland-native shell framework whose service behavior is intended to live in Luau backend plugins. This milestone resets the active roadmap around one practical objective: make backend plugins work as an MVP foundation before expanding more frontend, documentation, or package-distribution work.

The core value: **a backend plugin author can write a Luau service plugin, configure it, run it under the shell, emit state, handle commands, and understand failures without reading Rust source or relying on fragile special cases.**

## The Problem

The previous milestone stabilized several frontend and surface paths, but backend plugin behavior is still the critical foundation. MESH needs a stable backend runtime before higher-level shell features can be trusted.

The current risk is that backend plugins may appear to work in narrow cases while basic concepts remain underspecified:

1. Plugin lifecycle needs to be predictable: load, initialize, poll, command dispatch, shutdown, and restart.
2. Luau host APIs need a stable MVP contract: command execution, config access, logging, service emission, and poll interval control.
3. Service contracts need to connect backend providers to shell/frontend consumers without per-service Rust branches.
4. Backend failures need clear diagnostics instead of silent drops or shell-level instability.

## Current Milestone: v1.1 Backend Plugin MVP

**Goal:** Make backend plugins stable enough for MVP: core backend concepts work predictably, plugins can run service logic, emit state, receive config, log, execute host commands, expose service contracts, and surface failures clearly.

**Target features:**
- Backend plugin lifecycle: discovery, load, init, poll, command handling, stop/restart.
- Backend Luau host APIs: `mesh.exec`, `mesh.exec_shell`, `mesh.config`, `mesh.log`, `mesh.service.emit`, `mesh.service.set_poll_interval`.
- Service provider contracts: backend plugins declare provided services, state shape, and command handlers.
- Runtime diagnostics: init/poll/command failures degrade plugin health and remain visible.
- MVP proof plugin: one fresh backend service plugin proves the documented backend contract.

## Goals

### Primary

- Make backend plugin lifecycle behavior deterministic and testable.
- Lock the MVP backend Luau API contract.
- Route backend service emissions and commands through generic contracts, not service-specific Rust logic.
- Ensure backend errors produce actionable diagnostics while keeping the shell alive where possible.

### Secondary

- Provide a minimal reference backend plugin that exercises the MVP contract.
- Leave frontend documentation, LSP support, and plugin distribution for later milestones unless needed for backend proof.

### Out of Scope

- Frontend UI polish and new shell surfaces.
- Package download, signing, sandboxing, or marketplace flows.
- Full scripting API documentation beyond backend MVP notes/proofs.
- LSP completions/hover.

## Success Criteria

Done when:

1. A fresh backend plugin can be discovered, loaded, initialized, and polled by the shell.
2. The plugin can read config, log, execute allowed commands, emit service state, and change its poll interval.
3. A service command can be routed to the backend plugin and return visible success/failure behavior.
4. Invalid manifests, missing entrypoints, Luau init failures, poll failures, and command failures are reported through diagnostics.
5. The backend MVP is proven by automated tests and a reference backend plugin.

## Scope

**In scope - backend runtime:**
- Manifest handling for backend service plugins.
- Backend Luau VM setup and plugin-scoped host context.
- Init/poll/command lifecycle.
- Stop/restart behavior enough to avoid duplicate stale tasks.

**In scope - service contracts:**
- Interface/provider declaration validation.
- Latest emitted state association with the provider.
- Command handler routing from service command requests to backend Luau functions.

**In scope - observability:**
- Plugin-scoped diagnostics.
- Structured logs from backend scripts.
- Clear test coverage around failure modes.

## Audience

Backend plugin authors and core MESH maintainers. The MVP should be understandable from manifests, examples, and tests without spelunking through Rust wiring.

## Constraints

- Build and test must work within the Nix dev shell (`nix develop`).
- Rust core remains a wiring layer; service-specific behavior belongs in Luau backend plugins.
- Existing frontend/surface work from the previous milestone remains archived, not discarded.

## Current State

The previous v1.0 planning artifacts were archived on 2026-05-03 under `.planning/milestones/v1.0-reset-2026-05-03-*` before this roadmap reset. Active planning now starts fresh at Phase 1 for backend plugin MVP stabilization.

## Requirements

See `.planning/REQUIREMENTS.md` for the active v1.1 requirement set.

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Backend plugins use Luau for service logic | Keeps Rust core as wiring and makes services extensible | Locked |
| Rust core must stay generic across services | Prevents audio/network/power special cases from becoming architecture | Locked |
| Backend MVP comes before distribution and LSP | Runtime stability is prerequisite for tooling and package workflows | Decided this milestone |
| Reset active roadmap numbering for v1.1 | User explicitly chose reset roadmap after archiving prior artifacts | Locked |

## Evolution

This document evolves at phase transitions and milestone boundaries.

**After each phase transition**:
1. Requirements invalidated? Move to Out of Scope with reason.
2. Requirements validated? Move to validated with phase reference.
3. New requirements emerged? Add to Active.
4. Decisions to log? Add to Key Decisions.
5. "What This Is" still accurate? Update if drifted.

**After each milestone**:
1. Full review of all sections.
2. Core Value check: still the right priority?
3. Audit Out of Scope: reasons still valid?
4. Update Current State with validated outcomes.

---
*Last updated: 2026-05-03 after v1.1 reset*
