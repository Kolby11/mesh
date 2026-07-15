---
status: complete
type: quick
completed: 2026-07-15
source_commit: afc65c5e
---

# Quick Summary: Parameterize Brightness Component Scroll Scaling

## Outcome

The shipped brightness button now declares a settings-ready numeric
`scroll_sensitivity` component prop. Its default remains 5 percentage points,
with metadata constraining generated controls to values from 1 through 100 in
steps of 1.

Wheel and two-finger handlers share one defensively normalized amount for both
the optimistic brightness copy and the `mesh.brightness` command. Missing,
non-finite, and non-positive runtime values fall back to 5; finite values are
clamped to the declared range.

## Tests

- `nix develop -c cargo test -p mesh-core-shell shipped_navigation_brightness -- --nocapture` — 3 passed
- `nix develop -c cargo test -p mesh-core-shell props -- --nocapture` — 8 passed, 2 ignored benchmarks
- `rustfmt --edition 2024 --check crates/core/shell/src/shell/component/tests/integration/real_surfaces.rs` — passed
- `git diff --check` — passed before commit

The workspace-wide `cargo fmt --check` remains blocked by pre-existing
formatting drift in unrelated service-contract and backend-supervision files;
the changed Rust test file passes `rustfmt --check` directly.

## Commit

- `afc65c5e` — `feat: parameterize brightness scroll sensitivity`
