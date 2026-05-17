# Requirements: MESH

**Defined:** 2026-05-15
**Core Value:** MESH should let plugin authors build distinctive shell UI and service integrations while the shell stays observable, deterministic, and responsive on real interaction paths.

## v1.7 Requirements

### Conceptual Model

- [ ] **CONC-01**: Module authors can read one canonical vocabulary that distinguishes package/module identity, frontend surface, backend provider, interface contract, library, resource pack, contribution, capability, and dependency.
- [ ] **CONC-02**: Runtime structs, diagnostics, docs, and examples use the same canonical names, with old names handled only as replacement debt or internal-only migration paths until removal.
- [ ] **CONC-03**: Existing v1.1 and v1.6 decisions are reconciled into the model so keybind declarations, provider selection, interfaces, and installed-module graph behavior do not contradict each other.

### Manifest Contract

- [ ] **MAN-01**: New modules can use a documented canonical `package.json` plus `mesh` schema for identity, dependencies, capabilities, entrypoints, contributions, interfaces, providers, settings, keybinds, assets, i18n, and compatibility metadata.
- [ ] **MAN-02**: The runtime normalizes legacy manifest shapes into the canonical model without losing supported v1.1 backend provider behavior or v1.6 keybind declaration/resolution data.
- [ ] **MAN-03**: Invalid, deprecated, duplicate, or ambiguous manifest fields produce actionable author-facing diagnostics with module id, field path, severity, and suggested migration.

### Extensibility Contracts

- [ ] **EXT-01**: Interface modules can declare base, extension, and independent relationships that tools and diagnostics can use for discoverability without blocking independent interfaces.
- [ ] **EXT-02**: Backend provider declarations, interface dependencies, and capability requests remain separate concepts with validation that prevents host powers from being inferred by provider identity.
- [ ] **EXT-03**: Frontend entrypoints, slots, libraries, settings, keybinds, theme/icon/font/language resources, and provider/interface declarations are indexed as typed contributions in the installed module graph.
- [ ] **EXT-04**: A new or refactored module path can add interface/provider/library/resource behavior without adding service-specific Rust branches.

### Migration and Proof

- [ ] **MIGR-01**: Existing bundled modules and docs that still use legacy vocabulary or manifest shapes receive a clear migration path toward the canonical model.
- [ ] **MIGR-02**: Paused v1.6 keybind declaration/resolution work is preserved as part of the manifest/contribution model so later keybind dispatch phases can resume without rework.
- [ ] **PROOF-01**: At least one real bundled module/provider path proves the canonical model through code, docs, diagnostics, and tests.

## Future Requirements

### Distribution and Tooling

- **DIST-01**: Module marketplace, signing, trust policy, and remote distribution workflow.
- **DIST-02**: Installer UX for native dependencies, optional permissions, and package provenance.
- **TOOL-01**: Full schema-driven author tooling, including generated docs, editor completions, and contract-derived Luau type packages.

### Keybind Completion

- **KEYB-01**: Resume v1.6 script dispatch, conflict diagnostics, override safety, accessibility metadata, and shipped-surface keybind proof after the module model is stable.

### Platform Integration

- **PLAT-01**: Compositor-global shortcuts through XDG Desktop Portal or compositor-specific APIs.
- **PLAT-02**: Skia-backed rendering investigation.

## Out of Scope

| Feature | Reason |
|---------|--------|
| Remote marketplace or package signing | Requires distribution policy and trust UX beyond the conceptual/runtime contract consolidation. |
| Full installer UX | This milestone may improve diagnostics, but package installation flows are separate product work. |
| Compositor-global shortcuts | v1.7 is about module contracts; global shortcut permission/session behavior remains future platform work. |
| Completing all paused keybind runtime phases | v1.7 preserves keybind declarations in the module model, but dispatch/conflict/accessibility proof can resume afterward. |
| Service-specific Rust APIs | Direct audio/network/power APIs would undermine the extensibility goal. |
| Skia-backed rendering | Rendering backend work remains separate from modularity and extensibility concepts. |

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| CONC-01 | Phase 37 | Pending |
| CONC-02 | Phase 37 | Pending |
| CONC-03 | Phase 37 | Pending |
| MAN-01 | Phase 38 | Pending |
| MAN-02 | Phase 38 | Pending |
| MAN-03 | Phase 38 | Pending |
| EXT-01 | Phase 39 | Pending |
| EXT-02 | Phase 39 | Pending |
| EXT-03 | Phase 39 | Pending |
| EXT-04 | Phase 39 | Pending |
| MIGR-01 | Phase 40 | Pending |
| MIGR-02 | Phase 40 | Pending |
| PROOF-01 | Phase 41 | Pending |

**Coverage:**
- v1.7 requirements: 13 total
- Mapped to phases: 13
- Unmapped: 0

---
*Requirements defined: 2026-05-15*
*Last updated: 2026-05-15 after milestone v1.7 definition*
