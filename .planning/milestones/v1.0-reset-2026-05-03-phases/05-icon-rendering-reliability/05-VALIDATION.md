---
phase: 05
slug: icon-rendering-reliability
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-05-03
---

# Phase 05 - Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test harness via Cargo |
| **Config file** | Workspace `Cargo.toml`; no separate test config required |
| **Quick run command** | `nix develop -c cargo test -p mesh-core-icon -p mesh-core-render` |
| **Full suite command** | `nix develop -c cargo test` |
| **Estimated runtime** | ~90 seconds targeted; full suite varies by workspace state |

---

## Sampling Rate

- **After every task commit:** Run `nix develop -c cargo test -p mesh-core-icon -p mesh-core-render`
- **After every plan wave:** Run `nix develop -c cargo test -p mesh-core-icon -p mesh-core-render -p mesh-core-diagnostics -p mesh-core-shell`
- **Before `$gsd-verify-work`:** Full suite must be green
- **Max feedback latency:** 120 seconds for targeted checks

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 05-W0-01 | TBD | 0 | ICON-01 | T-05-01 / T-05-02 | Reject unsafe or malformed icon config without path traversal or panics | unit | `nix develop -c cargo test -p mesh-core-icon icon_config_resolves_ordered_fallbacks` | No - Wave 0 | pending |
| 05-W0-02 | TBD | 0 | ICON-01 | T-05-03 | Profile changes invalidate stale resolution cache | unit | `nix develop -c cargo test -p mesh-core-icon icon_profile_switch_invalidates_cache` | No - Wave 0 | pending |
| 05-W0-03 | TBD | 0 | ICON-02 | T-05-04 | SVG decode failures degrade without crashing render | unit | `nix develop -c cargo test -p mesh-core-render svg_icon_rasterizes_and_tints` | No - Wave 0 | pending |
| 05-W0-04 | TBD | 0 | ICON-03 | T-05-04 | Raster decode failures degrade without crashing render | unit | `nix develop -c cargo test -p mesh-core-render raster_icon_decodes_resizes_and_tints` | No - Wave 0 | pending |
| 05-W0-05 | TBD | 0 | ICON-04 | T-05-05 | Missing-icon diagnostics dedupe and mark degraded health without disabling plugin load | unit/integration | `nix develop -c cargo test -p mesh-core-diagnostics -p mesh-core-render missing_icon_dedupes_and_paints_fallback` | No - Wave 0 | pending |
| 05-W0-06 | TBD | 0 | ICON-01..ICON-04 | T-05-06 | Core surfaces use semantic icons only and do not bypass resolver safety | integration | `nix develop -c cargo test -p mesh-core-shell icon_reliability_core_surfaces_proof` | No - Wave 0 | pending |

---

## Wave 0 Requirements

- [ ] `crates/core/ui/icon/src/config.rs` tests for parsing active profile, packs, mappings, ordered fallback lists, and missing active profile.
- [ ] `crates/core/ui/icon/src/registry.rs` tests for configured root lookup, profile switching, bundled Material mapping, and cache invalidation.
- [ ] `crates/core/ui/render/src/surface/icon.rs` tests for SVG tint, raster tint, multicolor preservation, and fallback drawing.
- [ ] `crates/core/foundation/diagnostics/src/lib.rs` tests for missing-icon dedupe and degraded health.
- [ ] Shell/component proof test that gives renderer/plugin diagnostics enough context to dedupe by plugin plus semantic name.

*Executor note: Wave 0 is required before feature implementation tasks claim coverage for ICON-01 through ICON-04.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Visual inspection of panel, quick settings, and navigation bar icon appearance | ICON-01..ICON-04 | Pixel/unit tests prove rendering mechanics, but final shell composition should still be inspected for stable layout and expected icon semantics | Run the shell proof surface after automated tests pass; verify panel, quick settings, and navigation bar show expected semantic icons, preserve layout for fallback, and do not require pack-specific paths in `.mesh` files |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify commands or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all missing references
- [ ] No watch-mode flags
- [ ] Feedback latency < 120s for targeted checks
- [ ] `nyquist_compliant: true` set in frontmatter once Wave 0 tests exist

**Approval:** pending
