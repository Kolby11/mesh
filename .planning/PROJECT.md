# MESH — Scripting API Stabilization

## What This Is

MESH is a Rust-based, Wayland-native shell framework. This milestone stabilizes the Luau scripting runtime to MVP quality so that **external developers** can build complete shell plugins from scratch using a documented, reliable API.

The core value: **a plugin developer can write a `.mesh` frontend component and a `main.luau` backend service, connect them via `require('@mesh/...')`, and have everything work end-to-end** — without guessing, reading source code, or hitting silent failures.

## The Problem

The scripting layer is partially implemented but not production-solid:

1. **Missing host APIs** — `mesh.exec()` / `exec_shell()`, `mesh.config()`, and `mesh.log()` are incomplete or absent, blocking backend plugin authors from doing basic work
2. **Inconsistent behavior** — reactive globals, event handlers (`on_click`, `on_change`, `on_<service>_update`), and service proxy callbacks have edge cases and silent failures
3. **No typed contracts** — `require('@mesh/audio')` returns a proxy, but what fields it exposes, which callbacks exist, and what commands are available is undocumented and inconsistent; plugin authors must guess

The specific known failure: frontend scripts run in the Luau VM but service data from backend plugins does not reliably reach the frontend — the `require('@mesh/...')` proxy system is not delivering state updates.

## Core Value

> A developer with zero MESH knowledge can write a working top panel plugin + backend service in one sitting, guided only by the API documentation.

## Current Milestone: v1.0 Scripting API Stabilization

**Goal:** Stabilize the Luau frontend/backend scripting API so external developers can build documented, reliable MESH plugins end to end.

**Target features:**
- Backend host APIs for command execution, plugin configuration, structured logging, and service emission
- Reliable service proxy delivery from backend Luau plugins to frontend `.mesh` components
- Predictable frontend reactivity, service update handlers, and element event handlers
- Real top panel and quick settings surfaces backed by audio and network services
- API reference documentation validated against a fresh reference plugin
- XDG icon rendering fixes needed for production-quality shell surfaces

## Goals

### Primary
- Lock the Luau scripting API contract for both frontend and backend authoring
- Fix the service connection layer so `require('@mesh/audio')` delivers live state and fires `on_change` reliably
- Implement the critical host APIs: `mesh.exec()`, `mesh.config()`, `mesh.log()`
- Make reactive state, event handlers, and service proxies work predictably and identically every time

### Secondary
- Connect top panel and quick settings surfaces to real backend services (audio, network, power, media)
- Write the scripting API reference documentation

### Out of Scope
- LSP completions/hover for `.mesh` files — follow-up milestone
- Notification center surface — follow-up milestone
- Launcher surface — follow-up milestone
- Package manager / signed packages — later milestone

## Success Criteria

Done when a developer can:
1. Write a `main.luau` backend service using `mesh.exec()`, `mesh.service.emit()`, and `mesh.log()` — and it works
2. Write a `.mesh` frontend using reactive globals, `require('@mesh/audio')`, and `on_change` — and it re-renders when the backend emits
3. Run the top panel and quick settings with real audio + network data from backend services
4. Read API documentation that covers the full scripting surface without needing to read Rust source

The concrete test: write a fresh reference plugin end-to-end and validate the documented API is accurate and complete.

## Scope

**In scope — surfaces:**
- Top panel (primary bar surface)
- Quick settings drawer (connected to real backends with interactive controls: mute, volume, wifi toggle)

**In scope — backend services:**
- Audio (PipeWire / PulseAudio)
- Network (NetworkManager)
- Power (UPower)
- Media (MPRIS)

**In scope — scripting API:**
- Frontend: reactive globals, event handlers, `require('@mesh/...')` service proxies
- Backend: `mesh.exec()`, `mesh.exec_shell()`, `mesh.config()`, `mesh.log()`, `mesh.service.emit()`, `mesh.service.set_poll_interval()`
- Service proxy contract: state fields, `on_change()`, command methods

## Audience

External developers. The API must be:
- Learnable without reading Rust source
- Predictable — same inputs always produce same outputs
- Documented — API reference written as part of this milestone

## Constraints

- Build and test must work within the Nix dev shell (`nix develop`)
- No constraints on API changes — if the current API shape is wrong, fix it

## Requirements

### Active

- [ ] `mesh.exec(cmd, args)` host API implemented and callable from backend Luau scripts
- [ ] `mesh.exec_shell(cmd)` host API implemented and callable from backend Luau scripts
- [ ] `mesh.config()` host API returns plugin settings as a Luau table
- [ ] `mesh.log(level, msg)` host API writes structured log entries from Luau scripts
- [ ] `mesh.service.emit(payload)` reliably delivers state to all connected frontend proxies
- [ ] `require('@mesh/<service>')` proxy returns a table with correct state fields populated
- [ ] `proxy.on_change(fn)` fires whenever the backend emits a new payload
- [ ] Reactive globals in `<script>` always trigger a re-render when assigned
- [ ] `on_<service>_update()` handler fires in frontend script after every backend emit
- [ ] Event handlers (`on_click`, `on_change` on elements) fire reliably with correct state
- [ ] Top panel surface renders with live data from at least one backend service
- [ ] Quick settings surface renders with interactive audio + network controls backed by real services
- [ ] Scripting API reference document written and validated against a working reference plugin
- [ ] Icon rendering works for XDG icon names (four bugs identified in llm-context.md)

### Out of Scope

- LSP support — not in this milestone
- Notification center — not in this milestone
- Launcher surface — not in this milestone
- Signed/sandboxed packages — later milestone

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Backend plugins must use Luau (not Rust) for service logic | Architectural rule — core is wiring only; service logic in Luau keeps the shell extensible | Locked |
| `require('@mesh/service')` is the only interface between backend and frontend | Typed proxy pattern; no shared Rust state between plugins | Locked |
| Audience is external developers at MVP | API must be clean and discoverable, not just usable by the author | Decided this milestone |
| LSP deferred | Runtime correctness is more valuable than editor tooling at MVP | Deferred |

## Evolution

This document evolves at phase transitions and milestone boundaries.

**After each phase transition** (via `/gsd-transition`):
1. Requirements invalidated? → Move to Out of Scope with reason
2. Requirements validated? → Move to Validated with phase reference
3. New requirements emerged? → Add to Active
4. Decisions to log? → Add to Key Decisions
5. "What This Is" still accurate? → Update if drifted

**After each milestone** (via `/gsd-complete-milestone`):
1. Full review of all sections
2. Core Value check — still the right priority?
3. Audit Out of Scope — reasons still valid?
4. Update Context with current state

---
*Last updated: 2026-05-01 after milestone v1.0 start*
