---
phase: 39
slug: contribution-and-interface-extension-index
status: draft
nyquist_compliant: true
wave_0_complete: true
created: 2026-05-17
---

# Phase 39 - Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

## Test Infrastructure

| Property | Value |
|----------|-------|
| Framework | Rust cargo test |
| Config file | `Cargo.toml` |
| Quick run command | `cargo test -p mesh-core-module package::tests` |
| Shell run command | `cargo test -p mesh-core-shell shell::tests` |
| Full suite command | `cargo test --workspace` |
| Estimated runtime | 60-240 seconds |

## Sampling Rate

- After every task commit: run the task-specific focused cargo test listed in
  the plan.
- After every plan wave: run `cargo test -p mesh-core-module package::tests` and
  the relevant shell focused test for that wave.
- Before `$gsd-verify-work`: run `cargo test --workspace`.
- Max feedback latency: one task.

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 39-01-01 | 01 | 1 | EXT-01 | T-39-01-01 | Explicit interface relationship contradictions are rejected while independent same-domain interfaces remain guidance-only. | unit | `cargo test -p mesh-core-module package::tests interface_relationship` | yes | pending |
| 39-01-02 | 01 | 1 | EXT-01, EXT-02 | T-39-01-02 | Provider declarations, interface declarations, and frontend dependencies remain separately inspectable. | unit | `cargo test -p mesh-core-module package::tests installed_module_graph` | yes | pending |
| 39-02-01 | 02 | 2 | EXT-02 | T-39-02-01 | Provider identity does not imply host capability permission. | unit | `cargo test -p mesh-core-module package::tests provider_capability` | yes | pending |
| 39-02-02 | 02 | 2 | EXT-02 | T-39-02-02 | Missing provider, missing active provider, and missing contract diagnostics are distinct. | integration | `cargo test -p mesh-core-shell shell::tests backend` | yes | pending |
| 39-03-01 | 03 | 3 | EXT-03 | T-39-03-01 | Enabled modules produce source-rich typed contribution records with scoped ids. | unit | `cargo test -p mesh-core-module package::tests contribution_index` | yes | pending |
| 39-03-02 | 03 | 3 | EXT-03 | T-39-03-02 | Keybind, settings, frontend entrypoint, library, resource, interface, and provider contributions are queryable by typed getters. | unit | `cargo test -p mesh-core-module package::tests contribution_index` | yes | pending |
| 39-04-01 | 04 | 4 | EXT-03, EXT-04 | T-39-04-01 | Shell registration and backend launch consume installed graph metadata without service-specific branches. | integration | `cargo test -p mesh-core-shell shell::tests` | yes | pending |
| 39-04-02 | 04 | 4 | EXT-03, EXT-04 | T-39-04-02 | Resource/settings diagnostics route through typed graph data and remain non-fatal unless the module graph is invalid. | integration | `cargo test -p mesh-core-shell shell::tests` | yes | pending |

## Wave 0 Requirements

Existing test infrastructure covers all phase requirements. No new harness is
needed before implementation.

## Manual-Only Verifications

All phase behaviors have automated verification. Manual review is limited to
checking that diagnostic text is understandable.

## Validation Sign-Off

- [x] All tasks have automated verification or an existing Wave 0 dependency.
- [x] Sampling continuity: no 3 consecutive tasks without automated verify.
- [x] Wave 0 covers all missing references.
- [x] No watch-mode flags.
- [x] Feedback latency under 240 seconds for focused checks.
- [x] `nyquist_compliant: true` set in frontmatter.

Approval: approved 2026-05-17
