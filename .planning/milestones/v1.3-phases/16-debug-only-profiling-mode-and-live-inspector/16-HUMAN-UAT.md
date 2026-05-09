---
status: passed
phase: 16-debug-only-profiling-mode-and-live-inspector
source: [16-VERIFICATION.md]
started: 2026-05-08T18:59:16Z
updated: 2026-05-08T19:20:00Z
---

## Current Test Status

Completed after live-shell validation and follow-up fix for the right-side inspector Wayland anchor configuration.

## Tests

### 1. Debug-path inspector interaction remains explicit and familiar
expected: Starting the existing debug overlay shows the right-side inspector, and toggling profiling changes profiling state without auto-opening or auto-closing the inspector.
result: passed
notes: The initial live run exposed a Wayland layer-shell protocol error for the right-edge inspector surface. After the anchor fix in `crates/core/ui/render/src/surface/bridge/wayland_surface/backend.rs`, the inspector opens on the debug path and profiling remains independently toggleable.

### 2. All four inspector views stay legible with zero samples and live samples
expected: Overview, Surfaces, Backend services, and Benchmark remain readable in the narrow panel, zero-state copy is clear, and live values populate without layout breakage.
result: passed
notes: Manual shell validation proceeded after the anchor fix; the inspector could be opened and used as intended, which clears the previously blocked live-view check for phase closure.

## Summary

total: 2
passed: 2
issues: 0
pending: 0
skipped: 0
blocked: 0
 
## Gaps

None
