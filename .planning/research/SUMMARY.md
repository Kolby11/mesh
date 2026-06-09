# Research Summary: Event-Driven Wayland Frame Scheduler

**Domain:** Wayland frame scheduling and presentation optimization for MESH shell framework
**Researched:** 2026-06-09
**Overall confidence:** HIGH

## Executive Summary

The MESH shell framework currently uses a fixed 16ms `std::thread::sleep()` at the bottom of its main loop, burning idle CPU cycles and adding unnecessary latency. The migration to an event-driven frame scheduler requires zero new crate dependencies — every protocol object, API, and mechanism already exists in the current dependency tree (smithay-client-toolkit 0.19.2, wayland-client 0.31.14, rustix 0.38).

The core change is conceptually small: replace `std::thread::sleep(sleep_for)` with `poll()` on the Wayland connection fd, using the already-computed deadline as the timeout. The existing `dispatch_available()` method in `LayerShellBackend` already implements the full prepare_read/poll/read/dispatch loop but with a 0ms (non-blocking) timeout. Making it blocking requires only parameterizing the timeout.

Additionally, `wl_surface::set_opaque_region` can be sent from the present path using `WlCompositor::create_region()` (already bound) to tell the compositor which pixel regions are fully opaque. This is a standard Wayland optimization that compositors like Sway/Hyprland expect from well-behaved clients. The opaque rects are computed by walking the retained display list to find background fills with alpha=1.0.

Frame callbacks via `wl_surface::frame()` are already requested in `SurfaceEntry::attach_shm_buffer()` and handled in `CompositorHandler::frame()` — no new protocol bindings are needed. The scheduler simply needs to block until one arrives (or the deadline expires).

No calloop, no tokio, no async runtime. The shell loop remains synchronous and single-threaded, which is essential for deterministic rendering order, profile attribution, and frame-commit sequencing.

## Key Findings

**Stack:** All required dependencies already present (smithay-client-toolkit 0.19.2, wayland-client 0.31.14, rustix 0.38). Zero new crates needed.

**Architecture:** Add `wait_for_events(deadline)` to `PresentationEngine` → `LayerShellBackend`, called at the bottom of `Shell::run()` after all non-blocking work. Keep the existing loop order: drain → tick → render → present → block.

**Critical pitfall:** Blocking before draining the shell message channel (IPC, backends) adds up to 16ms latency to theme reloads and remote commands. The drain-messages-first ordering must be preserved.

## Implications for Roadmap

Based on research, suggested phase structure:

1. **Phase 1: Blocking Wayland Dispatch** — Replace `std::thread::sleep()` with `poll()` on Wayland fd
   - Addresses: idle CPU burn elimination, real event-driven wakeup
   - Avoids: pitfall 1 (blocking order), pitfall 6 (dev-window fallback)
   - Complexity: Low-Medium — the dispatch pattern already exists, only timeout needs parameterization

2. **Phase 2: Opaque Region Hints** — Send `wl_surface::set_opaque_region` from the present path
   - Addresses: compositor compositing optimization
   - Avoids: pitfall 3 (region leak), pitfall 4 (transparent surface false positives)
   - Complexity: Medium — requires walking retained display list to find opaque background rects

**Phase ordering rationale:**
- Phase 1 is the prerequisite: replacing `thread::sleep` enables the event-driven architecture
- Phase 2 is independent of Phase 1 but benefits from being in the same milestone (both touch the present path)
- The opaque region computation requires access to the retained display list structure — passing opaque rect metadata alongside the pixel buffer in `present_with_damage()` is the simplest integration point

**Research flags for phases:**
- Phase 1: Standard patterns, unlikely to need deeper research. The `dispatch_available()` code is well-understood and is a straightforward parameterization.
- Phase 2: Likely needs phase-specific research on how the retained display list exposes background-fill metadata. The display list structure is in `mesh_core_render` — need to verify it exposes color/alpha for filled rects.

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | All dependencies verified against Cargo.lock, SCT docs, wayland-client docs. No new crates needed. |
| Features | HIGH | Frame callbacks already wired. Blocking dispatch is a trivial timeout change. Opaque region is a standard protocol call. |
| Architecture | HIGH | Loop structure, dispatch pattern, and integration points all verified against live code. |
| Pitfalls | HIGH | Pitfalls identified from both the codebase analysis and general Wayland client patterns. Opaque-region transparency pitfall validated against protocol spec. |

## Gaps to Address

- **Retained display list opaque-rect API:** The display list in `mesh_core_render` may not currently expose per-rect alpha values for filled rectangles. Phase 2 research should verify whether the paint-backend's fill-rect data includes the fill color with alpha channel, or if an API change is needed.
- **Compositor behavior variability:** Different compositors may handle `set_opaque_region` differently (some ignore it, some require it for layer-shell). Phase 2 should include a compositor-compatibility matrix (Sway/Hyprland/KWin/Mutter) in its plan.
- **Frame callback timeout calibration:** `MAX_FRAME_CALLBACK_WAIT = 50ms` is currently used to detect stalled callbacks. With blocking dispatch, this threshold may need tuning — too low and we fall back to deadline-sleep unnecessarily; too high and we risk blocking on a dropped callback.

---
*Research complete: 2026-06-09*
