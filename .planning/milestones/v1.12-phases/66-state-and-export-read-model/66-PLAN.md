---
phase: 66
name: State And Export Read Model
status: ready
---

# Phase 66 Plan: State And Export Read Model

## Goal

Make backend `state` and frontend `exports` durable, replayable object fields.

## Tasks

1. Add a JSON snapshot helper for `ScriptState`.
2. Install a Luau `module` object with `module.state` and `module.exports`.
3. Mirror `module.exports` into reactive state and refresh `module.state` after script/handler/host mutations.
4. Cache latest service payloads in frontend components and seed newly-created runtimes before script execution when capabilities allow.
5. Add focused scripting tests for `module.state` and `module.exports`.

## Acceptance

- `MSTATE-01`: Backend service state remains available through canonical proxy state.
- `MSTATE-02`: Frontend scripts can expose public values through `module.exports`.
- `MSTATE-03`: New runtimes can receive cached backend service payloads before script execution.
- `MSTATE-04`: Compatibility state aliases remain intact.
