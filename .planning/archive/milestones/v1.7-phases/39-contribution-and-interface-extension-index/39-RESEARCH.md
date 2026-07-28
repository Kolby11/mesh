# Phase 39: Contribution and Interface Extension Index - Research

## Research Complete

Phase 39 should turn the normalized module manifests from Phase 38 into a
source-rich installed graph that tools, diagnostics, and shell startup can query
without relying on service-specific Rust branches. The codebase already has the
right center of gravity: `InstalledModuleGraph` indexes enabled modules,
backend providers, frontend requirements, interface declarations, and several
resource contribution kinds. The phase should extend that graph deliberately
rather than create a second registry.

No finalized `39-CONTEXT.md` exists. Planning uses the roadmap requirements, the
partial discuss checkpoint, the Phase 37/38 context, and the pending module
install requirement-resolution note as canonical inputs.

## Existing Implementation

- `crates/core/extension/module/src/package/installed_graph.rs` owns
  `InstalledModuleGraph::from_parts`, validates the root module graph, filters
  runtime contribution indexes to enabled modules, and retains disabled modules
  as graph nodes.
- `InstalledModuleGraph` currently exposes enabled modules by kind, backend
  providers by interface, active providers, frontend dependency requirements,
  interface declarations, interface guidance, layout entrypoint resolution,
  contributed themes, icons, fonts, i18n, libraries, and settings schemas.
- `ModuleContributionIndex` currently indexes layout, themes, icons, fonts,
  i18n, libraries, and settings. Records carry `module_id` and contribution
  fields, but they do not share common source metadata or scoped ids.
- `InstalledModuleNode` keeps `id`, `kind`, root graph path, enabled state, and
  `ModuleManifest`. `LoadedModuleManifest` has richer manifest source
  information, but that source/path data is not retained on installed nodes or
  contribution records.
- `crates/core/extension/module/src/package/module_manifest.rs` already has
  `MeshInterfaceDeclaration`, `InterfaceRelationship::{Base, Extension,
  Independent}`, `MeshProvidesDeclaration`, `MeshDependencies`, keybind actions,
  icon packs, icon requirements, theme resources, and `MeshContributes`.
- Interface relationship inference is permissive today: `extends` implies
  extension, `mesh.*` implies base, and everything else defaults to independent.
  Explicit contradictory combinations are not rejected.
- `crates/core/extension/service/src/interface.rs` owns contract/provider
  resolution with sorted provider priority and canonical interface names.
- `crates/core/shell/src/shell/discovery.rs` still registers interface
  contracts/providers by scanning runtime manifests and only uses the installed
  graph to filter enabled frontend modules.
- `crates/core/shell/src/shell/backend/candidates.rs` uses the installed graph
  for backend launch candidates and requirement status, then cross-checks
  selected providers against `InterfaceRegistry`.

## Discuss Checkpoint Decisions

The completed checkpoint area is Contribution Index Shape:

- Use strict typed registries rather than one untyped contribution list.
- Each contribution record should be source-rich.
- Contribution ids should be scoped ids.
- Runtime indexes should include enabled modules only; disabled installed
  modules remain available as catalog metadata.

The remaining discussion areas are covered by planning assumptions:

- Interface relationships get validation invariants, but independent interfaces
  remain allowed with guidance when a base exists for the same domain.
- Provider declarations, frontend dependencies, and host capability requests
  stay separate concepts in data structures and diagnostics.
- Resource and settings resolution is graph-owned metadata plus diagnostics in
  this phase; final author documentation remains Phase 40 work.

## Recommended Technical Approach

1. Keep `InstalledModuleGraph` as the runtime authority. Add missing typed
   registries there instead of adding a parallel shell-only index.
2. Introduce a small reusable `ContributionSource`/`ContributionId` shape with:
   - `module_id`
   - `module_kind`
   - `module_path`
   - `manifest_path`
   - `manifest_source`
   - `local_id`
   - `scoped_id` using `<module-id>:<local-id>` for module-local ids.
3. Retain disabled modules in `InstalledModuleNode`, including manifest path and
   manifest source, but only call `ModuleContributionIndex::index_module` for
   enabled modules.
4. Add typed indexes and getters for frontend entrypoints, layout contributions,
   settings schemas, keybind actions, resource requirements/contributions,
   interface declarations, and backend provider declarations.
5. Harden interface relationship validation:
   - `relationship = "extension"` requires `extends`.
   - `relationship = "base"` must not set `extends`.
   - `relationship = "independent"` must not set `extends`.
   - independent same-domain interfaces remain valid and produce guidance.
6. Keep provider identity separate from host permissions. Backend provider
   records may expose declared module capabilities for diagnostics, but
   frontend backend dependencies must never imply `service.*.read` or
   `service.*.control`.
7. Move shell integration toward graph-derived contracts/providers so discovery,
   backend launch, and diagnostics use the same typed metadata.

## Implementation Notes

- Avoid broad schema churn. Canonical manifest fields already exist; Phase 39 is
  primarily indexing, validation, and shell consumption.
- Keep legacy runtime manifest conversion behavior intact unless a typed index
  requires data already present in `ModuleManifest`.
- Slot definitions/contributions may still exist on legacy normalized manifests.
  If this phase indexes slots, bridge from normalized runtime data only when the
  canonical manifest path has a clear equivalent; otherwise leave a typed TODO in
  diagnostics rather than inventing a new public schema silently.
- Resource packs should be represented as typed contributions even before full
  lookup/cascade behavior exists. Missing semantic icon/font/i18n requirements
  should produce graph diagnostics rather than fail unrelated module loading.
- Tests should assert the separation of backend provider availability, selected
  provider, interface contract availability, and host capability permission.

## Validation Architecture

Use existing Rust cargo tests with focused package and shell suites.

- Quick package command: `cargo test -p mesh-core-module package::tests`
- Interface catalog command: `cargo test -p mesh-core-service interface::tests`
- Shell backend command: `cargo test -p mesh-core-shell shell::tests backend`
- Full shell command: `cargo test -p mesh-core-shell shell::tests`
- Full validation command: `cargo test --workspace`

Required automated coverage:

- explicit interface relationship contradictions produce validation errors.
- independent same-domain interfaces remain valid and produce guidance.
- frontend backend dependency checks report missing provider and no active
  provider without granting capabilities.
- provider declarations are indexed separately from interface dependencies and
  capability requests.
- enabled modules contribute runtime typed records; disabled modules remain
  installed catalog nodes but do not appear in runtime contribution getters.
- contribution records carry source metadata and stable scoped ids.
- keybind, settings, frontend entrypoint, library, resource, interface, and
  provider contributions are inspectable through typed installed graph getters.
- shell backend launch and interface registration can route through graph data
  without adding service-specific Rust branches.

## Open Risks

- Adding common source metadata can become noisy if every contribution struct is
  rewritten manually. Prefer one reusable source struct embedded in typed
  records.
- Existing tests may rely on permissive interface relationship inference. Keep
  inference permissive when relationship is omitted; only reject explicit
  contradictory declarations.
- Shell discovery still owns several runtime registration paths. Integrate
  graph-derived data in small steps and leave legacy fallback behavior intact
  when graph loading fails.
- Full user settings cascade is larger than Phase 39. This phase should index
  settings/resource declarations and surface diagnostics, not finish every
  shell-settings override rule.
