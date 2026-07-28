# Roadmap: MESH v1.8 Rendering Engine Architecture

**Milestone goal:** Decide and prove the next MESH rendering architecture by evaluating Blitz as a base or inspiration, comparing it against a MESH-owned focused-crate path, and shipping a constrained proof path that preserves retained invalidation, diagnostics, accessibility, profiling, and existing shipped-surface behavior.

**Starting phase:** Phase 42, continuing after v1.7 Phase 41.

## Phases

### Phase 42: Renderer Architecture Decision Matrix

**Goal:** Produce a source-backed adopt-vs-build decision for Blitz and the candidate crate stack before implementation work commits to a direction.

**Requirements:** REND-01, REND-02, REND-03

**Depends on:** v1.8 research

**Success criteria:**
1. Blitz direct adoption, Blitz-inspired architecture borrowing, and MESH-owned focused-crate paths are compared with the same scorecard.
2. Each candidate crate has an explicit accept, defer, or reject outcome for v1.8.
3. The scorecard includes determinism, retained invalidation, profiling, diagnostics, accessibility, Wayland shell fit, build cost, binary/dependency risk, and migration effort.
4. The selected prototype paths for Phase 43 are narrow enough to build without replacing the production renderer.

### Phase 43: Comparable Renderer Prototype Proofs

**Goal:** Build comparable Blitz-based and MESH-owned focused-crate prototypes against the same shipped-surface slice.

**Requirements:** PROTO-01, PROTO-02, PROTO-03

**Depends on:** Phase 42

**Success criteria:**
1. A Blitz prototype renders a MESH-equivalent shipped surface or records a concrete blocker with reproduction steps.
2. A MESH-owned prototype renders the same surface slice from retained MESH data using the selected focused crates.
3. Prototype comparison uses the same inputs, expected behavior, diagnostic checks, and benchmark expectations.
4. The milestone records which prototype path advances to production proof and why.

### Phase 44: Selected Renderer Proof Integration

**Goal:** Integrate the selected proof path behind a constrained boundary while preserving existing shipped navigation/audio surface behavior.

**Requirements:** INTG-01, INTG-02, INTG-03, INTG-04

**Depends on:** Phase 43

**Success criteria:**
1. Retained node identity, typed invalidation categories, damage/profiling payloads, and non-fatal diagnostics remain visible through the proof path.
2. Existing navigation/audio surface behavior remains covered by automated tests.
3. Text layout, selection geometry, and theme-owned selection colors are tested through the selected path.
4. Accessibility metadata has an AccessKit-compatible retained-node update boundary.

### Phase 45: Renderer Migration Plan and Author Contract

**Goal:** Convert the proof result into a phased migration plan and documented renderer contract for future milestones.

**Requirements:** MIGR-01, MIGR-02, MIGR-03

**Depends on:** Phase 44

**Success criteria:**
1. Broad renderer migration is documented as phased, reversible steps.
2. Existing renderer modules are classified as authoritative, adapter-owned, or replacement candidates.
3. Build, CI, feature flag, Linux/Nix dependency, and binary-size implications are documented before broad adoption.
4. Docs explain what the renderer architecture decision means for plugin-authored `.mesh` UI and shipped shell surfaces.

## Requirement Coverage

| Requirement | Phase |
|-------------|-------|
| REND-01 | Phase 42 |
| REND-02 | Phase 42 |
| REND-03 | Phase 42 |
| PROTO-01 | Phase 43 |
| PROTO-02 | Phase 43 |
| PROTO-03 | Phase 43 |
| INTG-01 | Phase 44 |
| INTG-02 | Phase 44 |
| INTG-03 | Phase 44 |
| INTG-04 | Phase 44 |
| MIGR-01 | Phase 45 |
| MIGR-02 | Phase 45 |
| MIGR-03 | Phase 45 |

**Coverage:** 13/13 requirements mapped.

## Deferred Context

- v1.6 phases 34-36 remain paused until renderer priorities and accessibility boundaries are clear.
- The v1.5 audio popover transition delay remains accepted polish debt and is not part of the renderer architecture decision unless it naturally falls out of proof-surface work.
- Full browser compatibility, broad shell redesign, marketplace/signing, and compositor-global shortcuts remain out of scope.

---
*Roadmap created: 2026-05-18 for milestone v1.8*
