# Phase 16: Debug-Only Profiling Mode and Live Inspector - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-05-08
**Phase:** 16-Debug-Only Profiling Mode and Live Inspector
**Areas discussed:** Activation model, Inspector host surface, Benchmark-view boundary

---

## Activation model

| Option | Description | Selected |
|--------|-------------|----------|
| Existing debug path + explicit profiling toggle/mode | Matches the locked Phase 14/15 debug-only rule without inventing a new diagnostics path | ✓ |
| Auto-switch into profiling views when profiling is enabled | Reuses the debug path but changes the default viewing behavior | |
| Separate profiling-only command/path | Simpler conceptually but conflicts with the existing-debug-path rule | |

**User's choice:** Existing debug path with an explicit profiling toggle/mode.
**Notes:** Follow-up decisions locked that enabling profiling does not auto-open the inspector, does not auto-switch views, and remains active for the current shell session until explicitly turned off.

---

## Inspector host surface

| Option | Description | Selected |
|--------|-------------|----------|
| Keep native overlay host, render `.mesh` inspector content inside it | Lowest-risk way to satisfy `.mesh` rendering requirement | |
| Separate profiling inspector shell surface/popover | Gives more room but adds surface lifecycle complexity | |
| Replace the native debug panel with a `.mesh`-driven inspector | Larger change, but makes the inspector a real frontend surface rather than a native diagnostics widget | ✓ |

**User's choice:** Replace the native debug panel with a `.mesh`-driven inspector, while keeping the familiar right-side panel layout.
**Notes:** User clarified that the inspector should be shell-owned by distribution but work like a normal frontend `.mesh` component/module. Its only special capability is access to shell-exposed debug endpoints, and in principle user-authored modules should be able to build the same panel using the same API. Follow-up decision locked the inspector as an internal core frontend module/package loaded when debug mode is active.

---

## Benchmark-view boundary

| Option | Description | Selected |
|--------|-------------|----------|
| Scaffold the benchmark/interaction view now | Lets Phase 16 ship the view architecture while keeping actual repeatable benchmark flows for Phase 17 | ✓ |
| Add a real interactive benchmark launcher now | More useful immediately, but starts collapsing the Phase 16/17 boundary | |
| Defer the benchmark view entirely | Simpler, but conflicts with the required view set | |

**User's choice:** Ship the benchmark/interaction view as a scaffold in Phase 16.
**Notes:** Follow-up decisions locked that the scaffold should define the future benchmark categories and what each measures, and should include hover, surface open/close, slider or pointer-driven update, keyboard traversal, and backend-driven update.

---

## the agent's Discretion

- Exact inspector view-navigation UX inside the right-side panel.
- Exact endpoint/API shape used by the core-shipped inspector module.
- Exact internal packaging and load mechanism for the debug-only core frontend module/package.
- Exact empty-state design for sparse profiling and benchmark data.

## Deferred Ideas

- Turn the scaffolded benchmark view into a repeatable benchmark launcher/proof surface in Phase 16.
- Add profiling trace capture, replay, or persistent storage.
- Generalize the `.mesh` replacement into a whole-debug-system rewrite beyond the profiling/debug inspector surface.
