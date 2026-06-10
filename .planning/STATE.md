---
gsd_state_version: 1.0
milestone: v1.20
milestone_name: Compositor Integration
status: executing
stopped_at: Completed 102-01-PLAN — Scale factor acquisition wired, ready for Plan 02
last_updated: "2026-06-10T15:27:13.690Z"
last_activity: 2026-06-10 — Plan 01 executed (scale: f32 on SurfaceEntry, wp_fractional_scale_v1 bound)
progress:
  total_phases: 3
  completed_phases: 1
  total_plans: 9
  completed_plans: 2
  percent: 22
---

# State: MESH v1.20

## Project Reference

See: `.planning/PROJECT.md` (updated 2026-06-10)

**Core value:** MESH should let plugin authors build distinctive shell UI and service integrations while the shell stays observable, deterministic, and responsive on real interaction paths.
**Current focus:** v1.20 Compositor Integration — Phase 102 HiDPI, Plan 02 remaining

## Current Position

Phase: 102 (HiDPI / Fractional Scale) — In Progress
Plan: 01 (Scale Factor Acquisition) — Complete
Next: Plan: 02 (Physical Pixel Pipeline)
Status: Plan 01 complete; ready for Plan 02
Last activity: 2026-06-10 — Plan 01 executed (scale: f32 on SurfaceEntry, wp_fractional_scale_v1 bound)

```
Progress [===-------] 22% (1/3 phases, 2/9 plans)
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
- [v1.20]: protocol_damage_rects helper extracted as pure function for testability
- [v1.20]: SHM copy region stays unioned; per-rect damage_buffer only for protocol calls
- [v1.20]: wp_viewporter lives in wayland-protocols::wp::viewporter, not wayland-protocols-wlr
- [v1.20]: fractional_scale requires staging feature flag on wayland-protocols
- [v1.20]: Scale factor handlers clamp values: 1..=3 (integer) and 60..=480 (fractional) per threat model

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

Last session: 2026-06-10T15:18:22Z
Stopped at: Completed 102-01-PLAN — scale factor acquisition wired for HiDPI
Resume file: .planning/phases/102-hidpi-fractional-scale/102-01-SUMMARY.md
