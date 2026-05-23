# MESH Milestones

## v1.10 Painter Engine (Shipped: 2026-05-23)

**Phases completed:** 9 phases, 29 plans, 36 tasks

**Key accomplishments:**

- Backend-neutral painter command contract with Skia capability reporting and diagnostic handling
- Existing renderer paint helpers lowered through the new painter command backend boundary
- Renderer documentation for the Skia-centric, backend-neutral painter boundary
- Bounded MESH shell CSS profile with executable property metadata and browser CSS exclusions
- StyleResolver token, custom-property, and shipped navigation/audio fixture coverage for the painter profile
- Profile-status-driven diagnostics for unsupported browser-like CSS and accepted-yet-unlowered declarations
- Component parser keyframe tests aligned with the Phase 52 profile plus final style/parser validation proof
- Backend-neutral background image and linear-gradient style data with unsupported-value diagnostics
- Background images and linear gradients lower into backend-neutral painter commands with retained parity
- Skia executes supported layer, image, and linear-gradient painter commands with pixel proof
- Unsupported effect cases diagnose, and retained visual bounds cover supported Phase 55 output
- Phase 55 focused suites and backend-neutrality proof are green, with validation metadata complete
- Explicit animation property buckets with shell-side transition classification proof
- Paint-only animation ticks repaint visually while geometry-changing ticks still relayout
- Paint-only keyframe rules repaint without layout while diagnostics and conservative fallbacks remain intact
- Animated transform and effect damage now covers current and previous visual pixels
- Shipped navigation/audio animation proof plus complete Phase 56 validation metadata
- Retained preclip filtering now uses visual bounds, with debug counters for changed layout, changed paint, effect overflow, and fallback promotion
- Painter backend snapshots now expose backend id, rollback authority, capabilities, and recent diagnostics
- Renderer ownership, migration, and `.mesh` author-contract docs now record the shipped v1.10 painter-engine boundary

**Known deferred items at close:** 2 open artifacts acknowledged and deferred; see `.planning/STATE.md` Deferred Items.

**Archive artifacts:**

- `.planning/milestones/v1.10-ROADMAP.md`
- `.planning/milestones/v1.10-REQUIREMENTS.md`
- `.planning/milestones/v1.10-MILESTONE-AUDIT.md`

---

## v1.9 Renderer Library Integration (Shipped: 2026-05-21)

**Phases completed:** 5 phases, 10 plans, 18 tasks

**Key accomplishments:**

- Rust 1.85-compatible renderer-library candidates are now production manifest entries behind disabled-by-default mesh-core-render features.
- Internal renderer-library feature status records now expose enabled adapter paths while keeping the software renderer as fixed rollback authority.
- Phase 46 renderer-library dependency risk, adapter ownership, author-contract boundaries, and final adoption gates are documented against the disabled-by-default scaffold.
- Taffy layout ownership moved to `mesh-core-elements` with NodeId-keyed diagnostics and Phase 47 replacement docs.
- `LayoutEngine` now uses Taffy as the production geometry source.
- Phase 47 now has explicit parity, shipped-surface, and documentation gates for the Taffy layout replacement.
- Parley proof evidence now covers text shaping and selection geometry while preserving theme-owned selection payloads and default text fallback.
- AnyRender proof evidence now encodes retained display-list background, border, and icon commands behind `renderer-anyrender` while the software painter remains authoritative.
- AccessKit retained-node runtime updates now build real `accesskit::TreeUpdate` values behind `renderer-accesskit`.
- Renderer migration, ownership, and author-contract docs now classify Taffy, Parley, AnyRender, Vello encoding, and AccessKit adoption status.

---

## v1.8 Rendering Engine Architecture (Shipped: 2026-05-18)

**Status:** shipped
**Archived:** 2026-05-18
**Delivered:** A source-backed renderer architecture direction, comparable prototype evidence, production-adjacent focused proof integration, and a phased renderer migration contract for future broad adoption.

**Phases completed:** 4 phases, 14 plans, 28 tasks

**Key accomplishments:**

- Source-backed renderer decision frame with local MESH contracts, external crate sources, hard blockers, and scorecard placeholders
- Renderer crate outcomes and path scores selecting a dual Phase 43 prototype comparison
- Dual renderer prototype handoff for Blitz reference and MESH-owned focused-crate paths across navigation bar and audio popover surfaces
- Shared renderer prototype fixture and isolated Rust evidence schema for comparable Blitz and focused-crate proofs
- Blitz reference path evidence with a reproducible high-level crate compile blocker and structured fixture fallback
- Retained MESH-shaped focused-crate evidence with Taffy layout, Parley text, AnyRender paint, and AccessKit accessibility boundaries
- Final renderer prototype comparison selecting the MESH-owned focused-crate path for Phase 44
- Render-owned focused proof snapshots preserving MESH node identity, typed dirty evidence, damage evidence, selected paint evidence, and AccessKit-compatible node IDs
- Shell paint now captures focused proof snapshots while preserving invalidation snapshots, present damage, profiling, and non-fatal diagnostics
- Focused proof evidence now covers theme-owned selection payloads, selection paint behavior, and AccessKit-compatible updates derived from retained MESH node IDs
- Navigation and audio shipped-surface tests now prove focused snapshots exist during normal paints, with final evidence covering INTG-01 through INTG-04
- Phased renderer migration roadmap with reversible rollout gates, required Nix/Cargo validation commands, and broad-adoption dependency records
- Renderer ownership matrix classifying current MESH boundaries as authoritative, Phase 44 proof outputs as adapter-owned, and future crate paths as replacement candidates
- Author-facing renderer contract for `.mesh` UI with explicit browser non-goals and links from existing module/frontend authoring docs

**Known deferred items at close:** 3 open artifacts acknowledged and deferred; see `.planning/STATE.md` Deferred Items.

**Archive artifacts:**

- `.planning/milestones/v1.8-ROADMAP.md`
- `.planning/milestones/v1.8-REQUIREMENTS.md`

---

## v1.7 Rethink Modularity and Extensibility Concepts (Shipped: 2026-05-18)

**Status:** shipped
**Archived:** 2026-05-18
**Phases completed:** 5 phases, 17 plans, 39 tasks

**Key accomplishments:**

- Canonical module vocabulary with old-name replacement inventory and reconciled provider/keybind model
- Author-facing module, backend, health, and icon docs now teach the canonical module/interface/provider vocabulary
- Runtime terminology inventory and Phase 38-41 handoff for module.json, typed contributions, diagnostics, and shipped proof
- Interface relationship validation and graph-boundary separation for base, extension, independent, provider, and frontend requirement metadata
- Provider metadata and backend diagnostics now keep interface requirements, provider identity, and host capabilities separate
- Installed graph contribution records now carry source metadata and expose typed runtime registries for frontend, resource, keybind, interface, provider, settings, and library data
- Shell startup and backend launch now consume typed installed graph records, with non-fatal resource/settings diagnostics and an end-to-end manifest-driven extension proof
- Manifest migration diagnostics now have test coverage for warning/error severity and author docs with exact replacement/removal actions
- Author-facing docs now teach canonical module.json manifests across installation, resources, settings, frontend examples, and LLM context
- Manifest keybind declarations now flow into installed graph records with default and localized trigger data, while shell shortcut dispatch enforces declared modifiers and preserves settings-only user overrides.
- Canonical audio interface and default icon modules now participate in the real installed graph proof for the shipped navigation/audio path.
- Shell tests now prove the shipped installed graph drives provider registration, active backend selection, frontend filtering, and navigation behavior without service-specific production branches.
- Author docs now teach the shipped navigation/audio proof path as the canonical workflow for extending or adding a MESH module.

**Known deferred items at close:** 4 open artifacts acknowledged and deferred; see `.planning/STATE.md` Deferred Items.

**Archive artifacts:**

- `.planning/milestones/v1.7-ROADMAP.md`
- `.planning/milestones/v1.7-REQUIREMENTS.md`
- `.planning/milestones/v1.7-phases/`

---

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
