# Phase 37: Concept Inventory and Vocabulary Lock - Context

**Gathered:** 2026-05-17T20:11:12+02:00
**Status:** Ready for planning

<domain>
## Phase Boundary

Phase 37 defines the canonical MESH module and extensibility vocabulary. It inventories current docs, runtime structs, manifests, diagnostics, and examples, then locks the names and concept boundaries that later phases must implement.

This is a hard consolidation phase: downstream work should replace old names rather than preserve public compatibility aliases. The model must stay usable for module authors and understandable to end users while preserving MESH's ability to support third-party innovation through generic extension points.

</domain>

<decisions>
## Implementation Decisions

### Hard Vocabulary Replacement
- **D-01:** `module` is the canonical public noun for an installable MESH unit. `package`, `plugin`, and similar old public names are replacement debt, not supported public aliases.
- **D-02:** The canonical author-facing manifest should move toward `module.json`. Existing `package.json` language in docs, examples, diagnostics, planning artifacts, and code names should be inventoried as old terminology and scheduled for hard replacement.
- **D-03:** Compatibility aliases are not part of the target public model. If old manifest shapes or field names must exist temporarily for migration sequencing, they should be treated as short-lived internal migration paths with visible diagnostics and removal targets, not as documented synonyms.
- **D-04:** Existing roadmap and requirements language that mentions "compatibility aliases" is stale relative to this discussion. Planning should treat this context as the corrected decision source for Phase 37.

### Developer And End-User Vocabulary
- **D-05:** Developer-facing docs should use one strict vocabulary: module, module kind, interface, provider, contribution, capability, dependency, resource pack, library, settings, and entrypoint.
- **D-06:** End-user-facing UI and diagnostics should avoid internal graph jargon where possible. Users should see concrete nouns such as "Audio provider", "Icon pack", "Theme module", "Missing interface", and "Missing resource", backed by precise technical details for authors.
- **D-07:** Diagnostics should teach the canonical model by naming the exact module id, field path, concept, and replacement wording. They should not say old and new terms are interchangeable.

### Concept Boundaries
- **D-08:** A module is the installable, configurable unit. A module kind describes its primary role: frontend, backend, interface, library, theme, icon pack, font pack, language pack, or future resource pack kinds.
- **D-09:** An interface is a named contract for state, methods, events, types, and capability requirements. It is not provider identity and not Rust service logic.
- **D-10:** A provider is a backend module's implementation of an interface. Frontend modules must depend on interface contracts and must not depend on backend provider modules.
- **D-11:** A contribution is something a module adds to the installed graph: frontend entrypoints, slots, settings schemas, keybind actions, interfaces, providers, libraries, themes, icons, fonts, language resources, sounds, or other typed resources.
- **D-12:** A dependency is something a module needs. A capability is host power granted to a module. A contribution is something a module provides. These three concepts must remain separate in vocabulary, manifests, validation, and diagnostics.
- **D-13:** Library modules are modules that contribute importable Luau code. They do not grant capabilities and do not act as providers unless they also explicitly contribute those separate concepts through valid module declarations.

### Extensibility With Consistency
- **D-14:** MESH should permit innovation by allowing new interfaces, providers, libraries, resource packs, and contributions without service-specific Rust branches.
- **D-15:** Consistency comes from typed registries, strict field names, validation, diagnostics, and author docs, not from blocking independent ideas.
- **D-16:** Interface relationships should remain expressive enough for `base`, `extension`, and `independent` models. Independent interfaces are allowed, but diagnostics and docs should guide authors toward extending a base interface when that preserves interoperability.
- **D-17:** Default modules have no privileged conceptual status. They are reference modules that prove the same model third-party authors use.

### Migration And Planning Consequences
- **D-18:** Phase 37 should produce an inventory table that marks each old term or shape as `replace`, `remove`, or `internal-only migration`, never as `public alias`.
- **D-19:** Later phases should migrate bundled docs, examples, runtime type names where practical, diagnostics, and tests toward the locked vocabulary. If a hard runtime removal would break the current repo, the same phase should migrate shipped artifacts before removing support.
- **D-20:** The paused v1.6 keybind model should be preserved as a module contribution model: keybind actions are contributions, localized triggers are contribution metadata, and user overrides are settings/configuration.
- **D-21:** The v1.1 backend provider model should be preserved as an interface/provider model: provider selection is user configuration over modules implementing interfaces, not a frontend dependency on a backend module.

### Folded Todos
- **Create unified package and module manifest phase:** Folded as vocabulary input. Its manifest naming question is now answered directionally: use module-centered naming and replace package-centered terminology.
- **Define module install requirement resolution:** Folded as concept-boundary input. Phase 37 should make install-time concepts precise enough for Phase 38/39: modules declare what they need, what they contribute, and what host powers they request.

### the agent's Discretion
The planner may decide the exact inventory format, but it must be concrete enough for downstream code and docs work. A table with old term, location, canonical replacement, replacement class, and follow-up phase is preferred.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Milestone And Requirements
- `.planning/PROJECT.md` - Current v1.7 milestone intent, active requirements, out-of-scope boundaries, and locked project decisions.
- `.planning/REQUIREMENTS.md` - v1.7 requirements. Treat CONC-02 compatibility-alias wording as superseded by D-01 through D-04 in this context.
- `.planning/ROADMAP.md` - Phase 37 through 41 sequencing and success criteria.
- `.planning/STATE.md` - Prior decisions from v1.1 backend providers, v1.6 keybind work, and v1.7 consolidation.

### Research And Pending Inputs
- `.planning/research/SUMMARY.md` - v1.7 research summary recommending one module/package/contribution model.
- `.planning/todos/pending/2026-05-08-create-unified-package-and-module-manifest-phase.md` - Original manifest unification todo; folded into Phase 37 vocabulary decisions and Phase 38 implementation context.
- `.planning/todos/pending/2026-05-15-define-module-install-requirement-resolution.md` - Requirement/contribution/provider/resource resolution todo; folded into Phase 37 concept boundaries and Phase 39 extension indexing context.

### Existing Author Docs
- `docs/module-system.md` - Current module model docs. Contains useful concept structure but still uses package-centered naming that Phase 37 should inventory and replace.
- `docs/extensibility.md` - Dynamic interface/provider/extensibility model. Contains an explicit transition note from trait to interface that should become hard replacement guidance.
- `docs/modules/README.md` - Shipped module overview and author path. Contains old package/manifest wording that should be inventoried.
- `docs/health.md` - Health and dependency diagnostics vocabulary that should align with module/interface/provider/resource terminology.
- `docs/theming/icons.md` - Icon pack and semantic icon wording; note any "alias" language that conflicts with the no-public-alias decision.

### Runtime And Manifest Code
- `crates/core/extension/module/src/package/module_manifest.rs` - Current normalized module manifest structs, module kinds, keybinds, provider declarations, and legacy conversion path.
- `crates/core/extension/module/src/package/installed_graph.rs` - Installed graph, provider selection, interface declarations, and typed contribution index.
- `crates/core/extension/module/src/manifest/model.rs` - Compatibility-normalized runtime manifest shape still using `PackageSection` and old fields.
- `crates/core/extension/module/src/manifest/json.rs` - JSON manifest parser for the older runtime manifest shape.
- `crates/core/extension/service/src/interface.rs` - Interface registry and provider resolution vocabulary.
- `modules/frontend/navigation-bar/module.json` - Current shipped frontend module manifest with capabilities, dependencies, keybinds, settings, and surface layout.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `ModulePackageManifest`, `MeshModuleSection`, and `ModuleKind` in `crates/core/extension/module/src/package/module_manifest.rs` already centralize much of the module vocabulary, but names still preserve `Package`.
- `InstalledModuleGraph` and `ModuleContributionIndex` in `crates/core/extension/module/src/package/installed_graph.rs` already model enabled modules, active providers, interface declarations, and typed contributions.
- `InterfaceRegistry` in `crates/core/extension/service/src/interface.rs` already keeps interface and provider resolution generic.
- Shipped module manifests such as `modules/frontend/navigation-bar/module.json` demonstrate concrete fields for capabilities, dependencies, keybinds, settings, and resources.

### Established Patterns
- Rust core stays generic. Service-specific behavior belongs in backend Luau providers and interface contracts.
- Frontend modules consume interfaces, not backend module ids.
- Provider multiplicity is valid, but runtime uses one active provider per interface unless future phases explicitly add coexistence semantics.
- Capabilities are permissions for host power and service access. They must not be inferred from provider identity.
- Diagnostics should be visible and actionable rather than hidden fallback behavior.

### Integration Points
- Manifest and graph vocabulary changes will touch docs, diagnostics, tests, and Rust type names in later phases.
- Phase 38 will need the Phase 37 replacement inventory to normalize or reject old manifest shapes.
- Phase 39 will need the concept boundaries for contribution indexing, interface/provider validation, and resource requirement resolution.
- Phase 40 will need the old-term inventory to update bundled docs and examples without keeping public aliases.

</code_context>

<specifics>
## Specific Ideas

- User correction: replace old names, no compatibility aliases. Example: `package` should be hard-renamed to `module`.
- User goal: search gray areas so the model is usable for developers and end users, extensible but consistent, and still permits innovation.
- Concrete gray areas resolved in this context: hard rename policy, author/end-user vocabulary split, concept boundaries, and extensibility governance.

</specifics>

<deferred>
## Deferred Ideas

- Runtime removal of old manifest support belongs to Phase 38 or later after shipped artifacts are migrated.
- Detailed provider conflict resolution, resource cascade resolution, and settings materialization belong to Phase 38/39 planning, using the vocabulary locked here.
- Completing paused v1.6 keybind dispatch, conflict diagnostics, and accessibility proof remains out of scope until after v1.7 stabilizes the module model.

### Reviewed Todos (not folded)
- **Audio Popover Transition Delay Polish:** Reviewed as a false positive from todo matching. It remains unrelated Phase 31 polish debt and should not influence Phase 37.

</deferred>

---

*Phase: 37-Concept Inventory and Vocabulary Lock*
*Context gathered: 2026-05-17T20:11:12+02:00*
