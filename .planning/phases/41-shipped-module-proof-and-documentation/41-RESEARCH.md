# Phase 41: Shipped Module Proof and Documentation - Research

## User Constraints

- D-01: Use the existing shipped audio/navigation path as the primary proof path:
  `@mesh/navigation-bar`, `@mesh/audio-interface`, `@mesh/pipewire-audio`, and
  `@mesh/pulseaudio-audio`.
- D-02: Prefer improving the real shipped path over creating a toy proof module;
  add a small fixture only for isolated validation that would be brittle against
  shipped modules.
- D-03: Do not add service-specific Rust APIs. Provider behavior must route
  through interface contracts, installed graph records, generic backend runtime
  wiring, and Luau provider modules.
- D-04: Automated proof must cover manifest normalization, contribution
  indexing, diagnostics, and real proof-module behavior.
- D-05: Prefer targeted Rust tests over broad workspace tests; use real
  manifests and graph fixtures; do not mock manifest parsing or
  interface/provider resolution for module-system behavior.
- D-06: Runtime proof must show root graph entry, canonical manifests, active
  provider, frontend requirement/interface consumption, contribution data, and
  visible diagnostics for missing or incompatible parts.
- D-07: Use `nix develop -c ...` for shell tests that need Wayland/xkbcommon
  native dependencies.
- D-08: Author docs present the final workflow as "extend or add a MESH module"
  using canonical `module.json`.
- D-09: Docs use the proof path walkthrough: frontend requires an interface,
  backend implements it, root graph selects the active provider,
  resources/settings/keybinds are contributions or overrides, diagnostics
  explain gaps.
- D-10: Documentation keeps public vocabulary strict: module, frontend module,
  backend provider, interface contract, contribution, capability, dependency,
  resource pack, settings, keybind.
- Folded install-resolution todo: prove real module requirements and
  contributions can be inspected, resolved, and diagnosed through the installed
  graph. Do not design full installer UX, provider conflict UI, resource pack
  suggestions, settings materialization, or cascade resolver.

## Project Constraints

- No `AGENTS.md` exists in the repo root. [VERIFIED: local filesystem]
- Existing dirty UI/icon/theme files predate this phase and should not be
  reverted or staged unless a Phase 41 task explicitly needs them. [VERIFIED:
  git status]
- Shell interaction tests require the Nix dev shell when host native libraries
  such as xkbcommon are absent. [VERIFIED: Phase 40 execution history and
  D-07]

## Standard Stack

- Use existing Rust cargo tests in `mesh-core-module` and `mesh-core-shell`.
  [VERIFIED: `Cargo.toml`, existing phase plans]
- Use `load_installed_module_graph` with `config/module.json` for real
  bundled-path proof. The current graph selects `@mesh/navigation-bar` as
  layout and `@mesh/pipewire-audio` as the active `mesh.audio` provider.
  [VERIFIED: `config/module.json`, package tests]
- Use `InstalledModuleGraph`, `ModuleContributionIndex`, provider records,
  frontend requirement records, and graph diagnostics as the proof surface.
  [VERIFIED: `crates/core/extension/module/src/package/installed_graph.rs`]
- Use grep checks for author documentation requirements. [VERIFIED: prior
  Phase 40 docs plans]

## Architecture Patterns

- Package-level proof belongs primarily in
  `crates/core/extension/module/src/package/tests.rs`. Existing tests already
  cover loader normalization, installed graph loading from repo fixtures,
  active provider selection, keybind/resource/settings contribution indexing,
  and compatibility diagnostics. [VERIFIED: local tests]
- Shell-level proof belongs in `crates/core/shell/src/shell/tests.rs` when the
  behavior is graph-to-shell registration or runtime selection, and in
  `crates/core/shell/src/shell/component/tests/interaction/navigation.rs` only
  when the behavior requires shipped navigation interaction. [VERIFIED: local
  tests]
- Documentation updates should connect `docs/module-system.md`, shipped
  navigation docs, backend provider docs, settings docs, and `docs/llm-context.md`
  without reintroducing legacy public vocabulary. [VERIFIED: Phase 41 context
  and current docs]

## Don't Hand-Roll

- Do not construct fake manifests when the shipped path can be loaded from
  `config/module.json` and module directories. Use real manifests for the main
  proof. [VERIFIED: D-01, D-05]
- Do not add direct Rust branches for `pipewire`, `pulseaudio`, or `mesh.audio`
  behavior. Tests should assert generic graph/provider records instead.
  [VERIFIED: D-03]
- Do not replace installed-graph diagnostics with ad hoc string scans over
  module JSON files. Use the existing graph diagnostic model. [VERIFIED:
  `InstalledModuleGraph::diagnostics`]

## Common Pitfalls

- Shipped docs currently contain stale vocabulary such as `Type: surface` and
  `@mesh/audio-contract`; docs tasks must update proof-path pages to the
  canonical vocabulary. [VERIFIED: navigation and pipewire README files]
- `config/module.json` currently lists the audio backend providers but does not
  list `@mesh/audio-interface` or icon pack modules. Phase 41 should add
  canonical installed-graph entries for the interface and default icon pack so
  the real proof path resolves through typed records instead of documenting the
  absence as an accepted gap. [VERIFIED: `config/module.json`]
- The primary package graph test currently proves active provider and layout
  only; Phase 41 needs to broaden that proof to contribution data,
  diagnostics, and real interface/provider records. [VERIFIED:
  `installed_module_graph_loads_repo_module_fixture`]
- Full workspace tests can fail because unrelated dirty UI/icon/theme changes
  are present; Phase 41 should use focused verification commands and report
  unrelated full-suite failures if they remain. [VERIFIED: current git status]

## Validation Architecture

Phase 41 should validate three evidence layers:

1. Package graph proof with real shipped manifests:
   `nix develop -c cargo test -p mesh-core-module shipped`.
2. Shell runtime proof with graph-derived provider/frontend selection:
   `nix develop -c cargo test -p mesh-core-shell installed_module_graph`.
3. Documentation proof with grep gates over canonical workflow strings:
   `rg -n "extend or add a MESH module|@mesh/navigation-bar|@mesh/audio-interface|@mesh/pipewire-audio|module.json|mesh.kind|mesh.implements|mesh.keybinds|diagnostics" docs/module-system.md docs/modules/frontend/core/navigation-bar/README.md docs/modules/backend/core/pipewire-audio/README.md docs/modules/backend/core/pulseaudio-audio/README.md docs/settings/README.md docs/llm-context.md`.

These focused gates should run after the relevant task commits, with broader
`nix develop -c cargo test -p mesh-core-module package::tests` and
`nix develop -c cargo test -p mesh-core-shell shell::tests` checks before phase
verification if runtime is acceptable.
