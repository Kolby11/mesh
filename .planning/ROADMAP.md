# Roadmap: MESH

## Milestones

- [x] **v1.11 Surface Keybind Completion** - Phases 60-64 shipped 2026-05-23
- [x] **v1.10 Painter Engine** - Phases 51-59 shipped 2026-05-23
- [x] **v1.9 Renderer Library Integration** - Phases 46-50 shipped 2026-05-21
- [x] **v1.8 Rendering Engine Architecture** - Phases 42-45 shipped 2026-05-18

## Phases

<details>
<summary>v1.11 Surface Keybind Completion (Phases 60-64) - SHIPPED 2026-05-23</summary>

- [x] Phase 60: Surface Keybind Dispatch Runtime (1/1 plan) - completed 2026-05-23
- [x] Phase 61: Localized Resolution And Override Safety (1/1 plan) - completed 2026-05-23
- [x] Phase 62: Conflict And Invalid-Keybind Diagnostics (1/1 plan) - completed 2026-05-23
- [x] Phase 63: Accessibility Metadata And Observability (1/1 plan) - completed 2026-05-23
- [x] Phase 64: Shipped Surface Keybind Proof (1/1 plan) - completed 2026-05-23

</details>

## Progress

| Phase | Milestone | Plans Complete | Status | Completed |
|---|---|---:|---|---|
| 60. Surface Keybind Dispatch Runtime | v1.11 | 1/1 | Complete | 2026-05-23 |
| 61. Localized Resolution And Override Safety | v1.11 | 1/1 | Complete | 2026-05-23 |
| 62. Conflict And Invalid-Keybind Diagnostics | v1.11 | 1/1 | Complete | 2026-05-23 |
| 63. Accessibility Metadata And Observability | v1.11 | 1/1 | Complete | 2026-05-23 |
| 64. Shipped Surface Keybind Proof | v1.11 | 1/1 | Complete | 2026-05-23 |

## Backlog

### Future: Compositor-Global Shortcuts

Compositor-global shortcuts remain deferred until focused-surface keybind semantics are stable. Future work needs platform/session permission design, diagnostics separate from focused surfaces, and likely XDG Desktop Portal or compositor-specific integration.

### Future: Keybind Settings UI

A full user-facing remapping UI remains deferred. v1.11 validates override schema and runtime behavior so a later settings surface can safely inspect and modify overrides.

### Future: Generated Access Keys

Automatic locale-aware access-key generation remains deferred. v1.11 keeps authors responsible for explicit localized trigger defaults.
