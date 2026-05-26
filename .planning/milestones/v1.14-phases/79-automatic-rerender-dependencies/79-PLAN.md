---
phase: 79
phase_name: automatic-rerender-dependencies
status: planned
created: 2026-05-26
requirements:
  - LUARERENDER-01
  - LUARERENDER-02
  - LUARERENDER-03
  - LUARERENDER-04
  - LUARERENDER-05
  - LUARERENDER-06
---

# Phase 79: Automatic Rerender Dependencies - Plan

## Tasks

### 79-01 Verify Existing Dependency Tracking

**Files:**
- `crates/core/runtime/scripting/src/context/runtime.rs`
- `crates/core/runtime/scripting/src/context/proxy.rs`
- `crates/core/shell/src/shell/component/shell_component.rs`
- `crates/core/shell/src/shell/component/runtime.rs`

**Work:**
- Confirm service reads record top-level tracked fields.
- Confirm service updates only trigger script rebuild when tracked fields or direct state changed.
- Confirm locale/theme changes trigger automatic rebuilds.
- Confirm bound instance refresh writes dirty parent state after child calls.

### 79-02 Preserve Escape Hatches

**Work:**
- Keep explicit redraw/invalidation APIs as compatibility and debug paths.
- Record v1.15 storage dependency tracking as reserved, not implemented.

## Verification

Run:

```bash
nix develop -c cargo test -p mesh-core-shell invalidation
nix develop -c cargo test -p mesh-core-scripting interface_proxy_tracks_top_level_field_reads
nix develop -c cargo fmt --check
```
