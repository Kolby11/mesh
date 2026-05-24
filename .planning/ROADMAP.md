# Roadmap: MESH

## Milestones

- ✅ **v1.13 Manifest I18n Contract** — Phases 70-73 shipped 2026-05-24 ([archive](milestones/v1.13-ROADMAP.md))
- ✅ **v1.12 Module Object Contract** — Phases 65-69 shipped 2026-05-23 ([archive](milestones/v1.12-ROADMAP.md))
- ✅ **v1.11 Surface Keybind Completion** — Phases 60-64 shipped 2026-05-23 ([archive](milestones/v1.11-ROADMAP.md))
- ✅ **v1.10 Painter Engine** — Phases 51-59 shipped 2026-05-23 ([archive](milestones/v1.10-ROADMAP.md))
- ✅ **v1.9 Renderer Library Integration** — Phases 46-50 shipped 2026-05-21 ([archive](milestones/v1.9-ROADMAP.md))

## Phases

<details>
<summary>✅ v1.13 Manifest I18n Contract (Phases 70-73) — SHIPPED 2026-05-24</summary>

- [x] Phase 70: Localized Text Manifest Model (1/1 plans) — completed 2026-05-24
- [x] Phase 71: Contribution Propagation (1/1 plans) — completed 2026-05-24
- [x] Phase 72: Runtime Text Resolution (1/1 plans) — completed 2026-05-24
- [x] Phase 73: Shipped Manifest I18n Proof (1/1 plans) — completed 2026-05-24

</details>

## Progress

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 70. Localized Text Manifest Model | v1.13 | 1/1 | Complete | 2026-05-24 |
| 71. Contribution Propagation | v1.13 | 1/1 | Complete | 2026-05-24 |
| 72. Runtime Text Resolution | v1.13 | 1/1 | Complete | 2026-05-24 |
| 73. Shipped Manifest I18n Proof | v1.13 | 1/1 | Complete | 2026-05-24 |

## Backlog

### Future: Language Pack Namespaces

Cross-module language-pack references remain future work. v1.13 kept the
localized text object extensible enough for a later explicit namespace field or
`@module:key` syntax.

### Future: Keybind Settings UI

A full user-facing remapping UI remains deferred. v1.13 prepared resolved
metadata so a later settings UI can display localized labels without changing
stable action ids.

### Future: Compositor-Global Shortcuts

Compositor-global shortcuts remain deferred until focused-surface declarations,
localized metadata, and module object contracts are stable.
