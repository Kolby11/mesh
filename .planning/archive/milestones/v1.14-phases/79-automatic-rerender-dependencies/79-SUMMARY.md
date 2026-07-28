---
phase: 79
phase_name: automatic-rerender-dependencies
status: complete
completed: 2026-05-26
requirements:
  - LUARERENDER-01
  - LUARERENDER-02
  - LUARERENDER-03
  - LUARERENDER-04
  - LUARERENDER-05
  - LUARERENDER-06
---

# Phase 79: Automatic Rerender Dependencies - Summary

## Delivered

- Verified existing service dependency tracking records top-level service fields read by scripts.
- Verified shell service update invalidation is field-aware and rebuilds when tracked fields change.
- Verified locale and theme changes already force automatic script/tree rebuild paths.
- Verified bound child public field refreshes use parent state writes, so bound-field changes participate in normal script state invalidation.
- Recorded storage read tracking as v1.15 reserved behavior.
- Preserved explicit redraw and invalidation escape hatches.

## Verification

Passed:

```bash
nix develop -c cargo test -p mesh-core-shell invalidation
nix develop -c cargo test -p mesh-core-scripting interface_proxy_tracks_top_level_field_reads
nix develop -c cargo fmt --check
```
