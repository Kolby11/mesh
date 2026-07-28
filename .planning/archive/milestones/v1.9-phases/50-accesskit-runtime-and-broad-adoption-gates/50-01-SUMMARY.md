---
phase: 50
plan: "01"
subsystem: mesh-core-render
tags: [accesskit, accessibility, renderer-adapter, retained-node]
dependency_graph:
  requires: []
  provides: [accesskit-runtime-update-adapter, renderer-library-adoption-gates]
  affects: [mesh-core-render, docs]
tech_stack:
  added: []
  patterns: [feature-gated adapter module, retained-node tree conversion, adoption-gate documentation]
key_files:
  created:
    - crates/core/frontend/render/src/accesskit_adapter.rs
  modified:
    - crates/core/frontend/render/src/lib.rs
    - docs/renderer-migration.md
    - docs/renderer-ownership.md
    - docs/frontend/renderer-contract.md
decisions:
  - "Build real accesskit::TreeUpdate values from retained WidgetNode trees under renderer-accesskit."
  - "Use MESH NodeId directly as accesskit::NodeId."
  - "Keep platform/screen-reader publication deferred."
  - "Classify AccessKit retained-node updates as a production adapter boundary, not a public author API."
metrics:
  duration: "~20 minutes"
  completed: "2026-05-21"
  tasks: 3
  files: 5
---

# Phase 50 Plan 01: AccessKit Runtime Update Adapter Summary

Implemented a feature-gated AccessKit retained-node runtime update adapter and closed the v1.9 renderer-library adoption documentation gates.

## Files

| File | Change | Lines |
|------|--------|-------|
| `crates/core/frontend/render/src/accesskit_adapter.rs` | Created retained `WidgetNode` → `accesskit::TreeUpdate` adapter and tests | 237 |
| `crates/core/frontend/render/src/lib.rs` | Registered and re-exported feature-gated adapter helper | 42 |
| `docs/renderer-migration.md` | Marked adoption checklist complete and added Phase 50 status/gates | 152 |
| `docs/renderer-ownership.md` | Updated AccessKit ownership classification | 50 |
| `docs/frontend/renderer-contract.md` | Clarified stable author contract and deferred platform publication | 57 |

## Verification

| Command | Result |
|---------|--------|
| `cargo check -p mesh-core-render` | Passed, existing `placement_top` warning |
| `cargo test -p mesh-core-render --features renderer-accesskit accesskit` | 4 passed |
| `cargo test -p mesh-core-render --features renderer-libraries renderer_library` | 2 passed, existing `placement_top` warning |
| `cargo test -p mesh-core-shell phase44_navigation` | 2 passed |
| `rg "AccessKit retained-node|renderer-accesskit|TreeUpdate|platform publication" docs/...` | Required docs records found |

## Requirements

- **A11Y-01:** Satisfied. AccessKit updates are built from retained `WidgetNode` trees and MESH `NodeId` values.
- **A11Y-02:** Satisfied. Roles, labels, focusability/control actions, bounds, child relationships, state, and focus are represented and tested.
- **GATE-01:** Satisfied. Default and feature-enabled render checks plus shipped shell surface checks passed.
- **GATE-02:** Satisfied. Renderer ownership, migration, and author-contract docs classify production, experimental, and deferred adapter states.

Platform accessibility publication remains deferred.
