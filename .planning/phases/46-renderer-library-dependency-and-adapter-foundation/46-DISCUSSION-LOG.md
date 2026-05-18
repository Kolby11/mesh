# Phase 46: Renderer Library Dependency And Adapter Foundation - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-05-18T17:48:42+02:00
**Phase:** 46-renderer-library-dependency-and-adapter-foundation
**Areas discussed:** Todo scope, Dependency introduction, Rollback switches, Adapter boundary, Build and Nix gates

---

## Todo Scope

| Option | Description | Selected |
|--------|-------------|----------|
| Defer both | Keep Phase 46 focused on renderer library dependency and adapter rollout foundations. | ✓ |
| Fold audio delay | Pull animation transition polish into this phase. | |
| Fold module install | Pull module install/provider resolution into this phase. | |

**User's choice:** Agent-selected fallback because interactive question UI was unavailable.
**Notes:** The audio delay todo is explicitly assigned to the next animation milestone. Module install resolution is separate module-system architecture work.

---

## Dependency Introduction

| Option | Description | Selected |
|--------|-------------|----------|
| Optional production deps | Add selected crates to production manifests behind disabled-by-default features and verify versions during planning. | ✓ |
| Default production deps | Add selected crates as always-built dependencies immediately. | |
| Prototype-only deps | Keep all crates in `.planning/prototypes` and do not add production manifest entries. | |

**User's choice:** Agent-selected fallback because interactive question UI was unavailable.
**Notes:** This satisfies LIBS-01 while controlling dependency fan-out and preserving rollback.

---

## Rollback Switches

| Option | Description | Selected |
|--------|-------------|----------|
| Compile feature plus local bypass | Use Cargo features for dependency fan-out and require adapter-level fallback once behavior exists. | ✓ |
| Runtime toggle only | Compile all dependencies by default and switch behavior at runtime. | |
| No switch until later | Add dependencies now and defer rollback design. | |

**User's choice:** Agent-selected fallback because interactive question UI was unavailable.
**Notes:** Cargo features alone are not enough for later behavior changes; adapters must still route back to current MESH behavior.

---

## Adapter Boundary

| Option | Description | Selected |
|--------|-------------|----------|
| Keep under render crate | Keep scaffolding in `mesh-core-render`, preserving shell and presentation ownership. | ✓ |
| Spread through shell | Add shell-level ownership for candidate renderer crates now. | |
| Presentation-level boundary | Start from presentation or Wayland backend integration. | |

**User's choice:** Agent-selected fallback because interactive question UI was unavailable.
**Notes:** Existing docs classify render object, display list, software painter, and presentation as authoritative; Phase 46 should not move those boundaries.

---

## Build And Nix Gates

| Option | Description | Selected |
|--------|-------------|----------|
| Document and test both paths | Measure dependency impact and run disabled/enabled feature checks before completion. | ✓ |
| Defer measurement | Add dependencies first and measure in later adapter phases. | |
| Full workspace only | Rely on final workspace tests without targeted dependency/build evidence. | |

**User's choice:** Agent-selected fallback because interactive question UI was unavailable.
**Notes:** The migration docs already require dependency records for Linux/Nix impact, native libraries, build risk, CI gates, and rollback.

---

## the agent's Discretion

- Exact dependency versions remain open for the planner to verify against current Cargo metadata and Rust 1.85 compatibility.
- Exact feature names may be adjusted if Cargo or repository conventions require it, but the disabled-by-default feature and rollback requirements are locked.

## Deferred Ideas

- Audio popover transition delay polish belongs to the next animations milestone.
- Module install requirement resolution belongs to separate module-system work.
