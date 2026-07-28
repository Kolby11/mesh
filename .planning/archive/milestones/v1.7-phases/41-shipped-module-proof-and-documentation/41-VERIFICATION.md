---
phase: 41-shipped-module-proof-and-documentation
verified: 2026-05-18T13:50:18+02:00
verdict: pass_with_unrelated_full_suite_failures
requirements: [PROOF-01]
---

# Phase 41 Verification

Phase 41 satisfies `PROOF-01`: the shipped module graph now includes canonical
interface and icon-pack modules, package and shell tests prove the real graph
path, shipped navigation remains interface/keybind-driven, and author docs
teach the canonical workflow.

## Verified Scope

- `config/module.json` enables `@mesh/navigation-bar`,
  `@mesh/audio-interface`, `@mesh/icons-default`, `@mesh/pipewire-audio`, and
  `@mesh/pulseaudio-audio`.
- `@mesh/pipewire-audio` is selected as the active `mesh.audio` provider while
  `@mesh/pulseaudio-audio` remains available as an alternate provider record.
- Shell runtime tests register providers through generic installed graph
  records rather than service-specific Rust branches.
- Navigation behavior remains driven by `require("mesh.audio@>=1.0")` and
  manifest-declared `mesh.keybinds.mute`.
- Documentation now describes extending or adding a MESH module through
  canonical `module.json`, interface contracts, backend providers,
  contributions, settings/keybind overrides, root graph selection, and
  diagnostics.

## Commands

Passed:

- `nix develop -c cargo test -p mesh-core-module shipped_module`
- `nix develop -c cargo test -p mesh-core-shell installed_module_graph`
- `nix develop -c cargo test -p mesh-core-shell shell::component::tests::interaction::navigation`
- `nix develop -c cargo test -p mesh-core-shell icon_reliability_core_surfaces_proof`
- `nix develop -c cargo test -p mesh-core-shell pcall_service_lookup_diagnostic_reaches_component_diagnostics`
- `nix develop -c cargo test -p mesh-core-shell selection_fixture_module_is_disabled_in_local_graph`
- `nix develop -c cargo build`
- Phase 41 documentation `rg` checks from `41-03-PLAN.md`

Full workspace test status:

- `nix develop -c cargo test` fails in three shell tests outside Phase 41's
  module graph and documentation scope:
  - `shell::component::tests::invalidation::basic::typed_invalidations_distinguish_restyle_from_script_rebuild`
  - `shell::tests::pointer_click_after_transfer_clears_transfer_forced_exclusive_override`
  - `shell::tests::pointer_click_claims_keyboard_owner_without_forcing_exclusive_mode`

The initial full-suite run also exposed three tests that still expected the
old pre-Phase-41 graph/import vocabulary. Those were updated and committed in
`d1c974e`:

- `icon_reliability_core_surfaces_proof`
- `pcall_service_lookup_diagnostic_reaches_component_diagnostics`
- `selection_fixture_module_is_disabled_in_local_graph`

## Residual Risk

The remaining full-suite failures should be handled by the owner of retained
invalidation and keyboard focus behavior. They do not invalidate the Phase 41
proof path, but they keep the workspace-wide test gate red.
