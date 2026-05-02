# Phase 02: service-proxy-delivery - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-05-02
**Phase:** 02-service-proxy-delivery
**Areas discussed:** reactive update model, failure visibility, contract source of truth, bundled surface migration scope, proxies are read-and-command only

---

## Reactive Update Model

| Option | Description | Selected |
|--------|-------------|----------|
| Rerender only | Service payload updates mark consumers dirty, and scripts/templates read fresh proxy fields on rerender without service callback APIs. | ✓ |
| Keep both | Rerender invalidation is primary, but callbacks remain a first-class documented pattern. | |
| Callbacks first | Callback-style service updates remain the main authoring model. | |

**User's choice:** Rerender only, with no compatibility path kept in Phase 2.
**Notes:** The discussion refined this beyond whole-service invalidation. The user wants field-level reactivity: only top-level proxy fields actually read during script/render execution should create dependencies, and rerender should occur only when one of those tracked fields changes value. Legacy `mesh.service.on(...)`, `mesh.service.bind(...)`, and `proxy.on_change(...)` should be removed entirely.

---

## Failure Visibility

| Option | Description | Selected |
|--------|-------------|----------|
| Diagnostic + Lua error | Emit a visible plugin diagnostic and still return the normal Lua error. | ✓ |
| Diagnostic only | Record a visible diagnostic without a Lua-facing error. | |
| Lua error only | Keep the Lua error path without guaranteed visible diagnostics. | |

**User's choice:** Diagnostic + Lua error.
**Notes:** `pcall(require, ...)` may catch the Lua error for fallback UI, but it must not suppress the diagnostic. Diagnostics should include plugin, interface, and concrete failure reason, and should be treated as errors.

---

## Contract Source Of Truth

| Option | Description | Selected |
|--------|-------------|----------|
| Contract metadata is authoritative | Contracts explicitly define the frontend-readable state fields and callable commands. | |
| Docs are authoritative | Prose docs define the official proxy shape. | |
| Runtime behavior is authoritative | The real emitted payloads and runtime behavior define the truth, with docs/contracts following implementation. | ✓ |

**User's choice:** Runtime behavior is authoritative, but services should still have a real interface.
**Notes:** The user clarified that the project should model a portable base interface plugin with formally inheriting/extending providers. Richer providers may expose additive functionality on the same interface. The weakest provider should not cap the public API; instead, the model is portable core plus richer dominant provider. The core should exist as a base plugin-level contract artifact so future shell concepts can define their own base interfaces.

---

## Bundled Surface Migration Scope

| Option | Description | Selected |
|--------|-------------|----------|
| Migrate all bundled consumers in scope | Update panel and all relevant quick-settings consumers to the finalized proxy model. | ✓ |
| Migrate one reference path only | Pick one strong example and defer the rest. | |
| Runtime first, surfaces later | Finish runtime/contract work first and defer most `.mesh` migration. | |

**User's choice:** Migrate all bundled consumers in scope.
**Notes:** The user wants legacy service usage removed completely from bundled surfaces. Built-in surfaces may assume the dominant provider for advanced behavior, but refined that the core user path should still work on the shared base contract, including both core reads and primary commands.

---

## Proxies Are Read-And-Command Only

| Option | Description | Selected |
|--------|-------------|----------|
| Commands only for writes | Frontend reads state from the proxy and sends backend changes through commands only. | ✓ |
| Optimistic local writes | Frontend mutates locally before backend confirms. | |
| Direct proxy assignment | Proxy assignment translates directly into backend updates. | |

**User's choice:** Commands only for writes, using named methods on the proxy.
**Notes:** The user wanted frontend code to be able to set values in backend services, but through methods like `audio.set_volume(50)` rather than direct state mutation. Unsupported commands should fail loudly with both a Lua error and a visible diagnostic. This area also confirmed the core proxy shape: readable state fields plus named command methods, with no subscription-style update APIs.

---

## the agent's Discretion

- Exact runtime representation of dependency tracking and field snapshots.
- Exact internal representation of base-interface inheritance as long as the public contract/inheritance semantics hold.
- Exact verification split across runtime, shell, contract, docs, and bundled-surface tests.

## Deferred Ideas

None.
