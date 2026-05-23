---
phase: 67
name: Method Call Result Lane
status: ready
---

# Phase 67 Plan: Method Call Result Lane

## Goal

Route object method calls through typed shell-managed calls and expose acknowledgements/results.

## Tasks

1. Add debug data structures for recent method calls.
2. Record service proxy dispatch acknowledgements in debug state.
3. Bridge backend `CommandResult` events into shell messages.
4. Record backend command results in debug state.
5. Update debug interface contract and shell regression tests.

## Acceptance

- `MMETH-01`: Existing proxy method syntax routes through one shell method/call lane.
- `MMETH-02`: Calls remain capability-checked, contract-checked, and provider-routed.
- `MMETH-03`: Structured success/failure data is visible beyond tracing through debug state.
- `MMETH-04`: Existing generated proxy command behavior remains intact.
