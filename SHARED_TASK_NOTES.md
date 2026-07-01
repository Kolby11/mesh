# Shared task notes

## This iteration (2026-07-02)

- Verified and closed two todo.md items that were actually already fixed by
  prior commits but left unchecked: the promoted-popover hover-bridge
  pointer-enter fix (`2425c33a`) and the backend `init`/frontend `onRender`
  legacy-lifecycle fallback removal (this iteration — migrated the one
  remaining shipped straggler `modules/frontend/debug-inspector/src/main.mesh`
  from `onRender` to `render`, then dropped both fallback code paths and
  updated ~10 test files that still used the legacy names).
- Full workspace test suite passes at the pre-existing baseline (see below).

## Known pre-existing breakage (not caused by this session, needs its own pass)

`mesh-core-animation`'s unit tests **do not compile** on current `main`:
- `crates/core/ui/animation/src/keyframes.rs:242` — `AnimatableStyle` literal
  missing the `visibility` field.
- `crates/core/ui/animation/src/transition.rs:587` — references
  `ComputedStyle.transition` (singular), but the real field is `transitions`.

This blocks a plain `cargo test --workspace`; use
`cargo test --workspace --exclude mesh-core-animation` until it's fixed.
Also note `style::tests::shipped_audio_style_fixture_resolves_painter_relevant_values`
in `mesh-core-elements` is a separately-known pre-existing failure (see
memory `project_animation_engine`).

Worth fixing next — small, isolated compile fix, then re-run `mesh-core-animation`'s
suite to see if it reveals real regressions once it compiles again.

## Environment note

Bare `cargo build`/`cargo test` fails outside the nix shell (`smithay-client-toolkit`
can't find `xkbcommon` via pkg-config). Always prefix Rust builds/tests with
`nix develop --command ...` in this repo.

## Good next todo.md items (scoped, not yet started)

- Fix the `mesh-core-animation` compile break above (quick, unblocks full-suite runs).
- `service_name_from_interface` dedup, `ModuleKind::{FontPack,Library}` lossy
  conversion, `BackendScriptContext`/`ScriptContext` constructor explosion —
  all in the "Migration tech-debt" section, all small and well-scoped.
- Any of the "Cheap quality wins" style items would be `Split
  FrontendSurfaceComponent::paint` or `StyleResolver::apply_declaration`
  table-driven refactor — larger, riskier, better as their own reviewed PRs.
- Frontend-side typed/declared event channel validation (todo.md "Make event
  channels typed and declared" — backend-side landed, frontend-side open).
