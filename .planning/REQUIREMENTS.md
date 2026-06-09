# Requirements: MESH

**Defined:** 2026-06-09
**Core Value:** MESH should let plugin authors build distinctive shell UI and service integrations while the shell stays observable, deterministic, and responsive on real interaction paths.

## v1.19 Requirements

Requirements for v1.19 Performance: Event-Driven Frame Scheduler. Each maps to roadmap phases.

### Scheduler

- [ ] **SCHED-01**: Shell blocks on Wayland connection fd via `poll()` with deadline from `next_runtime_sleep()` instead of `std::thread::sleep` when running the Wayland surface backend
- [ ] **SCHED-02**: Backend/command message senders signal an eventfd to wake the blocking Wayland poll, preventing stale-frame latency on IPC events
- [ ] **SCHED-03**: Shell loop preserves drain-first ordering: drain shell messages → tick → render → present → block
- [ ] **SCHED-04**: Dev-window backend (minifb) retains its existing sleep path — deadline-driven blocking applies to the Wayland surface backend only

### Opaque Region

- [ ] **OPAQUE-01**: Present path walks the retained display list to find fully-opaque root background rects, computes their union as a `wl_region`, and sends `wl_surface::set_opaque_region`

### Diagnostics

- [ ] **DIAG-01**: Profiling infrastructure records `ProfilingStage::SchedulerIdle` with block duration and wake reason, visible in debug inspector

## v2 Requirements

Deferred to future release. Tracked but not in current roadmap.

### Diagnostics

- **DIAG-02**: Structured `mesh.debug.scheduler` payload exposing per-iteration block/wake duration, wake reason, and frame-callback latency for compositor compatibility testing

### Opaque Region

- **OPAQUE-02**: Conservative opaque-region gating — skip `set_opaque_region` when surface has border-radius > 0, surface alpha < 1.0, box-shadows, backdrop-filters, or effects that may produce partial transparency

## Out of Scope

| Feature | Reason |
|---------|--------|
| tokio-based async event loop | Shell loop is synchronous by design for deterministic rendering order and profile attribution |
| calloop integration | Existing `rustix::event::poll` is simpler and already tested; no need for event-loop rewrite |
| Compositor-specific opaque region hints | Standard `wl_surface::set_opaque_region` works on all compliant compositors |
| Multi-fd epoll integration | Only needed if MESH adds timerfd or additional socket sources beyond the Wayland connection + eventfd |
| Per-surface fd blocking | All MESH surfaces share one Wayland connection — single fd poll suffices |
| wp_presentation feedback | Deferred until frame-scheduler stability is proven; not needed for idle CPU elimination |
| Automatic opaque-region transparency detection | Conservatively only claims root backgrounds; full widget-level transparency analysis is future work |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| SCHED-01 | — | Pending |
| SCHED-02 | — | Pending |
| SCHED-03 | — | Pending |
| SCHED-04 | — | Pending |
| OPAQUE-01 | — | Pending |
| DIAG-01 | — | Pending |

**Coverage:**
- v1.19 requirements: 6 total
- Mapped to phases: 0
- Unmapped: 6 ⚠️

---

*Requirements defined: 2026-06-09*
*Last updated: 2026-06-09 after initial definition*
