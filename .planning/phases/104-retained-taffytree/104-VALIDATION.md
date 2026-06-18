---
phase: 104
slug: retained-taffytree
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-06-18
---

# Phase 104 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in `#[cfg(test)]` + `cargo test` |
| **Config file** | none — workspace uses `cargo test` directly |
| **Quick run command** | `cargo test --package mesh-core-elements -- layout` |
| **Full suite command** | `cargo test --package mesh-core-elements && cargo test --package mesh-core-shell` |
| **Estimated runtime** | ~30 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test --package mesh-core-elements -- layout`
- **After Wave completion:** Run `cargo test --package mesh-core-elements && cargo test --package mesh-core-shell`

---

## Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command |
|--------|----------|-----------|-------------------|
| LAYOUT-01 | Retained tree survives across frames; no fresh-build when valid=true | unit | `cargo test --package mesh-core-elements -- retained_layout` |
| LAYOUT-02 | STYLE-only → set_style; LAYOUT → mark_dirty + compute_layout | unit | `cargo test --package mesh-core-elements -- retained_layout` |
| LAYOUT-03 | `_mesh_key` identity survives TREE_REBUILD | unit | `cargo test --package mesh-core-elements -- retained_layout` |
| LAYOUT-04 | `remove_taffy_subtree` removes all descendants post-order | unit | `cargo test --package mesh-core-elements -- remove_taffy_subtree` |
| LAYOUT-05 | Retained output == fresh-build output for all 5 dirty scenarios | unit | `cargo test --package mesh-core-elements -- retained_layout_parity` |
