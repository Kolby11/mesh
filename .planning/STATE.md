---
gsd_state_version: 1.0
milestone: v1.20
milestone_name: "Compositor Integration"
status: planning
last_updated: "2026-06-10T00:00:00.000Z"
last_activity: 2026-06-10 -- Roadmap created (3 phases, 12 requirements)
progress:
  total_phases: 3
  completed_phases: 0
  total_plans: 9
  completed_plans: 0
  percent: 0
---

# State: MESH v1.20

## Project Reference

See: `.planning/PROJECT.md` (updated 2026-06-10)

**Core value:** MESH should let plugin authors build distinctive shell UI and service integrations while the shell stays observable, deterministic, and responsive on real interaction paths.
**Current focus:** v1.20 Compositor Integration — roadmap created, ready for Phase 101

## Current Position

Phase: 101 (Per-Region Damage) — Not started
Plan: —
Status: Ready to plan Phase 101
Last activity: 2026-06-10 — Roadmap created

```
Progress [----------] 0% (0/3 phases, 0/9 plans)
```

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table.

- [v1.19]: Blocking `poll()` on Wayland fd replaces `std::thread::sleep` — zero new crate dependencies
- [v1.19]: eventfd wakes blocking poll on backend/IPC messages — sequential polling, not epoll
- [v1.19]: `supports_blocking_dispatch()` gate preserves dev-window sleep path
- [v1.19]: Conservative opaque region — root background only, alpha==255, no effects, no rounded corners
- [v1.20]: Use `org_kde_kwin_blur` (wayland-protocols-plasma 0.3) not `wp_blur_v1` — no shipping compositor implements the latter
- [v1.20]: No CPU software blur fallback — clients cannot read the compositor framebuffer; unsupported compositors render flat background
- [v1.20]: Damage rects capped at 16 per frame to bound protocol overhead
- [v1.20]: Phase order: damage → HiDPI → blur (scale must be authoritative before blur region coordinates are correct)

### Blockers/Concerns

None.

## Deferred Items

| Category | Item | Status | Deferred At |
|----------|------|--------|-------------|
| debug | phase31-live-uat-diagnosis | updated | v1.5 |
| todo | 2026-05-15-define-module-install-requirement-resolution.md | pending | v1.8 |
| todo | 2026-05-13-phase31-audio-popover-transition-delay.md | pending | v1.5 |
| todo | mesh.debug.scheduler structured payload (DIAG-02) | deferred | v1.19 |
| todo | widget-level opaque rect analysis (OPAQUE-02) | deferred | v1.19 |

## Session Continuity

Last session: 2026-06-10T00:00:00.000Z
Stopped at: Roadmap created — ready to plan Phase 101 (Per-Region Damage)
Resume file: None
