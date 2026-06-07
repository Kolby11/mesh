# Requirements: MESH v1.18 Performance: Smart Invalidation

**Milestone:** v1.18
**Goal:** Replace coarse "tree rebuild + full repaint" invalidation with typed dependency tracking so interaction state, service events, and script state changes only dirty the affected nodes and paint slots.
**Defined:** 2026-06-07

---

## v1 Requirements

### Selector Dependency Tracking

- [ ] **SEL-01**: `StyleRuleIndex` builds per-rule dependency masks recording which pseudo-class/state bits each rule depends on.
- [ ] **SEL-02**: A reverse index maps state-bit changes to affected rule indices for O(1) lookup instead of full rule iteration.
- [ ] **SEL-03**: `:hover`, `:focus`, and `:active` pseudo-class transitions restyle only nodes whose matched rules' dependency sets intersect the changed state.
- [ ] **SEL-04**: Inherited style values (color, font-family, font-size, font-weight, line-height) propagate correctly to children of restyled nodes.
- [ ] **SEL-05**: Existing shipped-surface regression tests pass with selector-narrow restyle enabled (navigation bar, audio popover).

### Per-Node Service Dependency Tracking

- [ ] **SRV-01**: During render, the template evaluator records per-node service field reads ((service, field) pairs per NodeId).
- [ ] **SRV-02**: A bidirectional `NodeServiceFieldDependencies` index supports both "which nodes read field X" and "which fields does node Y read" queries.
- [ ] **SRV-03**: Per-node field tracking overhead is below 1% of total render pass time on shipped surfaces.

### Narrow Invalidation & Event Routing

- [ ] **INV-01**: Simple text/value script state changes dirty only the affected leaf nodes plus their layout ancestor chain, not `TREE_REBUILD`.
- [ ] **INV-02**: Service events fan out only to components whose tracked field sets intersect the changed fields.
- [ ] **INV-03**: `TREE_REBUILD` fallback activates when >50% of nodes are affected, preserving correctness for bulk changes.
- [ ] **INV-04**: Profiling payloads show reduced dirty-node counts and retained-tree churn across canonical benchmarks (hover, open/close, slider, traversal, backend-update).
- [ ] **INV-05**: Pixel-identical output on all benchmark scenarios (equivalence testing against pre-invalidation baseline rendering).

---

## Future Requirements

Deferred beyond v1.18. Tracked but not in current roadmap.

### Direct Mutation Fast Path

- **FAST-01**: Text/content value updates that don't alter element structure skip `build_tree_with_state()` entirely.
- **FAST-02**: Per-property dirty categories (STYLE_VISUAL vs STYLE_FULL) for partial restyle optimization.

### Nested Service Fields

- **NEST-01**: Recursive proxy wrapping for nested service field tracking (`playback.title` vs `playback.artist` distinguishability).

---

## Out of Scope

| Feature | Reason |
|---------|--------|
| Salsa/incremental-computation libraries | Overhead and complexity not justified for MESH's bounded widget trees (~200 nodes per surface) |
| Static analysis of Luau scripts | Metatable proxy approach is simpler and works with existing dynamic Luau semantics |
| GPU rendering changes | v1.18 is CPU-side invalidation only; GPU remains deferred |
| Event-driven frame scheduler (v1.19) | Separate milestone — removes 16ms sleep cap, not invalidation narrowing |
| Per-property dirty categories | Complexity-to-value ratio too high for MESH's ~50-property CSS subset |

---

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| SEL-01 | Phase 96 | Pending |
| SEL-02 | Phase 96 | Pending |
| SEL-03 | Phase 96 | Pending |
| SEL-04 | Phase 96 | Pending |
| SEL-05 | Phase 96 | Pending |
| SRV-01 | Phase 97 | Pending |
| SRV-02 | Phase 97 | Pending |
| SRV-03 | Phase 97 | Pending |
| INV-01 | Phase 98 | Pending |
| INV-02 | Phase 98 | Pending |
| INV-03 | Phase 98 | Pending |
| INV-04 | Phase 98 | Pending |
| INV-05 | Phase 98 | Pending |

**Coverage:**
- v1 requirements: 13 total
- Mapped to phases: 13 ✓
- Unmapped: 0

---

*Requirements defined: 2026-06-07*
*Last updated: 2026-06-07 after research and scoping*
