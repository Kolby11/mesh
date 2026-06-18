---
phase: 104-retained-taffytree
plan: 03
status: complete
completed: 2026-06-18
---

# Plan 104-03 Summary

Wired retained layout into the shell render path:

- Added `layout_state: PerSurfaceLayoutState` to `FrontendSurfaceComponent`.
- Initialized retained layout state in the component constructor.
- Replaced the hot layout call in `finalize_tree` with `LayoutEngine::compute_incremental`.
- Derived `dirty_layout` from `ComponentDirtyFlags::LAYOUT` and `dirty_structural` from `SCRIPT | TEXT`.
- Reset retained layout state on theme changes, locale changes, and source reloads.
- Preserved shell `Send` compatibility by marking the owned per-surface layout cache as `Send` with an explicit safety note.
- Updated a stale shell test helper to current render/style paths so the shell test target compiles.

Verification:

- `nix develop -c cargo build --package mesh-core-shell` passed.
- `nix develop -c cargo test --package mesh-core-shell` compiled and ran, but the current dirty worktree has 54 failing shell tests tied to existing navigation/module/service changes. See `104-VERIFICATION.md`.

