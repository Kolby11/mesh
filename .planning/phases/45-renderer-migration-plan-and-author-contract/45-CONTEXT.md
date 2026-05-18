# Phase 45: Renderer Migration Plan and Author Contract - Context

**Gathered:** 2026-05-18
**Status:** Ready for planning

<domain>
## Phase Boundary

Phase 45 converts the completed renderer architecture decision, prototype comparison, and constrained production proof into a broad migration plan plus a documented renderer contract for future `.mesh` authors and shell/plugin maintainers.

This phase is a planning and documentation phase. It should not replace renderer code, add broad crate adoption, rewrite shipped surfaces, fix the deferred audio popover transition delay, or resume unrelated module installer work. Implementation work belongs to later migration phases derived from this plan.

</domain>

<decisions>
## Implementation Decisions

### Migration Sequence

- **D-01:** The migration plan should use phased, reversible adapter expansion as the default path. Current MESH renderer and presentation ownership stay authoritative until each later migration step passes explicit gates.
- **D-02:** The first migration steps should expand the Phase 44 focused proof boundary from evidence into production-ready adapter seams before any module-by-module replacement or broad renderer rewrite is attempted.
- **D-03:** Whole-renderer replacement is not an acceptable first migration step. It may appear only as a later option with dependency, observability, binary-size, performance, and rollback gates.

### Renderer Ownership Classification

- **D-04:** Classify current shell/component runtime, retained widget identity, render-object dirty slots, retained display-list concepts, diagnostics, profiling, damage plumbing, and Wayland presentation as authoritative until deliberately migrated.
- **D-05:** Classify Phase 44 focused proof snapshots, focused accessibility updates, focused text/layout/paint evidence, and any crate-facing conversion modules as adapter-owned.
- **D-06:** Classify Taffy, Parley, AnyRender/Vello-style rendering, AccessKit runtime expansion, Stylo-style resolution, and Skia fallback as replacement candidates or future adoption candidates, not current author-facing guarantees.
- **D-07:** Blitz remains reference/blocker evidence. Direct Blitz adoption stays deferred until the high-level crate compile blocker and shell ownership questions are resolved.

### `.mesh` Author Contract

- **D-08:** The author contract should state that Phase 45 does not introduce a broad `.mesh` authoring behavior change. Existing `.mesh` template/script/style, service proxy, theme, locale, capability, and module package contracts remain the public surface.
- **D-09:** The contract should document what the renderer decision means for authored UI: stable retained node identity remains a shell/runtime concern; selection colors remain theme-owned; diagnostics and profiling remain observable through current debug paths; accessibility mapping is moving toward AccessKit-compatible retained-node updates.
- **D-10:** The contract should explicitly name unsupported or deferred browser-like expectations. `.mesh` is not HTML/CSS in a browser engine, Blitz is not the production authoring model, Winit is not replacing Wayland shell ownership, and arbitrary DOM/web platform behavior is not promised.
- **D-11:** Plugin-authored surfaces should be told which behavior is expected to stay stable during migration: layout/control semantics for shipped component primitives, service-driven state updates, theme tokens, localized text, key/input behavior, and shell surface lifecycle boundaries.

### Build, CI, Release, And Rollback Guardrails

- **D-12:** Future renderer migration steps should be feature-gated or otherwise locally reversible until accepted. Rollout plans must describe how to disable or bypass the new adapter path without breaking shipped surfaces.
- **D-13:** Every migration step must document Linux/Wayland/Nix dependency implications, root workspace crate additions, native library requirements, binary-size or build-time risk, and CI/workspace command expectations.
- **D-14:** The plan should require gates for workspace tests, focused renderer proof tests, shipped navigation/audio surface regressions, selection proof, invalidation/damage/profiling evidence, and AccessKit-compatible update evidence.
- **D-15:** Observability parity is a migration gate. New renderer paths must preserve or replace MESH equivalents for retained identity, typed invalidation, damage, profiling, diagnostics, and debug payloads before becoming authoritative.

### Deferred Todo Handling

- **D-16:** Do not fold Audio Popover Transition Delay Polish into Phase 45. It remains accepted surface polish debt and should not distort the renderer migration plan.
- **D-17:** Do not fold Define Module Install Requirement Resolution wholesale into Phase 45. The author contract may reference module requirements at the boundary level, but installer/provider resolution remains a separate pending module-system task.
- **D-18:** The Blitz crate dependency todo has already been consumed by Phase 42 and Phase 43 evidence. Phase 45 should cite those results rather than reopening broad dependency research.

### the agent's Discretion

The user selected all gray areas for discussion and accepted the recommended defaults. Downstream planners may choose the exact migration-plan structure and document file names, but should preserve the phased reversible rollout, ownership classification, author-contract boundaries, and build/CI/release guardrails above.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Phase 45 Scope

- `.planning/ROADMAP.md` - Phase 45 goal, dependencies, and success criteria for MIGR-01 through MIGR-03.
- `.planning/REQUIREMENTS.md` - Renderer migration requirements and author-contract expectations.
- `.planning/PROJECT.md` - v1.8 milestone framing and renderer-history context.
- `.planning/STATE.md` - Current phase position, deferred items, and pending todos.

### Prior Renderer Decisions And Evidence

- `.planning/phases/42-renderer-architecture-decision-matrix/42-CONTEXT.md` - Architecture decision constraints, hard blockers, and crate posture.
- `.planning/phases/43-comparable-renderer-prototype-proofs/43-PROTOTYPE-COMPARISON.md` - Final comparison selecting the MESH-owned focused-crate path and documenting the Blitz blocker.
- `.planning/phases/43-comparable-renderer-prototype-proofs/43-PHASE44-HANDOFF.md` - Preserved contracts and constrained integration boundary inherited by Phase 44.
- `.planning/phases/44-selected-renderer-proof-integration/44-CONTEXT.md` - Phase 44 proof scope and preserved MESH ownership contracts.
- `.planning/phases/44-selected-renderer-proof-integration/44-INTEGRATION-EVIDENCE.md` - Completed proof evidence for retained identity, invalidation, damage/profiling, diagnostics, selection, accessibility, and shipped surfaces.
- `.planning/phases/44-selected-renderer-proof-integration/44-VERIFICATION.md` - Final Phase 44 PASS verdict and validation commands.

### Codebase Context Maps

- `.planning/codebase/ARCHITECTURE.md` - Current shell/frontend/render/presentation architecture and responsibility boundaries.
- `.planning/codebase/STACK.md` - Rust/Luau/.mesh stack, renderer dependencies, Wayland runtime, and Nix development environment.
- `.planning/codebase/INTEGRATIONS.md` - Wayland/compositor, IPC, system command, capability, observability, and deployment context.

### Current Renderer And Authoring Boundaries

- `crates/core/shell/src/shell/component/runtime_tree.rs` - Stable runtime node IDs and retained-tree dirty categories.
- `crates/core/frontend/render/src/render_object.rs` - Retained render-object tree and dirty slots.
- `crates/core/frontend/render/src/display_list.rs` - Retained display-list, damage, repaint policy, batching, and selection payloads.
- `crates/core/frontend/render/src/surface/painter.rs` - Current software painter boundary.
- `crates/core/presentation/src/lib.rs` - Current presentation boundary and damage-aware present path.
- `crates/core/presentation/src/wayland_surface/backend.rs` - Wayland surface backend behavior that broad migration must preserve or intentionally replace.
- `crates/core/ui/component/src/lib.rs` - `.mesh` single-file component parser.
- `crates/core/frontend/compiler/src/compile.rs` - `.mesh` frontend compiler and local component import boundary.
- `crates/core/runtime/scripting/src/context.rs` - Frontend Luau host APIs and service proxy behavior.
- `docs/module-system.md` - Current module/package authoring model.
- `docs/extensibility.md` - Current plugin author and capability model.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets

- The current frontend stack already separates `.mesh` parsing, compilation, scripting, retained runtime trees, render objects, display-list painting, and presentation. The migration plan should preserve these seams unless it explicitly replaces one.
- Phase 44 added proof evidence behind existing ownership rather than replacing `mesh-core-render` or `mesh-core-presentation`. That proof gives Phase 45 concrete boundaries for adapter expansion.
- Existing shipped navigation/audio tests, selection tests, invalidation/profiling tests, and render proof tests are the right regression gates for future migration phases.
- The Nix dev shell is required for reliable local workspace validation in this environment because direct host linking lacks required native libraries.

### Established Patterns

- Rust core remains generic across services and shell surfaces; renderer migration must not add service-specific behavior to render or shell core.
- Frontend authors write `.mesh` components, not browser pages. The renderer migration can borrow crates and architecture without promising browser platform semantics.
- MESH's observability contracts are part of the renderer architecture: retained node identity, typed invalidation, damage, profiling, diagnostics, and debug payloads are migration requirements, not optional debugging extras.
- Presentation remains Linux/Wayland shell-oriented. Winit or Blitz shell ownership should be treated as candidate evidence, not assumed production ownership.

### Integration Points

- The migration plan should classify boundaries around `mesh-core-render`, `mesh-core-presentation`, `mesh-core-frontend`, `mesh-core-component`, and shell component runtime.
- The author contract should translate renderer architecture decisions into documented expectations for `.mesh` plugin authors: stable behavior, non-goals, visible diagnostics, and migration compatibility promises.
- Build and release sections should call out root workspace dependencies, native Wayland/Nix implications, feature flags, binary-size/build-time tracking, and rollback paths.

</code_context>

<specifics>
## Specific Ideas

- Organize the Phase 45 output as a migration roadmap plus an author-facing contract section or companion doc.
- Use a classification table with at least three statuses: authoritative, adapter-owned, and replacement candidate.
- For every proposed migration step, require: objective, changed boundary, dependencies, tests, observability parity, feature flag/rollback path, and author-facing effect.
- Treat Phase 44 proof snapshots as the bridge from evidence to migration planning, not as a new public author API.

</specifics>

<deferred>
## Deferred Ideas

### Reviewed Todos (not folded)

- `Audio Popover Transition Delay Polish` (`.planning/todos/pending/2026-05-13-phase31-audio-popover-transition-delay.md`) - reviewed and not folded. This remains accepted polish debt for shell surface transition lifecycle.
- `Define Module Install Requirement Resolution` (`.planning/todos/pending/2026-05-15-define-module-install-requirement-resolution.md`) - reviewed and not folded wholesale. Phase 45 may mention module requirement boundaries only where the renderer author contract touches them.
- `Evaluate Blitz Crate Dependencies` (`.planning/todos/pending/2026-05-17-evaluate-blitz-crate-dependencies.md`) - already handled by Phase 42/43 decision work. Phase 45 should reference those artifacts instead of reopening the research.

</deferred>

---

*Phase: 45-Renderer Migration Plan and Author Contract*
*Context gathered: 2026-05-18*
