# Roadmap: MESH v1.9 Renderer Library Integration

## Milestone Goal

Move the selected renderer libraries from v1.8 prototype/proof evidence into production renderer paths behind reversible adapter boundaries, while preserving retained MESH identity, diagnostics, profiling, accessibility, and shipped navigation/audio behavior.

## Milestones

- ✅ **v1.8 Rendering Engine Architecture** — Phases 42-45 (shipped 2026-05-18)
- 🔄 **v1.9 Renderer Library Integration** — Phases 46-50 (active)

## Phases

### Phase 46: Renderer Library Dependency And Adapter Foundation

**Goal:** Add production dependency and rollout scaffolding for the selected renderer libraries without changing renderer authority by default.

**Requirements:** LIBS-01, LIBS-02, LIBS-03

**Success criteria:**
1. Production Cargo manifests include selected dependencies for Taffy, Parley, AnyRender or Vello-backed paint experimentation, and AccessKit with documented feature choices.
2. Each library-backed path has an explicit adapter switch, feature flag, or bypass that returns to the current MESH implementation.
3. Linux/Nix, binary-size, compile-time, native-dependency, and CI risks are measured and documented.
4. Existing shipped navigation/audio renderer tests still pass with all new paths disabled.

### Phase 47: Taffy Layout Adapter Integration

**Goal:** Use Taffy for production-adjacent retained layout computation while preserving MESH node identity and fallback behavior.

**Requirements:** LAYT-01, LAYT-02, LAYT-03

**Success criteria:**
1. A Taffy adapter computes retained MESH node geometry for shipped navigation and audio surfaces.
2. Stable node IDs, runtime keys, dirty categories, and retained render-object synchronization remain preserved.
3. Parity tests compare current and Taffy-backed output across rows, columns, stacks, fixed sizes, gaps, padding, absolute positioning, and container-width cases.
4. Unsupported layout cases fall back to the current MESH layout path with non-fatal diagnostics.

### Phase 48: Parley Text And Selection Integration

**Goal:** Use Parley for text shaping/layout where ready while keeping current text behavior and selection semantics intact.

**Requirements:** TEXT-01, TEXT-02, TEXT-03

**Success criteria:**
1. A Parley adapter shapes and lays out text nodes for shipped navigation/audio surfaces.
2. Current text content, alignment, wrapping, and measured-size behavior are preserved in regression tests.
3. Selection geometry preserves theme-owned selection colors, UTF-8 boundaries, anchor/focus evidence, and copy behavior.
4. Unsupported text cases fall back to the current text path and surface adapter diagnostics.

**Plans:** 2/2 plans complete

Plans:
**Wave 1**
- [x] 48-01-PLAN.md — Parley shaping adapter + diagnostics threading (TEXT-01, TEXT-03)

**Wave 2** *(blocked on Wave 1 completion)*
- [x] 48-02-PLAN.md — Parley cursor-derived selection evidence (TEXT-02)

### Phase 49: AnyRender/Vello Paint Backend Adapter

**Goal:** Introduce a library-backed paint adapter behind the retained display-list boundary while preserving software painter rollback.

**Requirements:** PAINT-01, PAINT-02, PAINT-03

**Success criteria:**
1. The paint adapter translates retained display-list commands to the selected library-backed path or to a documented lossless subset.
2. Shipped navigation/audio output preserves paint evidence for backgrounds, borders, opacity, icons, text, controls, and selection.
3. Damage, profiling, and debug payloads remain visible and comparable across current and library-backed paint paths.
4. The current software painter remains authoritative when the library-backed paint path is disabled or rejects a command.

### Phase 50: AccessKit Runtime And Broad Adoption Gates

**Goal:** Replace proof-only accessibility evidence with retained-node AccessKit runtime updates and close adoption documentation gates.

**Requirements:** A11Y-01, A11Y-02, GATE-01, GATE-02

**Success criteria:**
1. AccessKit runtime update building uses retained MESH node identity rather than proof-only string evidence.
2. Roles, labels, focusable/control metadata, and retained-node updates are proven on shipped navigation/audio surfaces.
3. Targeted renderer/shell and workspace checks pass with library-backed paths enabled and disabled.
4. Renderer ownership docs and the author contract classify each library-backed adapter as production, experimental, or deferred.

## Requirement Coverage

| Requirement | Phase | Coverage |
|-------------|-------|----------|
| LIBS-01 | Phase 46 | Dependency addition and feature choices |
| LIBS-02 | Phase 46 | Reversible adapter switches |
| LIBS-03 | Phase 46 | Build, binary, Linux/Nix, and CI risk gates |
| LAYT-01 | Phase 47 | Taffy retained layout adapter |
| LAYT-02 | Phase 47 | Layout parity tests |
| LAYT-03 | Phase 47 | Layout fallback and diagnostics |
| TEXT-01 | Phase 48 | Parley text adapter |
| TEXT-02 | Phase 48 | Selection geometry and copy parity |
| TEXT-03 | Phase 48 | Text fallback and diagnostics |
| PAINT-01 | Phase 49 | Library-backed paint adapter |
| PAINT-02 | Phase 49 | Shipped-surface paint parity |
| PAINT-03 | Phase 49 | Software painter fallback |
| A11Y-01 | Phase 50 | Retained-node AccessKit runtime updates |
| A11Y-02 | Phase 50 | Accessibility metadata proof |
| GATE-01 | Phase 50 | Enabled/disabled regression gates |
| GATE-02 | Phase 50 | Ownership and author-contract documentation |

**Coverage:** 16/16 requirements mapped.

## Progress

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 46. Renderer Library Dependency And Adapter Foundation | v1.9 | 3/3 | Complete   | 2026-05-18 |
| 47. Taffy Layout Adapter Integration | v1.9 | 3/3 | Complete    | 2026-05-18 |
| 48. Parley Text And Selection Integration | v1.9 | 2/2 | Complete   | 2026-05-19 |
| 49. AnyRender/Vello Paint Backend Adapter | v1.9 | 0/? | Planned | — |
| 50. AccessKit Runtime And Broad Adoption Gates | v1.9 | 0/? | Planned | — |

## Next

Start Phase 46 with `$gsd-discuss-phase 46`.
