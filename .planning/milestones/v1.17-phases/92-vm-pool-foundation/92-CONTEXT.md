# Phase 92: VM Pool Foundation - Context

**Gathered:** 2026-06-07
**Status:** Ready for planning
**Mode:** Auto-generated (infrastructure phase — discuss skipped)

<domain>
## Phase Boundary

Introduce `LuaVmPool`, `PooledVm` RAII guard, and `ChunkCache` as isolated, independently testable types in `mesh-core-scripting`. No behavioral change to existing `ScriptContext` — pool and cache types exist but are not yet wired into any component path.

Covers requirements: POOL-01, POOL-02, POOL-03, POOL-04, CACHE-01, CACHE-02.

</domain>

<decisions>
## Implementation Decisions

### Claude's Discretion
All implementation choices are at Claude's discretion — pure infrastructure phase. Use ROADMAP phase goal, success criteria, and codebase conventions to guide decisions.

Key constraints from REQUIREMENTS.md:
- `LuaVmPool` is thread-local; each pool VM initialized with `Lua::sandbox(true)` so stdlib tables are read-only
- `PooledVm` is a RAII drop guard that returns its slot to the pool; must assert same-thread identity on drop
- Pool grows on-demand with minimum 4 VM floor; never blocks on exhaustion
- `ChunkCache` is process-wide (not thread-local), keyed on FNV64 content hash of source strings
- No changes to `ScriptContext` behavior — pool and cache are standalone types only

</decisions>

<code_context>
## Existing Code Insights

### Integration Points
- `mesh-core-scripting` crate — new types go here: `crates/core/scripting/src/`
- `ScriptContext` in `crates/core/scripting/src/` — NOT modified in this phase; pool types are standalone
- `BackendScriptContext` — NOT modified in this phase
- `mlua` is the Luau VM library already used; `Lua::sandbox(true)` is the pool VM construction call

### Established Patterns
- Rust unit tests in `#[cfg(test)]` modules at bottom of source files
- `mesh-core-scripting` already has thread-local and process-wide state patterns
- RAII patterns established across the codebase for resource ownership

### Reusable Assets
- `mlua` already in `mesh-core-scripting` Cargo.toml
- Existing `ScriptContext` as the reference for how VMs are currently created (to inform pool design, not to modify)

</code_context>

<specifics>
## Specific Ideas

No specific requirements — infrastructure phase. Refer to ROADMAP phase description and success criteria.

POOL-04 thread assertion: use `std::thread::current().id()` captured at checkout, compared on drop.
CACHE-01 FNV64: use `fnv` crate or hand-roll a 64-bit FNV-1a hash; keyed by source string content.

</specifics>

<deferred>
## Deferred Ideas

- Bytecode cross-VM sharing via `luau_compile`/`luau_load` C FFI — deferred (mlua v0.11 safe API doesn't expose this)
- Pool size auto-tuning — future milestone
- Backend VM pooling — lazy-init only in v1.17, no pooling for backend contexts

</deferred>
