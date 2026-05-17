# Roadmap: MESH v1.7 Rethink Modularity and Extensibility Concepts

**Milestone goal:** Rework MESH's modularity model so frontend modules, backend providers, manifests, service contracts, capabilities, and extension points form a coherent author-facing architecture instead of a set of separate milestone-grown mechanisms.

**Starting phase:** Phase 37, continuing from v1.6's planned Phase 36.

## Phases

### Phase 37: Concept Inventory and Vocabulary Lock

**Goal:** Define the canonical module/extensibility vocabulary and map every current term to runtime structures, docs, diagnostics, canonical replacements, or internal-only migration paths.

**Requirements:** CONC-01, CONC-02, CONC-03

**Depends on:** v1.6 Phase 33 context

**Success criteria:**
1. Canonical definitions exist for module identity, frontend, backend provider, interface, library, resource pack, contribution, capability, and dependency.
2. Existing docs and code terms are inventoried with canonical replacements, removal targets, or internal-only migration paths.
3. v1.1 backend provider decisions and v1.6 keybind declaration/resolution decisions are reconciled into the vocabulary.
4. Diagnostics and docs have a clear target vocabulary for later phases.

### Phase 38: Canonical Manifest Normalization

**Goal:** Align package manifest parsing and runtime manifest normalization around the canonical `package.json.mesh` contract while preserving existing behavior.

**Requirements:** MAN-01, MAN-02, MAN-03

**Depends on:** Phase 37

**Success criteria:**
1. The canonical manifest schema is documented and represented in Rust normalization code.
2. Legacy manifest forms load through explicit compatibility paths without silently dropping supported data.
3. v1.6 keybind declarations and v1.1 provider declarations survive normalization.
4. Manifest validation emits field-path diagnostics with actionable migration guidance.

### Phase 39: Contribution and Interface Extension Index

**Goal:** Make extension points inspectable through typed installed-graph contributions and contract-aware interface/provider validation.

**Requirements:** EXT-01, EXT-02, EXT-03, EXT-04

**Depends on:** Phase 38

**Success criteria:**
1. Interface relationship metadata supports base, extension, and independent contracts.
2. Provider declarations, interface dependencies, and host capability requests are validated as separate concepts.
3. Installed graph contribution indexing covers frontend entrypoints, slots, libraries, settings, keybinds, resources, interfaces, and providers.
4. Tests prove new extension behavior routes through manifests, contracts, libraries, and providers without service-specific Rust branches.

### Phase 40: Compatibility Migration and Author Diagnostics

**Goal:** Turn compatibility behavior into visible, author-facing migration guidance across bundled docs, examples, and diagnostics.

**Requirements:** MIGR-01, MIGR-02

**Depends on:** Phase 38, Phase 39

**Success criteria:**
1. Legacy terminology and manifest shapes in bundled modules/docs are updated or explicitly marked as compatibility.
2. Diagnostics distinguish blocking load errors from migration warnings.
3. Existing v1.6 keybind declaration/resolution data remains addressable under the canonical contribution model.
4. Module authors have a documented migration path from old examples to the new package model.

### Phase 41: Shipped Module Proof and Documentation

**Goal:** Prove the consolidated model on a real bundled module/provider path with tests, docs, and diagnostics.

**Requirements:** PROOF-01

**Depends on:** Phase 39, Phase 40

**Success criteria:**
1. A bundled module/provider path uses the canonical manifest and contribution model end to end.
2. The proof includes interface/provider/library/resource or settings/keybind behavior without adding service-specific Rust APIs.
3. Automated tests cover manifest normalization, contribution indexing, diagnostics, and proof-module behavior.
4. Author documentation shows the final workflow for adding or extending a MESH module.

## Requirement Coverage

| Requirement | Phase |
|-------------|-------|
| CONC-01 | Phase 37 |
| CONC-02 | Phase 37 |
| CONC-03 | Phase 37 |
| MAN-01 | Phase 38 |
| MAN-02 | Phase 38 |
| MAN-03 | Phase 38 |
| EXT-01 | Phase 39 |
| EXT-02 | Phase 39 |
| EXT-03 | Phase 39 |
| EXT-04 | Phase 39 |
| MIGR-01 | Phase 40 |
| MIGR-02 | Phase 40 |
| PROOF-01 | Phase 41 |

**Coverage:** 13/13 requirements mapped.

## Deferred Context

- v1.6 phases 34-36 are paused, not shipped. Resume keybind dispatch/conflict/accessibility work after v1.7 stabilizes the module model.
- The slight audio popover transition delay from v1.5 remains accepted polish debt: `.planning/todos/pending/2026-05-13-phase31-audio-popover-transition-delay.md`.
- Marketplace, signing, remote distribution, installer UX, compositor-global shortcuts, and Skia remain future milestones.

---
*Roadmap created: 2026-05-15 for milestone v1.7*
