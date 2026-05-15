# MESH Milestones

## v1.5 CPU Rendering Performance Improvement

**Status:** shipped
**Started:** 2026-05-10
**Archived:** 2026-05-13

**Goal:** Research Qt Quick renderer patterns and current MESH CPU bottlenecks, then implement the retained pipeline, culling, and caching improvements needed to make shipped shell surfaces feel visibly smoother on the software renderer.

**Delivered:**

- Attribute CPU render cost across canonical benchmark scenarios and shipped proof surfaces
- Prune offscreen, hidden, and clip-excluded content earlier in the retained paint pipeline
- Replace whole-tree retained paint-command recollection with dirty-subtree updates
- Restrict partial-damage paints to commands that actually intersect the changed region
- Cache expensive SVG, bitmap, icon, text, and glyph raster work more aggressively
- Tune repaint heuristics for visible smoothness while keeping GPU and parallel rendering out of scope
- Close live audio surface UAT regressions in popover open/close, slider grabbing, and mute-state reconciliation

**Stats:** 6 phases, 10 plans, 17 requirements shipped

**Deferred at close:** Slight audio popover transition delay accepted as polish by user request.

**Archive artifacts:**

- `.planning/milestones/v1.5-ROADMAP.md`
- `.planning/milestones/v1.5-REQUIREMENTS.md`
- `.planning/milestones/v1.5-MILESTONE-AUDIT.md`

## v1.7 Rethink Modularity and Extensibility Concepts

**Status:** planning
**Starts after:** v1.6 Localized Keybind Management pause

**Goal:** Rework MESH's modularity model so frontend modules, backend providers, manifests, service contracts, capabilities, and extension points form a coherent author-facing architecture instead of a set of separate milestone-grown mechanisms.

**Priority:** high. MESH now has useful extension pieces across manifests, backend providers, interfaces, capabilities, resources, keybinds, diagnostics, and docs; consolidating the model will reduce friction before more extension features are added.

**Planned scope:**

- Define canonical vocabulary for package/module identity, frontend surfaces, backend providers, interface contracts, libraries, resource packs, contributions, capabilities, and dependencies
- Normalize `package.json.mesh` manifest schema and compatibility behavior for existing manifests
- Index typed contributions for UI entrypoints, slots, libraries, settings, keybinds, resources, interfaces, and providers
- Preserve v1.1 backend provider behavior and v1.6 keybind declaration/resolution behavior during migration
- Prove the model with docs, diagnostics, tests, and at least one real bundled module/provider path

**Out of scope:**

- Remote marketplace, signing, trust policy, or installer UX
- Compositor-global shortcuts
- Completing all paused keybind dispatch/conflict/accessibility runtime work
- Service-specific Rust APIs
- Skia-backed rendering investigation

## v1.6 Localized Keybind Management

**Status:** paused
**Starts after:** v1.5 CPU Rendering Performance Improvement

**Goal:** Let frontend modules declare semantic keybind actions that scripts can handle, while the shell resolves localized defaults, user overrides, conflicts, scope, and accessibility metadata.

**Paused:** 2026-05-15 after phases 32 and 33 to prioritize v1.7 modularity and extensibility concept consolidation.

**Delivered before pause:**

- Manifest-backed semantic keybind declarations with stable action ids, handlers, target references, scopes, labels/i18n keys, and trigger metadata
- Compatibility bridge from `settings.keyboard.shortcuts` into the same declaration model
- User overrides keyed by surface id and action id rather than localized labels
- Locale-aware trigger resolution with user override, exact locale, parent locale, generic trigger, then no-binding precedence
- Localized trigger defaults scoped to `access_key` actions while shortcut actions retain generic defaults unless overridden

**Priority:** high. Plugin authors need declared, localizable keyboard actions before MESH expands into compositor-global shortcuts or broader settings UI.

**Planned scope:**

- Add module manifest/settings support for semantic keybind actions with stable ids, handlers, labels, scopes, triggers, and target controls
- Resolve effective keybinds from module defaults, active locale, and user overrides with deterministic precedence
- Support localized access-key defaults such as English `Accept -> A` and Slovak `Prijat -> P`
- Preserve existing shell-global shortcuts, text input, focus traversal, and focused widget key behavior
- Emit non-fatal diagnostics for malformed, duplicate, or unresolved keybinds
- Expose resolved keybind metadata through accessibility annotations and prove behavior on shipped surfaces

**Out of scope:**

- Compositor-global shortcuts through XDG Desktop Portal or compositor-specific APIs
- Full user-facing keybind settings UI
- Automatic translation or automatic access-key generation
- Replacing existing keyboard focus traversal or widget activation behavior
- Skia-backed rendering investigation, now deferred beyond v1.6

## v1.4 Major Performance Fixes

**Status:** shipped
**Started:** 2026-05-09
**Archived:** 2026-05-09

**Goal:** Continue retained-rendering performance work by adding dirty-type invalidation, incremental style/layout propagation, retained render/display data, damage tracking, and batching foundations before GPU backend work.

**Delivered:**

- Typed invalidation for style, layout, paint, text, accessibility, metrics, and surface configuration changes
- Incremental style/layout propagation over retained widget identity
- Retained render objects synchronized from stable widget nodes
- Retained display list plus damage tracking and partial-present metrics
- Text layout caching, selector indexing, typed slots, and batching barrier counters
- GPU-readiness guardrails without implementing a GPU backend

**Stats:** 7 phases, 7 plans, retained CPU rendering foundations shipped

**Archive artifacts:**

- `.planning/milestones/v1.4-phases/`

## v1.3 Performance Instrumentation and Responsiveness

**Status:** shipped
**Started:** 2026-05-08
**Archived:** 2026-05-09

**Goal:** Make shell responsiveness measurable on real interaction paths by adding a debug-only live profiler, a `.mesh`-rendered inspector, canonical benchmark scenarios, and a bounded optimization pass driven by those measurements.

**Delivered:**

- Profiling runtime model and stage timing hooks
- Per-surface and per-backend-provider/stage attribution
- Debug-overlay profiling mode with live inspector views
- Canonical benchmark flows for hover, open/close, slider drag, keyboard traversal, and backend-driven updates
- Retained widget-tree foundation with stable node identity and dirty summaries

**Stats:** 5 phases, 16 plans, retained widget-tree foundation shipped

**Archive artifacts:**

- `.planning/milestones/v1.3-ROADMAP.md`
- `.planning/milestones/v1.3-REQUIREMENTS.md`

## v1.2 Rendering System Upgrade

**Status:** shipped
**Started:** 2026-05-05
**Archived:** 2026-05-08

**Goal:** Make MESH frontend rendering expressive and interactive enough for distinctive shell UI without turning the renderer into a full browser engine.

**Delivered:**

- Practical CSS coverage with diagnostics, shorthands, token resolution, and author docs
- Container-size and interaction-state reactivity without losing runtime or service state
- Selectable rendered text with copy support and theme-owned highlight rendering
- Keyboard navigation, focus-visible behavior, and configurable shortcuts on shipped surfaces
- Theme animation tokens and constrained CSS animation playback with diagnostics
- Navigation-bar migration as the milestone proof surface

**Stats:** 6 phases, 26 plans, 26 requirements shipped

**Deferred at close:** 4 acknowledged items recorded in `.planning/STATE.md`

**Archive artifacts:**

- `.planning/milestones/v1.2-ROADMAP.md`
- `.planning/milestones/v1.2-REQUIREMENTS.md`

## v1.1 Backend Plugin MVP

**Status:** shipped
**Started:** 2026-05-03
**Archived:** 2026-05-05

**Goal:** Make backend plugins stable enough for MVP: core backend concepts work predictably, plugins can run service logic, emit state, receive config, log, execute host commands, expose service contracts, and surface failures clearly.

**Delivered:**

- Shell-owned installed-plugin package manifest and normalized plugin graph
- Deterministic backend runtime lifecycle with explicit active-provider ownership
- Locked backend Luau MVP host API centered on structured `mesh.exec(program, args)`
- Generic service state publication and command routing
- Backend diagnostics hardening, reference backend proof plugin, and author docs
- Milestone planning reconciliation so the archive matches the shipped contract

**Stats:** 6 phases, 21 plans, 63 tasks, 28 requirements shipped

**Deferred at close:** 3 validation/verification cleanup items recorded in `.planning/STATE.md`

**Archive artifacts:**

- `.planning/milestones/v1.1-ROADMAP.md`
- `.planning/milestones/v1.1-REQUIREMENTS.md`

## v1.0 Scripting API Stabilization

**Status:** archived by reset
**Archived:** 2026-05-03
**Reason:** User reset active roadmap to focus on backend plugin MVP fundamentals before continuing frontend documentation or distribution work.

**Archive artifacts:**

- `.planning/milestones/v1.0-reset-2026-05-03-ROADMAP.md`
- `.planning/milestones/v1.0-reset-2026-05-03-REQUIREMENTS.md`
- `.planning/milestones/v1.0-reset-2026-05-03-STATE.md`
- `.planning/milestones/v1.0-reset-2026-05-03-phases/`
