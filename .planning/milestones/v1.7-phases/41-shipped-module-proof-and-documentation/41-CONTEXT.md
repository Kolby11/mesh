# Phase 41: Shipped Module Proof and Documentation - Context

**Gathered:** 2026-05-18T11:15:33Z
**Status:** Ready for planning

<domain>
## Phase Boundary

Phase 41 proves the v1.7 consolidated module model on a real bundled
module/provider path. The proof should connect canonical `module.json`
manifests, installed-graph contribution records, interface/provider
resolution, settings/keybind/resource declarations, diagnostics, tests, and
author documentation.

This phase is not for adding new module-system capabilities, broad installer
UX, marketplace/distribution policy, compositor-global shortcuts, or a full
resource-cascade implementation. It should demonstrate the model that Phases
37-40 already defined and implemented, using the smallest real shipped path
that proves `PROOF-01`.

</domain>

<decisions>
## Implementation Decisions

### Proof Path
- **D-01:** Use the existing shipped audio/navigation path as the primary
  proof path: `@mesh/navigation-bar` frontend, `@mesh/audio-interface`, and
  the `@mesh/pipewire-audio` / `@mesh/pulseaudio-audio` backend providers.
  This path already exercises frontend module metadata, interface dependency,
  active provider selection, backend provider declarations, settings,
  keybinds, icons/resources, and real Luau provider code.
- **D-02:** Prefer improving this real path over creating a toy proof module.
  A small fixture module may be added only when a focused validation case would
  be awkward or brittle against the real shipped modules.
- **D-03:** The proof must not add service-specific Rust APIs. Any provider
  behavior should continue to route through interface contracts, installed
  graph records, generic backend runtime wiring, and Luau provider modules.

### Evidence Depth
- **D-04:** Automated proof should cover all four roadmap success criteria:
  manifest normalization, contribution indexing, diagnostics, and real
  proof-module behavior.
- **D-05:** The planner should bias toward targeted Rust tests over broad
  workspace tests as the phase acceptance proof. Use real manifests and graph
  fixtures where possible; do not mock manifest parsing or interface/provider
  resolution when proving module-system behavior.
- **D-06:** Runtime proof should show the real path end to end enough to be
  credible: root graph entry, canonical module manifests, active provider,
  frontend requirement/interface consumption, contribution data, and visible
  diagnostics for any missing or incompatible part.
- **D-07:** `nix develop -c ...` is the canonical verification wrapper for
  shell tests because Wayland/xkbcommon dependencies are required. Plain
  `cargo test` outside Nix is not a valid shell proof gate when it fails before
  tests run due missing native libraries.

### Author Documentation
- **D-08:** Author docs should present the final workflow as "extend or add a
  MESH module" using canonical `module.json`, not as package/plugin
  compatibility guidance.
- **D-09:** The docs should use the proof path as a concrete walkthrough:
  frontend requires an interface, backend module implements that interface,
  root graph selects the active provider, resources/settings/keybinds are
  declared as contributions or overrides, and diagnostics explain gaps.
- **D-10:** Documentation should keep public vocabulary strict: module,
  frontend module, backend provider, interface contract, contribution,
  capability, dependency, resource pack, settings, keybind. Old names belong
  only in migration diagnostics or explicitly labeled legacy guidance.

### Folded Todos
- **Define module install requirement resolution:** Folded only as proof
  context. Phase 41 should prove that real module requirements and
  contributions can be inspected/resolved/diagnosed through the installed
  graph. It should not design the full future installer, provider conflict UI,
  resource pack suggestion system, settings materialization policy, or
  cascade resolver beyond what the proof path needs.

### the agent's Discretion
Because interactive question UI was unavailable in this runtime, this context
uses the workflow's recommended defaults. The planner may choose the exact test
split, fixture boundaries, and documentation page placement, provided the
decisions above and `PROOF-01` remain satisfied.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Milestone And Phase Context
- `.planning/PROJECT.md` - v1.7 milestone intent, Phase 40 completion state,
  active boundaries, and locked project decisions.
- `.planning/REQUIREMENTS.md` - `PROOF-01` is the only remaining v1.7
  requirement and maps to Phase 41.
- `.planning/ROADMAP.md` - Phase 41 goal and success criteria.
- `.planning/STATE.md` - Current phase position and prior architecture
  decisions, especially no service-specific Rust APIs.
- `.planning/phases/37-concept-inventory-and-vocabulary-lock/37-CONTEXT.md` -
  Locked vocabulary, no public aliases, concept boundaries.
- `.planning/phases/38-canonical-manifest-normalization/38-CONTEXT.md` -
  Canonical manifest shape, loader/migration behavior, root graph migration,
  and diagnostic expectations.
- `.planning/phases/40-migration-diagnostics-and-author-docs/40-VERIFICATION.md`
  - Confirms migration diagnostics and keybind preservation passed; Phase 41
  should build proof on top of this state.

### Proof Path Manifests And Runtime
- `config/module.json` - Root installed-module graph selecting modules,
  active provider, and layout entrypoint.
- `modules/frontend/navigation-bar/module.json` - Primary real frontend
  proof module with interface dependency, settings, keybinds, resources, and
  frontend entrypoint.
- `modules/frontend/navigation-bar/src/main.mesh` - Real frontend surface
  entrypoint for shipped behavior proof.
- `modules/frontend/navigation-bar/src/components/volume-button.mesh` - Real
  audio interface consumer and shortcut/control behavior.
- `modules/interfaces/audio.toml` - Audio interface contract used by the proof
  path.
- `modules/backend/pipewire-audio/module.json` - Primary backend provider
  manifest for `mesh.audio`.
- `modules/backend/pipewire-audio/src/main.luau` - Real provider
  implementation through generic backend host APIs.
- `modules/backend/pulseaudio-audio/module.json` - Alternate backend provider
  manifest proving active-provider selection and provider multiplicity.
- `modules/backend/pulseaudio-audio/src/main.luau` - Alternate provider
  implementation.
- `modules/icon-packs/material-symbols/module.json` and
  `modules/icon-packs/default/module.json` - Resource-pack manifests available
  for proof of resource contribution/requirement diagnostics.

### Module System Code And Tests
- `crates/core/extension/module/src/package/installed_graph.rs` - Installed
  graph loader, active provider validation, contribution indexing, diagnostics,
  and keybind/resource records.
- `crates/core/extension/module/src/package/tests.rs` - Primary place for
  manifest normalization, graph contribution, diagnostics, and real-fixture
  proof tests.
- `crates/core/extension/module/src/manifest/model.rs` and
  `crates/core/extension/module/src/manifest/tests.rs` - Canonical/runtime
  manifest validation and keybind/default parsing behavior.
- `crates/core/shell/src/shell/discovery.rs` - Shell loading of
  `config/module.json` and installed graph registration.
- `crates/core/shell/src/shell/tests.rs` - Shell-level provider/interface
  lifecycle and installed-graph integration tests.
- `crates/core/shell/src/shell/component/tests/interaction/navigation.rs` -
  Real navigation shortcut/interface behavior tests.

### Author Documentation
- `docs/module-system.md` - Main canonical module model and migration
  diagnostics documentation.
- `docs/modules/frontend/core/navigation-bar/README.md` - Existing shipped
  frontend module documentation for the proof path.
- `docs/modules/backend/core/pipewire-audio/README.md` and
  `docs/modules/backend/core/pulseaudio-audio/README.md` - Existing backend
  provider docs to align with the final workflow.
- `docs/settings/README.md` - Settings and keybind override boundary.
- `docs/llm-context.md` - AI-facing canonical module workflow and codebase
  map; update only if Phase 41 proof changes author guidance.

### Folded Input
- `.planning/todos/pending/2026-05-15-define-module-install-requirement-resolution.md`
  - Requirement/contribution/resource resolution note folded narrowly into
  proof-path expectations.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `InstalledModuleGraph`, `ModuleContributionIndex`, `BackendProviderNode`,
  `FrontendRequirementSet`, and `Contributed*` records in
  `crates/core/extension/module/src/package/installed_graph.rs` are the core
  proof targets.
- Real shipped manifests already exist under `modules/frontend/`,
  `modules/backend/`, `modules/interfaces/`, and `modules/icon-packs/`.
- `config/module.json` already points the runtime at the canonical module
  graph and should be part of the proof.
- Existing package/module tests include helper fixtures for root graphs,
  loaded modules, providers, interfaces, resources, diagnostics, and keybinds.
- Shell tests and navigation interaction tests already exercise real
  frontend/backend/interface behavior through private integration helpers.

### Established Patterns
- Rust core remains generic. Do not add direct audio/network/power behavior to
  Rust for proof convenience.
- Frontend modules depend on interface contracts, not backend module IDs.
- Backend providers implement interfaces and are selected by the installed
  graph.
- Capabilities, dependencies, contributions, and settings/keybind overrides
  are separate concepts and should stay separate in tests and docs.
- Tests should use real manifest parsing and installed graph resolution rather
  than mocking those layers.

### Integration Points
- Package graph proof belongs primarily in
  `crates/core/extension/module/src/package/tests.rs`.
- Shell proof may use `crates/core/shell/src/shell/tests.rs`,
  `crates/core/shell/src/shell/discovery.rs`, and navigation interaction tests
  when runtime registration or real surface behavior matters.
- Author docs should connect `docs/module-system.md`, shipped frontend docs,
  backend provider docs, and settings docs into one coherent workflow.

</code_context>

<specifics>
## Specific Ideas

- Defaulted discussion choice: use the audio/navigation path because it is the
  most complete real bundled path: frontend surface, interface contract,
  multiple backend providers, active provider selection, settings, keybinds,
  icons/resources, docs, and diagnostics.
- Defaulted discussion choice: proof must be credible enough for a future
  module author to copy, not just an internal unit-test fixture.
- Defaulted discussion choice: docs should teach "how to add or extend a MESH
  module" through the proof path, while keeping old manifest names out of the
  happy path.

</specifics>

<deferred>
## Deferred Ideas

- Full installer UX, provider conflict UI, package suggestions, resource-pack
  recommendations, and settings materialization/reset policy remain future
  module tooling or installer work.
- Completing all paused v1.6 keybind runtime phases remains future
  `KEYB-01` work beyond this proof phase.

### Reviewed Todos (not folded)
- **Audio Popover Transition Delay Polish:** False-positive match. It remains
  Phase 31 visual polish debt and should not influence Phase 41.
- **Evaluate Blitz crate dependencies:** Future rendering/layout architecture
  spike. It is unrelated to the module proof path.

</deferred>

---

*Phase: 41-Shipped Module Proof and Documentation*
*Context gathered: 2026-05-18T11:15:33Z*
