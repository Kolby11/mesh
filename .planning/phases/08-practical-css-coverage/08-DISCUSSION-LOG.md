# Phase 8: Practical CSS Coverage - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-05-05
**Phase:** 8-Practical CSS Coverage
**Areas discussed:** CSS support boundary, Shorthands and computed style model, Tokens and variables, Unsupported CSS diagnostics, Parser/resolver/docs boundaries

---

## CSS Support Boundary

| Option | Description | Selected |
|--------|-------------|----------|
| Practical shell subset | Support common CSS used for shell UI while documenting unsupported browser features. | yes |
| Full browser compatibility | Attempt broad CSS compatibility including grid/floats/arbitrary at-rules. | |
| Minimal incremental additions | Add only the next missing property as discovered. | |

**User's choice:** Practical shell subset, inferred from the user statement that MESH should support most common CSS attributes but avoid unnecessary ones.
**Notes:** The milestone is about expressive shell styling, not implementing a browser engine.

---

## Shorthands and Computed Style Model

| Option | Description | Selected |
|--------|-------------|----------|
| Lower shorthands into `ComputedStyle` | Normalize useful CSS syntax into the existing renderer style model. | yes |
| Preserve raw CSS object model | Keep browser-like declarations around for later cascade/layout behavior. | |
| Longhands only | Avoid shorthand support and require authors to be verbose. | |

**User's choice:** Lower shorthands into `ComputedStyle`.
**Notes:** This fits the existing architecture and gives authors normal-looking CSS without requiring browser-style internal machinery.

---

## Tokens and Variables

| Option | Description | Selected |
|--------|-------------|----------|
| Tokens plus small CSS variable support | Keep `token(...)` first-class and add practical `var(...)` resolution for supported properties. | yes |
| Tokens only | Continue relying only on theme tokens. | |
| Full browser custom property cascade | Recreate full CSS custom property inheritance and fallback semantics. | |

**User's choice:** Tokens plus small CSS variable support.
**Notes:** Theme tokens remain central, but local variables are useful for shell styling ergonomics.

---

## Unsupported CSS Diagnostics

| Option | Description | Selected |
|--------|-------------|----------|
| Visible diagnostics | Unknown/unsupported properties surface diagnostics or warnings with useful context. | yes |
| Silent no-op | Ignore unsupported CSS quietly. | |
| Hard fail everything unknown | Reject any unknown property or at-rule at parse time. | |

**User's choice:** Visible diagnostics.
**Notes:** Malformed supported CSS should still fail clearly. Unsupported constructs that would affect cascade semantics should not silently alter output.

---

## Parser, Resolver, and Docs Boundaries

| Option | Description | Selected |
|--------|-------------|----------|
| Preserve crate boundaries | Parser lowering in component, computed style in elements, paint/layout in render. | yes |
| Centralize all CSS behavior in renderer | Move parsing/resolution/render decisions into render crate. | |
| Defer docs/LSP | Implement properties first and document later. | |

**User's choice:** Preserve crate boundaries.
**Notes:** Docs and LSP metadata should track the supported subset where practical so plugin authors can discover the contract.

---

## the agent's Discretion

- The interactive question tool was unavailable in this runtime, so the workflow fallback selected the recommended options from the user's stated scope and the codebase scout.
- Planner/researcher may refine the exact supported property table and diagnostics transport during planning, while preserving the decisions in CONTEXT.md.

## Deferred Ideas

- Container reactivity, text selection, keyboard navigation, animations, and navigation-bar migration are explicitly deferred to later v1.2 phases.
