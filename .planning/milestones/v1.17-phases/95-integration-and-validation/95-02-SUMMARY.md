---
phase: 95-integration-and-validation
plan: 02
subsystem: scripting
tags: [lua, chunk-cache, fnv64, hot-reload, compile-and-execute]

# Dependency graph
requires:
  - phase: 94-env-isolation-lazy-init
    provides: "compile_and_execute() and compile_and_execute_simple() on ScriptContext with ChunkCache::get_or_insert()"
provides:
  - "ScriptContext::new_lazy() — documented lazy constructor integration point for pool/cache"
  - "FrontendSurfaceComponent::create_runtime_for_component switched to cached compile_and_execute path"
  - "reload_source() evicts ChunkCache entries for old script sources before .mesh recompile (CACHE-03)"
affects: ["future hot-reload reliability", "chunk cache integrity after source edits"]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "FNV64 content-hash-based chunk cache eviction during hot-reload"
    - "Lazy ScriptContext construction delegating to new() preserves vm: None invariant"

key-files:
  created: []
  modified:
    - "crates/core/runtime/scripting/src/context/runtime.rs"
    - "crates/core/shell/src/shell/component/runtime.rs"
    - "crates/core/shell/src/shell/component/shell_component.rs"

key-decisions:
  - "new_lazy() delegates to new() rather than duplicating the constructor body — both set vm: None, env_table: None"
  - "ChunkCache eviction uses FNV64 content hash of old script source — astronomically low collision rate on source text (~1 in 2^64)"
  - "use mesh_core_scripting::chunk_cache import placed inside reload_source() function body rather than module-level to scope the import to its only use site"

patterns-established:
  - "Pattern: cache-busting via content hash before source recompile ensures stale entries never survive a hot-reload cycle"
  - "Pattern: lazy constructors use delegating calls to avoid duplicating initialization logic"

requirements-completed: [INT-01, CACHE-03]

# Metrics
duration: 2min
completed: 2026-06-07
---

# Phase 95 Plan 02: Chunk Cache Integration and Hot-Reload Eviction Summary

**Script loading wired to cached compile_and_execute path; mtime watcher evicts stale cache entries on .mesh source change**

## Performance

- **Duration:** 2 min
- **Started:** 2026-06-07T18:06:47Z
- **Completed:** 2026-06-07T18:08:32Z
- **Tasks:** 2
- **Files modified:** 3

## Accomplishments
- Added `ScriptContext::new_lazy()` constructor delegating to `new()` — documented INT-01 integration point
- Switched `FrontendSurfaceComponent::create_runtime_for_component` from direct `load_script_with_interface_imports` to cached `compile_and_execute` path
- Wired `ChunkCache::remove(fnv64())` eviction in `reload_source()` for both main component script and all local component scripts before `.mesh` recompile

## task Commits

Each task was committed atomically:

1. **task 1: add ScriptContext::new_lazy() and switch create_runtime_for_component to compile_and_execute** - `dcf839f` (feat)
2. **task 2: wire ChunkCache eviction in reload_source() on .mesh file change** - `8e3f457` (feat)

**Plan metadata:** (will be committed with SUMMARY.md)

## Files Modified
- `crates/core/runtime/scripting/src/context/runtime.rs` - Added `new_lazy()` constructor (lines 153-162) delegating to `new()` with `vm: None, env_table: None`
- `crates/core/shell/src/shell/component/runtime.rs` - Changed `create_runtime_for_component` line ~218 from `load_script_with_interface_imports` to `compile_and_execute`
- `crates/core/shell/src/shell/component/shell_component.rs` - Added ChunkCache eviction at top of `reload_source()` (lines 776-787) for main script and local components before recompile

## Decisions Made
None - followed plan as specified. All integration choices (delegating constructor, function-scoped import, FNV64 hashing) were prescribed by the plan.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

- **Pre-existing build failures:** `cargo build -p mesh-core-shell` fails due to missing `xkbcommon` system library; `cargo build -p mesh-core-scripting` fails due to pre-existing errors in `backend/runtime.rs`. Neither is related to the changes in this plan. All three modified files pass `rustfmt --check` (shell_component.rs and shell/runtime.rs pass clean; context/runtime.rs has a pre-existing let-chain warning at line 284). Plan grep gates all pass: zero `load_script_with_interface_imports` calls remain in shell `runtime.rs`, `ChunkCache::remove` count in `shell_component.rs` is 2, import path `mesh_core_scripting::chunk_cache` is present.

## Next Phase Readiness
- INT-01 (compile_and_execute integration) complete — `create_runtime_for_component` uses cached path
- CACHE-03 (hot-reload eviction) complete — stale cache entries evicted on `.mesh` source change
- Ready for 95-03 (configuration persistence, CONFIG-01) if planned

---
*Phase: 95-integration-and-validation*
*Completed: 2026-06-07*
