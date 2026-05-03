# Phase 4: Service Provider Contract - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-05-03
**Phase:** 4-Service Provider Contract
**Areas discussed:** State identity and envelope, latest state semantics, command result contract, contract validation strictness

---

## State Identity And Envelope

| Option | Description | Selected |
|--------|-------------|----------|
| Explicit `mesh.service.emit(payload)` | Backend providers call an emit API with a JSON-compatible payload. Runtime tags it with interface/provider identity. | |
| Exported reactive `state` | Backend providers assign top-level exported `state`; runtime snapshots it after lifecycle callbacks and propagates changes. | yes |
| Runtime-injected state envelope | Runtime wraps provider payloads with public source/provider fields visible in state. | |

**User's choice:** Use exported top-level `state` as the primary backend service state contract.
**Notes:** The user wanted a simpler structure where backend modules expose a special variable. A top-level non-`local` `state` variable should automatically become module state, directly readable by consumers as `module.state`. Module identity should correspond to the package id specified in `package.json`.

---

## Module Import And Provider Resolution

| Option | Description | Selected |
|--------|-------------|----------|
| Import concrete provider ids | Consumers require `@mesh/pipewire-audio` or another concrete provider directly. | |
| Import interface ids | Consumers require `@mesh/audio`; runtime resolves to the active provider. | yes |
| Support only legacy service event channels | Continue pushing service events without direct `require(...)` state access. | |

**User's choice:** `require("@mesh/audio")` resolves to the active provider for that interface, while concrete provider imports may exist for explicit provider-specific use.
**Notes:** This preserves provider swapping while keeping the public module shape simple. Future Lua package-manager support should not be blocked by this design, but it is deferred.

---

## Command Result Contract

| Option | Description | Selected |
|--------|-------------|----------|
| Return result table | Command methods return `{ ok = true }` or `{ ok = false, error = "..." }`. | |
| Reactive state only | Commands return nothing; callers observe only state updates and diagnostics. | |
| Both result and state | Commands return a result table and may also update reactive provider state. | yes |

**User's choice:** Both result and state.
**Notes:** This satisfies BSVC-05 by making command success/failure visible to the caller while preserving reactive state as the source of truth for current service data.

---

## Latest State Semantics

| Option | Description | Selected |
|--------|-------------|----------|
| Per interface only | `require("@mesh/audio").state` always means active provider state. | |
| Per interface plus provider metadata | Public state is per interface; provider id is tracked internally for diagnostics/debugging. | yes |
| Per provider and per interface | Store each provider state and map interface state to the active provider. | |

**User's choice:** Per interface plus provider metadata.
**Notes:** Normal consumers see the active provider's interface state. Provider identity remains metadata and should not be injected into the public `state` table by default.

---

## Contract Validation Strictness

| Option | Description | Selected |
|--------|-------------|----------|
| Strict at runtime | Missing/wrong state fields or unknown commands become errors. | |
| Warn during Phase 4 | Validate declarations and command names; state shape mismatches produce warnings/diagnostics. | yes |
| Startup declarations only | Verify manifest/interface/provider identity but do not validate emitted state shape yet. | |

**User's choice:** Warn during Phase 4.
**Notes:** This gives plugin authors visibility without making the first contract implementation brittle. Full diagnostics hardening remains Phase 5 work.

## the agent's Discretion

- Exact Rust/Luau mechanics for observing exported top-level `state`.
- Exact compatibility behavior for `mesh.service.emit(...)`.
- Exact command result-table fields beyond `ok` and `error`.
- Whether direct concrete provider imports are fully implemented in Phase 4 or only preserved as a compatible design point.

## Deferred Ideas

- Add Lua package manager support so modules can import Lua libraries through package-managed dependencies.
