# Phase 13: Navigation-Bar Rendering Proof - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-05-08
**Phase:** 13-Navigation-Bar Rendering Proof
**Areas discussed:** Proof scope, Content model, Constrained-width behavior, Animation proof, Test strategy

---

## Proof Scope

| Option | Description | Selected |
|--------|-------------|----------|
| Keep the bar compact and add one passive text/status area | Lowest scope increase while still proving selection and responsive layout | |
| Restore richer visible labels/status copy directly in the bar | Stronger real-surface proof with more layout and content space | ✓ |
| Keep it icon-first and prove selection elsewhere like the popover | Minimal bar churn but weaker primary-surface proof | |
| Let the agent decide | Smallest change that still satisfies the requirement set | |

**User's choice:** Restore richer visible labels/status copy directly in the bar, but keep the work proof-focused rather than turning Phase 13 into broader feature development.
**Notes:** The user initially suggested adding a clock, theme dropdown, expanded volume mixer, and battery popover to create more testing space. Those were treated as future capabilities outside current Phase 13 scope.

---

## Content Model

| Option | Description | Selected |
|--------|-------------|----------|
| One compact status cluster plus controls | Keeps the bar readable and shell-like while creating proof space | ✓ |
| Several labeled sections across the bar | Stronger layout proof, but heavier migration | |
| Mostly controls with labels/values on each control | Control-first proof with more visible copy | |
| Let the agent decide | Smallest richer content model that still proves the milestone | |

**User's choice:** One compact status cluster plus controls.
**Notes:** The user wants the surface richer than the current compact strip, but still cohesive and shell-like.

---

## Constrained-Width Behavior

| Option | Description | Selected |
|--------|-------------|----------|
| Status text compresses first, controls stay available | Best fit for shell chrome and preserves interactivity | ✓ |
| Controls compress to icon-only first, text stays longer | Better for info-first surfaces | |
| One secondary control drops behind overflow/popover | Stronger responsive proof but more behavior complexity | |
| Let the agent decide | Choose the narrowest policy that still reads clearly | |

**User's choice:** Status text compresses first and controls stay available.
**Notes:** Interactivity should survive constrained widths before secondary copy does.

---

## Animation Proof

| Option | Description | Selected |
|--------|-------------|----------|
| Subtle transitions everywhere plus one clear custom keyframe moment | Balanced proof of Phase 12 without visual noise | ✓ |
| Mostly subtle transitions only | Safer visually but weaker keyframe proof | |
| More atmospheric multi-element animation | Stronger visual proof, higher migration complexity | |
| Let the agent decide | Choose one restrained but obvious keyframe proof | |

**User's choice:** Subtle transitions everywhere plus one clear custom keyframe moment.
**Notes:** The user wants the proof to be visible and deliberate, not noisy.

---

## Test Strategy

| Option | Description | Selected |
|--------|-------------|----------|
| Real-surface shell tests plus one constrained-width proof | Strong coverage while keeping the shipped surface primary | ✓ |
| Real-surface shell tests only | Simpler, but responsive behavior less directly pinned down | |
| Real-surface tests plus constrained-width plus tighter focused fixture | Strongest coverage, slightly broader scope | |
| Let the agent decide | Lightest defensible test set | |

**User's choice:** Real-surface shell tests plus one constrained-width proof.
**Notes:** The user wants defensible proof coverage without turning Phase 13 into a large testing-only phase.

---

## the agent's Discretion

- Exact arrangement of the compact status cluster and control cluster
- Exact responsive truncation/compression strategy for status text
- Exact element/state chosen for the one explicit keyframe proof
- Exact automated constrained-width test shape

## Deferred Ideas

- Clock component with configurable time format and hover date
- Theme picker dropdown
- Expanded volume mixer
- Battery-status indicator with hover popover and battery statistics
