---
status: complete
phase: 29-damage-indexed-paint-execution-and-repaint-policy
source:
  - .planning/phases/29-damage-indexed-paint-execution-and-repaint-policy/29-01-SUMMARY.md
started: "2026-05-11T21:45:52+02:00"
updated: "2026-05-12T13:36:20+02:00"
---

## Current Test

[testing complete]

## Tests

### 1. Debug Paint Policy Payload
expected: Inspecting the existing debug profiling payload for a surface shows repaint-policy proof under `invalidation.paint`, including `repaint_policy`, `filtered_span_count`, `filtered_command_count`, `filtered_commands_skipped`, and `filtered_fallback_count`. The policy value is one of `minimal_damage`, `bounding_rect`, or `full_surface`.
result: issue
reported: "i dont know the debug window is kinda messy but isnt this too much of a rerendering?"
severity: major

### 2. Sparse Damage Filters Retained Commands
expected: For a partial damage case, retained paint execution uses fewer commands than the full retained command list, reports `filtered_commands_skipped > 0`, preserves the original command order among survivors, and keeps scrollbar commands with the owning span.
result: skipped
reason: "i have no way to measure his"

### 3. Full-Surface Fallback Is Explicit
expected: For a broad or ambiguous repaint case, retained paint execution keeps the full command list, reports `repaint_policy: "full_surface"`, and increments `filtered_fallback_count`.
result: pass

### 4. Canonical Benchmark Proof Remains In Existing Harness
expected: The Phase 29 benchmark proof uses the existing benchmark path and lists all five canonical scenario IDs: `hover`, `surface_open_close`, `pointer_update`, `keyboard_traversal`, and `backend_update`. It does not introduce a new benchmark harness or trace persistence path.
result: pass

## Summary

total: 4
passed: 2
issues: 1
pending: 0
skipped: 1
blocked: 0

## Gaps

- truth: "Inspecting the existing debug profiling payload for a surface shows repaint-policy proof under `invalidation.paint`, including `repaint_policy`, `filtered_span_count`, `filtered_command_count`, `filtered_commands_skipped`, and `filtered_fallback_count`. The policy value is one of `minimal_damage`, `bounding_rect`, or `full_surface`."
  status: failed
  reason: "User reported: i dont know the debug window is kinda messy but isnt this too much of a rerendering?"
  severity: major
  test: 1
  root_cause: ""
  artifacts: []
  missing: []
  debug_session: ""
