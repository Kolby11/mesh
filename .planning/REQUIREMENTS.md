# Requirements: MESH

**Defined:** 2026-05-18
**Core Value:** MESH should let plugin authors build distinctive shell UI and service integrations while the shell stays observable, deterministic, and responsive on real interaction paths.

## v1.8 Requirements

### Architecture Decision

- [x] **REND-01**: The project has a source-backed adopt-vs-build decision for Blitz that compares direct adoption, architecture borrowing, and a MESH-owned renderer path.
- [x] **REND-02**: The decision scorecard covers determinism, retained invalidation, profiling, diagnostics, accessibility, Wayland shell fit, build cost, binary/dependency risk, and migration effort.
- [x] **REND-03**: Blitz, Skia/rust-skia, Stylo, Taffy, Parley, AnyRender, Winit, AccessKit, Muda, html5ever, and xml5ever each have an explicit accept/defer/reject outcome for v1.8.

### Prototype Proofs

- [x] **PROTO-01**: A Blitz-based prototype renders a MESH-equivalent shipped surface or clearly documents why direct Blitz adoption is blocked.
- [x] **PROTO-02**: A MESH-owned focused-crate prototype renders the same surface slice using retained MESH data with candidate layout/text/paint crates.
- [x] **PROTO-03**: Prototype comparison records behavior and cost using the same surface, inputs, benchmark expectations, and diagnostic criteria.

### Renderer Integration

- [x] **INTG-01**: The selected proof path preserves MESH retained node identity, typed invalidation categories, damage/profiling payloads, and non-fatal diagnostics.
- [x] **INTG-02**: The selected proof path preserves current shipped navigation/audio surface behavior under automated tests.
- [x] **INTG-03**: Text layout, selection geometry, and theme-owned selection colors remain testable through the selected path.
- [x] **INTG-04**: Accessibility metadata remains derivable from retained nodes and has a clear AccessKit-compatible update boundary.

### Migration Plan

- [ ] **MIGR-01**: The roadmap for broad renderer migration is documented as phased, reversible steps instead of a whole-renderer rewrite.
- [ ] **MIGR-02**: The migration plan identifies which existing renderer modules stay authoritative, which become adapters, and which are candidates for replacement.
- [ ] **MIGR-03**: Build, CI, feature flags, and Linux/Nix dependency implications are documented before any broad adoption.

## Future Requirements

### Broad Renderer Rollout

- **FUTR-01**: Replace or substantially extend the production renderer after the v1.8 proof path is accepted.
- **FUTR-02**: Add richer HTML/XHTML import or authoring support if html5ever/xml5ever prove valuable.
- **FUTR-03**: Expand text editing, IME, and complex text behavior beyond the current shipped-surface proof.

### Platform Integration

- **FUTR-04**: Revisit Winit/Muda only if MESH needs app-window or native-menu behavior beyond current Wayland shell surfaces.
- **FUTR-05**: Resume paused v1.6 keybind dispatch/conflict/accessibility work after the renderer architecture decision is stable.

## Out of Scope

| Feature | Reason |
|---------|--------|
| Full browser engine compatibility | MESH is a shell UI framework, not a general browser; full HTML/CSS/JS compatibility would swamp the renderer decision. |
| Whole-renderer replacement in one milestone | Too much risk to retained invalidation, profiling, diagnostics, text, accessibility, and shipped-surface behavior. |
| Compositor-global shortcuts | Still belongs to the paused keybind milestone, not renderer architecture. |
| Remote marketplace or signing | Orthogonal to renderer architecture. |
| New broad shell UI redesign | v1.8 proves renderer architecture on existing surfaces first. |

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| REND-01 | Phase 42 | Complete |
| REND-02 | Phase 42 | Complete |
| REND-03 | Phase 42 | Complete |
| PROTO-01 | Phase 43 | Complete |
| PROTO-02 | Phase 43 | Complete |
| PROTO-03 | Phase 43 | Complete |
| INTG-01 | Phase 44 | Complete |
| INTG-02 | Phase 44 | Complete |
| INTG-03 | Phase 44 | Complete |
| INTG-04 | Phase 44 | Complete |
| MIGR-01 | Phase 45 | Pending |
| MIGR-02 | Phase 45 | Pending |
| MIGR-03 | Phase 45 | Pending |

**Coverage:**
- v1.8 requirements: 13 total
- Mapped to phases: 13
- Unmapped: 0

---
*Requirements defined: 2026-05-18*
*Last updated: 2026-05-18 after v1.8 milestone definition*
