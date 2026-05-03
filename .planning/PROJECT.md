# MESH - Backend Plugin MVP

## What This Is

MESH is a Rust-based, Wayland-native shell framework whose service behavior is intended to live in Luau backend plugins. This milestone resets the active roadmap around one practical objective: make backend plugins work as an MVP foundation before expanding more frontend, documentation, or package-distribution work.

The core value: **MESH has one shell-owned plugin package interface that declares installed frontend plugins, their backend dependencies, available backend providers by category, and the user's active provider choices; backend plugin authors can then write Luau services against a stable runtime contract.**

## The Problem

The previous milestone stabilized several frontend and surface paths, but backend plugin behavior is still the critical foundation. MESH needs a stable backend runtime before higher-level shell features can be trusted.

The current risk is that backend plugins may appear to work in narrow cases while basic concepts remain underspecified:

1. Plugin installation and activation need one central source of truth instead of scattered implicit discovery rules.
2. Frontend plugins need a way to declare underlying backend dependencies so installation can pull in the service providers they need.
3. Backend plugins need categories/services such as `audio`, `network`, or `shortcuts` so users can choose between multiple providers in the same category.
4. Plugin lifecycle needs to be predictable: load, initialize, poll, command dispatch, shutdown, and restart.
5. Luau host APIs need a stable MVP contract: command execution, config access, logging, service emission, and poll interval control.
6. Backend failures need clear diagnostics instead of silent drops or shell-level instability.

## Current Milestone: v1.1 Backend Plugin MVP

**Goal:** Make backend plugins stable enough for MVP by first introducing a unified shell-owned plugin package manifest, then using it to drive backend lifecycle, provider selection, host APIs, service contracts, and diagnostics.

**Target features:**
- Central plugin package manifest: package.json-like shell-owned list of user-installed frontend plugins, backend plugins, dependency relationships, categories, and active provider choices.
- Plugin dependency model: frontend plugins declare required backend providers; backend plugins declare category/service such as `audio`, `network`, or `shortcuts`.
- Backend plugin lifecycle: discovery from the package manifest, load, init, poll, command handling, stop/restart.
- Backend Luau host APIs: `mesh.exec`, `mesh.exec_shell`, `mesh.config`, `mesh.log`, `mesh.service.emit`, `mesh.service.set_poll_interval`.
- Service provider contracts: backend plugins declare provided services, state shape, and command handlers.
- Runtime diagnostics: init/poll/command failures degrade plugin health and remain visible.
- MVP proof plugin: one fresh backend service plugin proves the documented backend contract.

## Goals

### Primary

- Establish the central plugin package manifest as the first source of truth for installed/active plugins.
- Make backend plugin lifecycle behavior deterministic and testable from that manifest.
- Lock the MVP backend Luau API contract.
- Route backend service emissions and commands through generic contracts, not service-specific Rust logic.
- Ensure backend errors produce actionable diagnostics while keeping the shell alive where possible.

### Secondary

- Provide a minimal reference backend plugin that exercises the MVP contract.
- Keep actual download/marketplace behavior separate from the local package manifest and dependency graph.

### Out of Scope

- Frontend UI polish and new shell surfaces.
- Remote package download, signing, sandboxing, or marketplace flows.
- Full scripting API documentation beyond backend MVP notes/proofs.
- LSP completions/hover.

## Success Criteria

Done when:

1. A central package manifest lists installed frontend/backend plugins and can be parsed into a normalized plugin graph.
2. Frontend plugin backend dependencies and backend plugin categories/providers can be represented in that graph.
3. The shell can derive active backend provider choices from defaults plus user overrides.
4. A fresh backend plugin can be discovered, loaded, initialized, and polled by the shell from the unified plugin graph.
5. The plugin can read config, log, execute allowed commands, emit service state, and change its poll interval.
6. Invalid manifests, missing entrypoints, Luau init failures, poll failures, and command failures are reported through diagnostics.

## Scope

**In scope - plugin package manifest:**
- A shell-owned package.json-like manifest for installed plugins.
- Frontend plugin dependency declarations for required backend capabilities/providers.
- Backend plugin category/provider metadata, including multiple providers in the same category.
- User provider selection that can be reorganized into shell settings.

**In scope - backend runtime:**
- Manifest handling for backend service plugins selected through the package graph.
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

The previous v1.0 planning artifacts were archived on 2026-05-03 under `.planning/milestones/v1.0-reset-2026-05-03-*` before this roadmap reset.

Phase 1 is complete. MESH now has the package.json-like installed module manifest foundation, module package schema, package-first compatibility loader, normalized installed module graph, active backend provider selection proof, and repo-local fixtures that mirror the target `~/.mesh` layout. Phase 2 is ready to plan: backend lifecycle should consume that graph for deterministic provider runtime creation, init, polling, and stop/restart behavior.

## Requirements

See `.planning/REQUIREMENTS.md` for the active v1.1 requirement set.

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Backend plugins use Luau for service logic | Keeps Rust core as wiring and makes services extensible | Locked |
| Rust core must stay generic across services | Prevents audio/network/power special cases from becoming architecture | Locked |
| Package graph comes before backend lifecycle | A unified installed-plugin interface should drive which backend providers exist and which one is active | Decided this milestone |
| Backend MVP comes before remote distribution and LSP | Runtime stability and local package semantics are prerequisites for tooling and package workflows | Decided this milestone |
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
*Last updated: 2026-05-03 after Phase 1 completion*
