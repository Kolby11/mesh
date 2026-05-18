---
phase: 43
slug: comparable-renderer-prototype-proofs
status: draft
nyquist_compliant: true
wave_0_complete: false
created: 2026-05-18
---

# Phase 43 - Validation Strategy

> Per-phase validation contract for prototype evidence sampling during execution.

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust Cargo prototype harness |
| **Config file** | `.planning/prototypes/phase43/Cargo.toml` |
| **Quick run command** | `cargo check --manifest-path .planning/prototypes/phase43/Cargo.toml` |
| **Full suite command** | `cargo test --manifest-path .planning/prototypes/phase43/Cargo.toml` |
| **Estimated runtime** | ~60 seconds after dependencies are available |

## Sampling Rate

- **After every task commit:** Run `cargo check --manifest-path .planning/prototypes/phase43/Cargo.toml` after the prototype manifest exists.
- **After every plan wave:** Run `cargo test --manifest-path .planning/prototypes/phase43/Cargo.toml` when tests exist; otherwise run the relevant `rg` artifact checks in the plan.
- **Before `$gsd-verify-work`:** Final comparison and Phase 44 handoff must exist and mention all required headings.
- **Max feedback latency:** 120 seconds, excluding first-time dependency fetch/build.

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 43-01-01 | 01 | 1 | PROTO-03 | T-43-01 | N/A | artifact | `rg -n "nav-baseline|audio-slider-change-release|audio-popover-close" .planning/prototypes/phase43/fixtures/phase43-scenarios.json` | W0 | pending |
| 43-01-02 | 01 | 1 | PROTO-03 | T-43-02 | N/A | cargo | `cargo check --manifest-path .planning/prototypes/phase43/Cargo.toml` | W0 | pending |
| 43-02-01 | 02 | 2 | PROTO-01 | T-43-03 | N/A | cargo/artifact | `cargo check --manifest-path .planning/prototypes/phase43/Cargo.toml` | W0 | pending |
| 43-02-02 | 02 | 2 | PROTO-01 | T-43-03 | N/A | artifact | `rg -n "Attempted harness|Crate/API boundary|Observed error or mismatch|Reproduction" .planning/prototypes/phase43/evidence/blitz-reference.md` | W0 | pending |
| 43-03-01 | 03 | 2 | PROTO-02 | T-43-04 | N/A | cargo/artifact | `cargo check --manifest-path .planning/prototypes/phase43/Cargo.toml` | W0 | pending |
| 43-03-02 | 03 | 2 | PROTO-02 | T-43-04 | N/A | artifact | `rg -n "stable_node_id|display_slot|accesskit_node_id|taffy_layout|parley_text" .planning/prototypes/phase43/evidence/focused-crate.md .planning/prototypes/phase43/output/focused-crate.json` | W0 | pending |
| 43-04-01 | 04 | 3 | PROTO-03 | T-43-05 | N/A | artifact | `rg -n "visual/layout fidelity|interaction shape|retained identity fit|accessibility boundary|build/dependency cost|blocker evidence|Phase 44 integration readiness" .planning/phases/43-comparable-renderer-prototype-proofs/43-PROTOTYPE-COMPARISON.md` | W0 | pending |

## Wave 0 Requirements

- [ ] `.planning/prototypes/phase43/Cargo.toml` - isolated prototype manifest with an empty `[workspace]` table.
- [ ] `.planning/prototypes/phase43/fixtures/phase43-scenarios.json` - shared scenario fixture.
- [ ] `.planning/prototypes/phase43/src/lib.rs` - shared fixture loading and evidence schema.

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Optional screenshot/pixel evidence | PROTO-01, PROTO-02 | Headless renderer setup may exceed throwaway scope or require local GPU/window support. | Inspect generated output files and note whether screenshots were produced or intentionally skipped. |
| Phase 44 path selection judgment | PROTO-03 | Requires architectural tradeoff review, not just command success. | Read `43-PROTOTYPE-COMPARISON.md` and confirm the selected path is supported by the two prototype evidence files. |

## Validation Sign-Off

- [x] All tasks have automated verify commands or artifact checks.
- [x] Sampling continuity: no 3 consecutive tasks without automated verify.
- [x] Wave 0 covers all missing references.
- [x] No watch-mode flags.
- [x] Feedback latency target is under 120 seconds after dependencies are available.
- [x] `nyquist_compliant: true` set in frontmatter.

**Approval:** approved 2026-05-18

