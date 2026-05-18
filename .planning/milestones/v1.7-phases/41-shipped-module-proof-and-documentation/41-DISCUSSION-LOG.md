# Phase 41: Shipped Module Proof and Documentation - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-05-18T11:15:33Z
**Phase:** 41-Shipped Module Proof and Documentation
**Areas discussed:** Pending todos, Proof path, Evidence depth, Author documentation

---

## Runtime Note

The workflow attempted to use the interactive question UI, but
`request_user_input` was unavailable in Default mode. Per the discuss-phase
fallback rule, the recommended defaults were selected and recorded explicitly.

---

## Pending Todos

| Option | Description | Selected |
|--------|-------------|----------|
| Fold install-resolution todo | Use it only to shape proof scope around requirement/contribution/resource resolution; keep broad installer policy out of scope. | ✓ |
| Review only | Mention as related background but do not let it constrain decisions. | |
| Ignore todos | Leave all pending todos out of this discussion context. | |

**User's choice:** Defaulted to recommended fallback.
**Notes:** The audio popover and Blitz dependency todos were reviewed as false
matches and deferred.

---

## Proof Path

| Option | Description | Selected |
|--------|-------------|----------|
| Audio/navigation shipped path | Use `@mesh/navigation-bar`, `@mesh/audio-interface`, and PipeWire/PulseAudio providers as the real bundled proof. | ✓ |
| New small proof module | Create a purpose-built fixture module that exercises all concepts in isolation. | |
| Docs-only proof path | Treat existing docs/examples as sufficient proof and avoid runtime-path changes. | |

**User's choice:** Defaulted to recommended fallback.
**Notes:** The real audio/navigation path is the most complete shipped path
and already exercises frontend, backend, interface, provider, settings,
keybinds, resources, and diagnostics.

---

## Evidence Depth

| Option | Description | Selected |
|--------|-------------|----------|
| End-to-end targeted proof | Cover manifest normalization, contribution indexing, diagnostics, and proof-module behavior with focused tests. | ✓ |
| Graph-only proof | Prove installed graph records and diagnostics, leaving shell behavior to existing tests. | |
| Broad workspace proof | Use the whole workspace test suite as the primary acceptance gate. | |

**User's choice:** Defaulted to recommended fallback.
**Notes:** Focused tests are preferred because the workspace currently has
unrelated dirty shell/icon work. Shell tests must run through `nix develop -c`.

---

## Author Documentation

| Option | Description | Selected |
|--------|-------------|----------|
| Workflow walkthrough | Show how to add or extend a MESH module using the proof path from manifest to graph to runtime. | ✓ |
| Reference tables only | Keep docs as schema/reference material without a narrative workflow. | |
| Minimal doc patch | Only update stale wording found during implementation. | |

**User's choice:** Defaulted to recommended fallback.
**Notes:** The docs should teach canonical `module.json`, interface/provider
selection, contributions, settings/keybind overrides, resources, and
diagnostics using strict module vocabulary.

---

## the agent's Discretion

- Exact plan split.
- Exact fixture boundaries.
- Whether a small supplemental fixture is needed in addition to the real
  audio/navigation proof path.
- Which docs receive the final author walkthrough, provided
  `docs/module-system.md` remains the central reference.

## Deferred Ideas

- Full installer UX, provider conflict UI, resource pack suggestion policy,
  and settings materialization/reset semantics.
- Phase 31 audio popover transition polish.
- Future Blitz dependency/rendering architecture evaluation.
