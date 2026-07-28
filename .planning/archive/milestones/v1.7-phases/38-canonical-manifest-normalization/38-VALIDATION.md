---
phase: 38
slug: canonical-manifest-normalization
status: draft
nyquist_compliant: true
wave_0_complete: true
created: 2026-05-17
---

# Phase 38 - Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

## Test Infrastructure

| Property | Value |
|----------|-------|
| Framework | Rust cargo test |
| Config file | `Cargo.toml` |
| Quick run command | `cargo test -p mesh-core-module package::tests` |
| Full suite command | `cargo test --workspace` |
| Estimated runtime | 60-240 seconds |

## Sampling Rate

- After every task commit: run the task-specific focused cargo test listed in
  the plan.
- After every plan wave: run `cargo test -p mesh-core-module`.
- Before `$gsd-verify-work`: run `cargo test --workspace`.
- Max feedback latency: one task.

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 38-01-01 | 01 | 1 | MAN-01 | T-38-01-01 | Public Rust names expose module vocabulary without aliases. | compile/unit | `cargo test -p mesh-core-module package::tests` | yes | pending |
| 38-01-02 | 01 | 1 | MAN-03 | T-38-01-02 | Manifest diagnostics include path, field path, severity, and replacement action. | unit | `cargo test -p mesh-core-module package::tests` | yes | pending |
| 38-02-01 | 02 | 1 | MAN-01, MAN-02 | T-38-02-01 | Canonical `module.json` is preferred and duplicate sources block. | unit | `cargo test -p mesh-core-module package::tests` | yes | pending |
| 38-02-02 | 02 | 1 | MAN-02, MAN-03 | T-38-02-02 | Legacy manifest forms load only as migration inputs with warnings. | unit | `cargo test -p mesh-core-module manifest::tests` | yes | pending |
| 38-03-01 | 03 | 2 | MAN-01, MAN-02 | T-38-03-01 | Root graph uses canonical `module.json` and preserves active providers/layout. | integration | `cargo test -p mesh-core-module package::tests installed_module_graph_loads_repo_module_fixture` | yes | pending |
| 38-03-02 | 03 | 2 | MAN-02 | T-38-03-02 | Shipped manifests preserve keybind/provider/capability/dependency data. | integration | `cargo test -p mesh-core-module package::tests` | yes | pending |
| 38-04-01 | 04 | 3 | MAN-01, MAN-03 | T-38-04-01 | Docs and tests expose canonical schema and actionable diagnostics. | docs/unit | `cargo test -p mesh-core-module manifest::tests` | yes | pending |
| 38-04-02 | 04 | 3 | MAN-02 | T-38-04-02 | Shell uses canonical root graph path without losing fallback behavior. | integration | `cargo test -p mesh-core-shell shell::tests` | yes | pending |

## Wave 0 Requirements

Existing infrastructure covers all phase requirements.

## Manual-Only Verifications

All phase behaviors have automated verification.

## Validation Sign-Off

- [x] All tasks have automated verification or an existing Wave 0 dependency.
- [x] Sampling continuity: no 3 consecutive tasks without automated verify.
- [x] Wave 0 covers all missing references.
- [x] No watch-mode flags.
- [x] Feedback latency under 240 seconds for focused checks.
- [x] `nyquist_compliant: true` set in frontmatter.

Approval: approved 2026-05-17

