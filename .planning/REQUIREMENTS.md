# Requirements: MESH v1.9 Renderer Library Integration

Defined: 2026-05-18

## Goal

Move the selected renderer libraries from v1.8 prototype/proof evidence into production renderer paths behind reversible adapter boundaries, while preserving retained MESH identity, diagnostics, profiling, accessibility, and shipped navigation/audio behavior.

## Requirements

### Dependency And Rollout

- [ ] **LIBS-01**: Production Cargo manifests include the selected renderer-library dependencies for Taffy layout, Parley text, AnyRender or Vello-backed paint experimentation, and AccessKit runtime updates, with feature choices documented.
- [ ] **LIBS-02**: Each new renderer-library path has a feature flag, local bypass, or adapter switch that restores the current MESH renderer authority without breaking shipped surfaces.
- [ ] **LIBS-03**: Binary size, compile-time, native dependency, Linux/Nix, and CI risk are measured and documented before any library-backed path becomes default.

### Layout

- [ ] **LAYT-01**: A Taffy-backed layout adapter can compute MESH retained node geometry for the shipped navigation and audio surfaces while preserving stable node identity and runtime keys.
- [ ] **LAYT-02**: Layout parity tests compare current MESH layout output against Taffy-backed output for rows, columns, stacks, fixed sizes, gaps, padding, absolute positioning, and container-width cases.
- [ ] **LAYT-03**: Layout fallback keeps the current MESH layout engine authoritative when Taffy cannot represent a supported MESH layout case.

### Text

- [ ] **TEXT-01**: A Parley-backed text adapter shapes and lays out MESH text nodes while preserving current text content, alignment, wrapping, and measured-size behavior on shipped surfaces.
- [ ] **TEXT-02**: Parley-backed selection geometry preserves theme-owned selection colors, UTF-8 boundaries, anchor/focus evidence, and existing copy behavior.
- [ ] **TEXT-03**: Text fallback keeps the current text measurement/layout path authoritative for unsupported Parley cases and reports non-fatal diagnostics.

### Paint

- [ ] **PAINT-01**: An AnyRender/Vello-style paint adapter can execute retained display-list paint commands, or a lossless translated subset, behind the current display-list ownership boundary.
- [ ] **PAINT-02**: Paint adapter output preserves background, border, opacity, icon, text, slider/input, selection, damage, and profiling evidence on shipped navigation/audio surfaces.
- [ ] **PAINT-03**: Paint fallback keeps the current software painter authoritative when the library-backed paint path is disabled or cannot render a command.

### Accessibility

- [ ] **A11Y-01**: AccessKit runtime updates are built from retained MESH node identity rather than proof-only string evidence.
- [ ] **A11Y-02**: AccessKit-compatible updates preserve roles, labels, focusable/control metadata, and retained-node update behavior for shipped navigation/audio surfaces.

### Adoption Gates

- [ ] **GATE-01**: Workspace and targeted renderer/shell regression commands pass with library-backed paths enabled and disabled.
- [ ] **GATE-02**: Renderer ownership docs and the author contract identify which library-backed adapters are production, experimental, or deferred.

## Future Requirements

### Animations And Motion Fidelity

- **ANIM-01**: Animation and transition polish should run as the next milestone after renderer library integration.
- **ANIM-02**: Surface show/hide transitions, keyframe scheduling, transition invalidation, and visible motion fidelity should be audited against real shell surfaces.
- **ANIM-03**: Audio popover transition delay polish should be considered in the animation milestone.

### Other Deferred Work

- **MODR-01**: Module install requirement resolution remains separate from renderer library integration.
- **KEYB-01**: Paused keybind dispatch, conflict diagnostics, and accessibility proof remain separate from renderer library integration.

## Out of Scope

- Direct Blitz production adoption; v1.8 kept Blitz blocked as the production base.
- Whole-renderer rewrite; v1.9 integrates library-backed adapters behind current MESH ownership boundaries.
- Animation system redesign, transition polish, and richer keyframe behavior; these are planned for v1.10.
- Module installer/provider resolution; this remains separate pending module-system work.
- Browser DOM or web-platform compatibility; MESH keeps its shell-oriented renderer contract.

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| LIBS-01 | Phase 46 | Planned |
| LIBS-02 | Phase 46 | Planned |
| LIBS-03 | Phase 46 | Planned |
| LAYT-01 | Phase 47 | Planned |
| LAYT-02 | Phase 47 | Planned |
| LAYT-03 | Phase 47 | Planned |
| TEXT-01 | Phase 48 | Planned |
| TEXT-02 | Phase 48 | Planned |
| TEXT-03 | Phase 48 | Planned |
| PAINT-01 | Phase 49 | Planned |
| PAINT-02 | Phase 49 | Planned |
| PAINT-03 | Phase 49 | Planned |
| A11Y-01 | Phase 50 | Planned |
| A11Y-02 | Phase 50 | Planned |
| GATE-01 | Phase 50 | Planned |
| GATE-02 | Phase 50 | Planned |
