# Phase 43: Comparable Renderer Prototype Proofs - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-05-18
**Phase:** 43-comparable-renderer-prototype-proofs
**Areas discussed:** Prototype fidelity, Shared inputs, Blocker threshold, Comparison output, Todo handling

---

## Prototype Fidelity

| Option | Description | Selected |
|--------|-------------|----------|
| Structural/behavioral parity | Compare layout shape, control presence, text/icon state, and interaction flow without requiring pixel-perfect fidelity. | ✓ |
| Pixel-tight rendering | Require screenshot-level fidelity before the prototype can pass. | |
| Minimal smoke proof | Only prove each crate stack can render something. | |

**User's choice:** Runtime fallback selected the recommended option because `request_user_input` was unavailable.
**Notes:** This keeps Phase 43 useful for architecture selection without turning it into production renderer integration.

---

## Shared Inputs

| Option | Description | Selected |
|--------|-------------|----------|
| Shared scenario fixtures | Use the same navigation/audio state scenarios across both prototypes. | ✓ |
| Independent demos | Let each path choose a convenient demo. | |
| Full `.mesh` ingestion | Require both prototypes to parse and render existing `.mesh` source directly. | |

**User's choice:** Runtime fallback selected the recommended option because `request_user_input` was unavailable.
**Notes:** The MESH-owned path should use retained MESH-shaped data. The Blitz path may use equivalent HTML/CSS fixtures.

---

## Blocker Threshold

| Option | Description | Selected |
|--------|-------------|----------|
| Fast concrete blocker proof | Accept a failed Blitz render only with a reproducible harness, boundary, and reason. | ✓ |
| Force rendering at all costs | Keep adapting Blitz until it renders both surfaces. | |
| Skip Blitz if hard | Let the focused-crate path proceed without comparable Blitz evidence. | |

**User's choice:** Runtime fallback selected the recommended option because `request_user_input` was unavailable.
**Notes:** This preserves PROTO-01 while respecting Phase 42's hard blockers.

---

## Comparison Output

| Option | Description | Selected |
|--------|-------------|----------|
| Phase 44 readiness table | Compare visual/layout fidelity, interaction shape, retained identity fit, accessibility boundary, build cost, blockers, and integration readiness. | ✓ |
| Narrative only | Write prose findings without a fixed comparison structure. | |
| Benchmark-style only | Compare only measured timings and build cost. | |

**User's choice:** Runtime fallback selected the recommended option because `request_user_input` was unavailable.
**Notes:** Phase 43 should identify the path that advances to Phase 44, not design the full migration plan.

---

## Todo Handling

| Option | Description | Selected |
|--------|-------------|----------|
| Review but defer | Keep the audio popover transition delay as accepted polish debt, while allowing prototype evidence to mention it if relevant. | ✓ |
| Fold into Phase 43 | Make transition delay improvement part of this phase. | |
| Ignore entirely | Do not mention the todo in context. | |

**User's choice:** Runtime fallback selected the recommended option because `request_user_input` was unavailable.
**Notes:** Fixing the delay requires shell-owned transition lifecycle work, which is outside Phase 43's throwaway prototype scope.

---

## the agent's Discretion

- Selected all key gray areas because the structured question tool was unavailable.
- Chose conservative defaults that preserve Phase 42's hard boundaries and keep Phase 43 from becoming production integration.

## Deferred Ideas

- Audio popover transition delay polish remains deferred as accepted Phase 31 polish debt.
