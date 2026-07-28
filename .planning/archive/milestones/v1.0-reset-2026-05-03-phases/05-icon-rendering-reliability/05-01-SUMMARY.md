---
phase: 05-icon-rendering-reliability
plan: 01
subsystem: ui-icon
tags: [icons, xdg, config, rust]
requires:
  - phase: 04-real-core-surfaces
    provides: core surfaces that consume semantic icon names
provides:
  - typed icon config parsing and validation
  - semantic IconRegistry with ordered fallback resolution
  - profile-aware cache invalidation
  - default Material-backed compatibility facade
affects: [render, shell-surfaces, plugin-icons]
tech-stack:
  added: [serde, toml]
  patterns: [typed-config, semantic-registry, profile-generation-cache]
key-files:
  created:
    - crates/core/ui/icon/src/config.rs
    - crates/core/ui/icon/src/registry.rs
    - crates/core/ui/icon/src/xdg.rs
    - crates/core/ui/icon/src/fallback.rs
  modified:
    - crates/core/ui/icon/src/lib.rs
    - crates/core/ui/icon/Cargo.toml
key-decisions:
  - "Default named-icon compatibility now routes through a built-in Material profile."
  - "Icon candidates use explicit `pack_id:asset_name` mappings with optional `?multicolor` metadata."
patterns-established:
  - "IconConfig validates active profile, unique packs, non-empty mappings, and known candidate packs."
  - "IconRegistry clears profile-sensitive cache state when config changes."
requirements-completed: [ICON-01]
duration: 6 min
completed: 2026-05-03
---

# Phase 05 Plan 01: Semantic Icon Registry Summary

**Typed semantic icon registry with dedicated TOML config, explicit pack roots, ordered fallbacks, and profile-aware cache invalidation**

## Performance

- **Duration:** 6 min
- **Started:** 2026-05-03T14:00:00Z
- **Completed:** 2026-05-03T14:06:00Z
- **Tasks:** 4
- **Files modified:** 6

## Accomplishments
- Added `IconConfig`, `IconPackRoot`, `IconProfile`, and `IconCandidate` with TOML parsing and validation.
- Added `IconRegistry` and `IconResolution` so callers can distinguish found and missing semantic icons.
- Preserved `resolve_icon(name, size)` compatibility through a default Material-backed registry.

## Task Commits

1. **Semantic icon registry and config** - `965058a` (feat)

## Files Created/Modified
- `crates/core/ui/icon/src/config.rs` - typed icon configuration and validation.
- `crates/core/ui/icon/src/registry.rs` - semantic resolution, ordered fallbacks, and profile cache invalidation.
- `crates/core/ui/icon/src/xdg.rs` - configured-root lookup adapter.
- `crates/core/ui/icon/src/fallback.rs` - built-in fallback identity.
- `crates/core/ui/icon/src/lib.rs` - public facade and default registry.
- `crates/core/ui/icon/Cargo.toml` - serde/toml dependencies.

## Decisions Made
- Kept explicit file path support in `resolve_icon()` for compatibility.
- Used bundled Material assets as the default profile so existing surface tests continue to resolve named icons.

## Deviations from Plan

The four small TDD/config/registry tasks were implemented and committed together as one subsystem commit. No behavioral scope was added beyond the plan.

## Issues Encountered

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

Render code can now consume typed `IconResolution` values and handle found versus missing icon paths.

## Self-Check: PASSED

- `nix develop -c cargo test -p mesh-core-icon` passed.

---
*Phase: 05-icon-rendering-reliability*
*Completed: 2026-05-03*

