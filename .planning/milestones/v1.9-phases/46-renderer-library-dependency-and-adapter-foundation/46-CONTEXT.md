# Phase 46: Renderer Library Dependency And Adapter Foundation - Context

**Gathered:** 2026-05-18T17:48:42+02:00
**Status:** Ready for planning

<domain>
## Phase Boundary

Phase 46 adds production dependency and rollout scaffolding for the selected renderer-library path without making any new renderer behavior authoritative by default. It should prepare Taffy layout, Parley text, AnyRender/Vello-style paint experimentation, and AccessKit runtime work for later phases while keeping current MESH layout, text, software paint, retained display-list, and presentation behavior as the rollback path.

</domain>

<decisions>
## Implementation Decisions

### Dependency Introduction

- **D-01:** Add selected renderer libraries as production Cargo manifest entries only behind conservative adoption boundaries. Prefer workspace-level dependency declarations when a crate will be shared, but keep actual use scoped to `mesh-core-render` unless a later phase proves another crate needs it.
- **D-02:** Taffy, Parley, and AccessKit are the primary foundation dependencies for later v1.9 phases. The paint dependency path remains experimental: Phase 46 may scaffold AnyRender and/or Vello as optional candidates, but must not commit to a default paint backend before the Phase 49 paint adapter work.
- **D-03:** Exact dependency versions are not locked by discussion. Planning must verify current crate metadata and choose Rust 1.85-compatible, non-yanked, production-appropriate versions. If a latest crate release is experimental or carries a higher Rust/native requirement, keep that crate disabled by default and document the reason.
- **D-04:** Do not add Blitz, Winit, DOM/web-platform, Stylo, or broader Skia expansion work in Phase 46. Existing `skia-safe` presence in `mesh-core-render` is not a signal to switch the selected v1.9 path away from the focused-crate plan.

### Rollback And Feature Switches

- **D-05:** Current renderer behavior remains the default for both build and runtime. New library-backed paths must be disabled unless an explicit feature flag, adapter switch, or test-only route opts into them.
- **D-06:** Use explicit Cargo features for dependency fan-out control. Recommended feature names for planning are `renderer-taffy`, `renderer-parley`, `renderer-accesskit`, `renderer-anyrender`, `renderer-vello`, plus an aggregate feature such as `renderer-libraries` for enabled-path checks.
- **D-07:** Feature flags are not enough by themselves once behavior exists. Adapter APIs should also preserve a local bypass that routes back to the current MESH implementation without touching `.mesh` authoring, shell surface lifecycle, or presentation ownership.

### Adapter Boundary

- **D-08:** Keep Phase 46 code in or below `crates/core/frontend/render` unless the planner finds a narrow manifest-only reason to update root workspace metadata. Do not move ownership into `mesh-core-shell` or `mesh-core-presentation`.
- **D-09:** Treat `FocusedProofSnapshot`, focused layout/text/paint evidence, and focused accessibility update construction as adapter-owned migration evidence, not public API. Phase 46 may harden module boundaries around this evidence, but it must not expose proof fields as `.mesh` author contract.
- **D-10:** Phase 46 should define the adapter seam and dependency gates; real Taffy layout computation, Parley shaping, paint backend command execution, and AccessKit runtime publication belong to Phases 47-50.

### Build, Nix, And Promotion Gates

- **D-11:** Before Phase 46 completes, update the renderer migration dependency record with actual Linux/Nix impact, root/workspace dependency changes, native libraries, binary/build risk, CI/test commands, and rollback path.
- **D-12:** Verification must cover both paths: current default behavior with library features disabled, and an enabled dependency build path that proves the optional feature set compiles.
- **D-13:** Minimum gate commands for planning should include Cargo metadata/tree checks, `cargo check` for default and enabled feature paths, focused `mesh-core-render` proof tests, focused Phase 44 shell tests, and workspace tests when feasible. If Nix cache, disk, or native dependency limits prevent a gate from running, record the blocker explicitly rather than silently downgrading the gate.

### Todo Scope

- **D-14:** Do not fold the audio popover transition-delay todo into Phase 46. It belongs to the next animations and motion-fidelity milestone.
- **D-15:** Do not fold the module install requirement-resolution todo into Phase 46. It remains separate module-system work.

### the agent's Discretion

Interactive `AskUserQuestion` was unavailable in this runtime, so the context uses conservative defaults consistent with the v1.9 roadmap and existing renderer migration docs. The planner may adjust exact feature names or file layout if the codebase demands it, but must preserve the decisions above: optional dependency fan-out, current renderer default, local rollback, no behavior switch in Phase 46, and explicit build/Nix risk documentation.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Milestone Scope

- `.planning/PROJECT.md` — Current v1.9 milestone goal, target features, and v1.10 animation deferral.
- `.planning/REQUIREMENTS.md` — Phase 46 requirements LIBS-01 through LIBS-03 and out-of-scope boundaries.
- `.planning/ROADMAP.md` — Phase 46 success criteria and downstream phase sequencing.
- `.planning/STATE.md` — Current milestone state and carried-forward renderer decisions.

### Renderer Migration Contracts

- `docs/renderer-migration.md` — Phased reversible renderer migration principles, promotion gates, dependency record template, and required commands.
- `docs/renderer-ownership.md` — Authoritative, adapter-owned, and replacement-candidate renderer boundaries.
- `docs/frontend/renderer-contract.md` — Public `.mesh` renderer contract and deferred work boundaries.
- `crates/core/frontend/render/README.md` — Render crate ownership and where render-specific code should live.

### Current Code Boundaries

- `Cargo.toml` — Workspace members, shared dependencies, Rust 1.85 floor, and current absence of renderer-library workspace deps.
- `crates/core/frontend/render/Cargo.toml` — Current render crate dependencies and likely home for optional renderer-library deps.
- `flake.nix` — Current Nix dev-shell packages and runtime library list that must be updated if new native deps require it.
- `crates/core/frontend/render/src/lib.rs` — Current render crate exports.
- `crates/core/frontend/render/src/proof.rs` — Focused proof snapshot, focused text/layout/paint evidence, and AccessKit-compatible update evidence from Phase 44.
- `crates/core/frontend/render/src/display_list.rs` — Retained display-list ownership and selection paint payloads that must remain authoritative.

### Prototype Evidence

- `.planning/prototypes/phase43/Cargo.toml` — Throwaway prototype dependency choices; useful evidence, not production version authority.
- `.planning/prototypes/phase43/evidence/focused-crate.md` — Focused-crate proof evidence for Taffy layout, Parley text, AnyRender paint, and AccessKit boundary.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets

- `crates/core/frontend/render/src/proof.rs`: Existing focused proof structures already capture retained `NodeId`, layout evidence, text evidence, paint slots, dirty counters, damage, diagnostics, and AccessKit-compatible IDs.
- `docs/renderer-migration.md`: Existing dependency record template and broad adoption checklist should be filled instead of creating a parallel process.
- `docs/renderer-ownership.md`: Existing ownership table gives the planner the authoritative boundaries that adapter scaffolding must not cross.
- `.planning/prototypes/phase43`: Prototype-only dependency/evidence source for Taffy, Parley, AnyRender, and AccessKit, useful for naming and proof continuity.

### Established Patterns

- Renderer-specific dependencies belong in `mesh-core-render`; compiler/runtime/presentation crates should not absorb render-library fan-out without a specific boundary reason.
- Current MESH rendering authority flows through retained widget nodes, render objects, retained display list, software painter, `PixelBuffer`, and presentation. Candidate crates are replacement candidates or adapter-owned evidence until later phases promote them.
- The workspace currently has no Cargo feature pattern for renderer candidates, so Phase 46 should introduce a small, explicit feature pattern rather than broad default dependencies.
- Nix dev-shell native runtime libraries are explicit in `flake.nix`; new native requirements must be documented and added there when required.

### Integration Points

- `crates/core/frontend/render/Cargo.toml`: Main dependency and feature declaration target.
- `Cargo.toml`: Workspace dependency target if selected crates should be centralized.
- `crates/core/frontend/render/src/lib.rs`: Export point for any internal adapter module that downstream phases need.
- `docs/renderer-migration.md` and `docs/renderer-ownership.md`: Required documentation targets for dependency impact, rollout status, and rollback path.

</code_context>

<specifics>
## Specific Ideas

No user-specific implementation preferences were added during this run beyond the prior request: make v1.9 implement the selected rendering libraries, and keep animation work for the following milestone.

</specifics>

<deferred>
## Deferred Ideas

### Reviewed Todos (not folded)

- Audio Popover Transition Delay Polish — deferred to the v1.10 animations and motion-fidelity milestone.
- Define module install requirement resolution — deferred as separate module-system architecture work.

</deferred>

---

*Phase: 46-Renderer Library Dependency And Adapter Foundation*
*Context gathered: 2026-05-18T17:48:42+02:00*
