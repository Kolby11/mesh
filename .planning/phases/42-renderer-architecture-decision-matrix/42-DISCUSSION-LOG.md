# Phase 42: Renderer Architecture Decision Matrix - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-05-18
**Phase:** 42-Renderer Architecture Decision Matrix
**Areas discussed:** Blitz posture, Scorecard gates, Crate outcomes, Prototype boundary

---

## Blitz Posture

| Option | Description | Selected |
|--------|-------------|----------|
| Blitz as direct adoption candidate | Assume Blitz might become the base renderer unless blockers appear. | |
| Blitz as reference architecture | Study and prototype against Blitz, but default to preserving MESH-owned renderer boundaries unless direct adoption is clearly cleaner. | ✓ |
| Blitz as dependency/source mining only | Evaluate its crate choices and patterns, but do not consider using Blitz itself as a base. | |
| Other | Freeform stance. | |

**User's choice:** Blitz as reference architecture.
**Notes:** Direct adoption is still allowed if evidence justifies deeper internal redesign. Shell model mismatch and browser-engine-level performance overhead are hard blockers. If direct adoption fails, MESH should borrow Blitz architecture and crates where useful.

---

## Scorecard Gates

| Option | Description | Selected |
|--------|-------------|----------|
| Shell + performance only | Wayland shell fit and no browser-engine-level overhead are hard blockers; other criteria are weighted tradeoffs. | ✓ |
| Shell + performance + observability | Also require recoverable invalidation, damage, profiling, and diagnostics. | |
| All core MESH contracts | Shell fit, performance, observability, retained identity, accessibility, `.mesh` authoring fit, and build/CI cost are all hard blockers. | |
| Other | Freeform blocker set. | |

**User's choice:** Shell + performance only.
**Notes:** Capability gain matters most after blockers pass. Performance overhead includes interaction latency, render/layout/paint architecture cost, startup/build/resource cost, binary size, memory, and native dependency burden. Observability may temporarily regress during prototype work if the final migration plan restores it. Heavy build/native dependencies are acceptable when they unlock significant renderer capability.

---

## Crate Outcomes

| Option | Description | Selected |
|--------|-------------|----------|
| Skia/rust-skia fallback | Useful if Blitz/AnyRender cannot meet MESH needs, but not first direction. | ✓ |
| Taffy + Parley likely accept | Strong standalone layout/text candidates even if Blitz is not adopted. | ✓ |
| Stylo direct candidate | Consider direct Stylo adoption for MESH style resolution if it brings enough CSS capability. | ✓ |
| AnyRender / Vello-style likely accept | Preferred rendering abstraction path before falling back to Skia. | ✓ |
| Winit + AccessKit likely accept; parsers/Muda defer | Window/input plus accessibility matter; menus/parsers are secondary. | ✓ |

**User's choice:** Mixed per-crate stance.
**Notes:** Skia is fallback. Taffy, Parley, AnyRender/Vello-style, Winit, and AccessKit are likely accepts. Stylo is a direct candidate. Muda, html5ever, and xml5ever are deferred unless a concrete need appears.

---

## Prototype Boundary

| Option | Description | Selected |
|--------|-------------|----------|
| Navigation bar | Stable shipped surface for layout/style/icons/state. | |
| Audio popover | Tougher interaction surface for slider, backend updates, transitions. | |
| Both navigation bar and audio popover | Broader proof across layout, style, icons, state, popover interaction, slider behavior, backend updates, text, and transition concerns. | ✓ |
| Synthetic retained fixture | Fastest controlled comparison, weaker real-surface proof. | |

**User's choice:** Both navigation bar and audio popover.
**Notes:** Prototypes should be throwaway harnesses, compare visual output plus interaction shape, and not wire into production code. Phase 42 should hand off a decision matrix only. If two surfaces feel expensive, do not reduce scope.

---

## the agent's Discretion

- Choose the exact matrix format, scoring scale, and evidence presentation.
- Decide how to express weighted tradeoffs as long as hard blockers remain separate.

## Deferred Ideas

- Audio popover transition delay polish remains a prior deferred todo, not folded into Phase 42.
