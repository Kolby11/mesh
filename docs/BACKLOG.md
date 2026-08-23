# MESH — Active Backlog

The single list of what is open. Specifications describe contracts
([`spec/`](spec/)); guides describe current behavior; history and measurements
live in [`.planning/log/`](../.planning/log/).

**Items here say what to do and why it is not done — nothing else.** Progress
narratives, benchmark numbers, and completed items belong in the log. When an
item lands, delete it from this file and write its record in
[`.planning/log/`](../.planning/log/README.md).

Verify an older item against the source before starting it; later work
sometimes lands without updating a checkbox.

**Detail** for items carried over from before 2026-07-28 is in
[`.planning/log/backlog-archive-2026-07-28.md`](../.planning/log/backlog-archive-2026-07-28.md)
— a verbatim snapshot of this file with the full progress history. Items marked
_(detail: …)_ name the section to search for there. Section letters (A–V) refer
to [`.planning/log/sections.md`](../.planning/log/sections.md); `→ vX.Y` markers
are from the retired milestone scheme and are kept only as rough sequencing.

---

## Shell features

## Section audit feature checklist

These are the feature-level items extracted from the section improvement audits.
The audit files retain the process maps, evidence, design detail, and regression
matrices; this checklist is the canonical place to track implementation.

### 1. Core foundation contracts

The six primary findings in this audit shipped on 2026-08-20 and are recorded in
the monthly log, so they are intentionally absent from the open backlog.
[Audit](../.planning/log/sections/01-core-foundation-contracts/improvements.md)

### 2. Module system and installation

[Audit](../.planning/log/sections/02-module-system-and-installation/improvements.md)


### 3. Service contracts

[Audit](../.planning/log/sections/03-service-contracts/improvements.md)


### 4. Themes

[Audit](../.planning/log/sections/04-themes/improvements.md)

### 5. Localization and i18n

[Audit](../.planning/log/sections/05-localization-i18n/improvements.md)

### 6. Host resources and icon packs

[Audit](../.planning/log/sections/06-host-resources-and-icon-packs/improvements.md)

- [ ] Prepare resource parsing and asset handles away from shell/render threads with bounded cancellation.
- [ ] Generate runtime, CLI, LSP, doctor, and debug resource explanations from one effective snapshot.
- [ ] Add a resource coverage advisor for semantic vocabulary and font-script gaps without silently reordering user chains.

### 7. Component language

[Audit](../.planning/log/sections/07-component-language/improvements.md)

- [ ] Make local component imports module-relative, canonicalized, contained, and symlink-safe across compiler, watcher, and LSP paths.
- [ ] Resolve recursive imports by owner scope and canonical target, with collision and cycle diagnostics.
- [ ] Replace lossy block extraction with a span-preserving top-level parser that validates required, unique, attributed blocks and `<i18n>` policy.
- [ ] Parse interpolations and control-flow braces with a real lexer/parser that rejects malformed expressions and preserves spans.
- [ ] Add semantic validation linking `<props>`, `prop()` references, child props, visibility, types, and CSS domains.
- [ ] Select the highest valid value across prop/configuration layers while retaining invalid overrides only in diagnostics.
- [ ] Make JSON-to-prop conversion scalar-only until an explicit structured prop type exists.
- [ ] Normalize and validate `PropDef` constraints, units, options, tokens, and CSS-domain values once.
- [ ] Replace line-oriented script/import/symbol scans with one Luau lexer/parser and source metadata.
- [ ] Carry reliable source spans through component AST nodes, compiler errors, CLI diagnostics, and LSP output.

### 8. UI element core

[Audit](../.planning/log/sections/08-ui-element-core/improvements.md)

- [ ] Normalize roles, names, descriptions, hidden state, ARIA aliases, focus, relationships, and visibility after child construction and publish the semantic snapshot.
- [ ] Use visible descendant text in accessible-name precedence and preserve hidden-child and locale behavior.
- [ ] Generate one typed pseudo-state table for indexing, mutation, matching, invalidation, diagnostics, and tests.
- [ ] Make compiler inheritance matching consume the actual element state.
- [ ] Make retained layout transactional, preserve last-known-good geometry, and retry after failure.
- [ ] Use one stateful input dispatcher with pointer capture, press-origin identity, activation semantics, focus eligibility, and invalidation output.
- [ ] Generate element types, source/runtime tags, contracts, events, attributes, style hooks, and accessibility defaults from one schema.
- [ ] Detect CSS custom-property cycles with structured diagnostics and specified invalid-value fallback.
- [ ] Preserve explicit/inherited property masks through retained style resolution and targeted restyle.
- [ ] Derive accessibility focus from canonical live focus state during snapshot generation.
- [ ] Include all shaping inputs and resource/measurer revisions in text measurement contexts and cache keys.
- [ ] Make popover placement tokens and trigger/surface relationships typed, validated, and observable across promotion.
- [ ] Introduce an immutable frame snapshot with phase stamps, semantic diffs, stable identities, and property-based invariant tests.

### 9. Interaction and motion

[Audit](../.planning/log/sections/09-interaction-and-motion/improvements.md)

- [ ] Share visibility, transformed geometry, disabled/inert eligibility, and target filtering across interaction, rendering, focus, scrolling, tooltips, and accessibility.
- [ ] Use one affine transform/clip contract for hit testing, paint bounds, scrolling, and focus geometry.
- [ ] Prevent disabled and inert nodes, including descendants and captured targets, from receiving activation.
- [ ] Preserve keyframe progress across pause/resume and iteration-boundary changes.
- [ ] Add a `MotionPolicy` snapshot for reduced motion across transitions, keyframes, scrolling, inertia, tooltips, and surfaces.
- [ ] Consolidate focus, pointer capture, press origin, gesture ownership, and scroll ownership into one transaction.
- [ ] Preserve the previous discrete value during visibility transitions until the transition completes.
- [ ] Propagate validated per-keyframe easing through component parsing, shell state, and animation sampling.
- [ ] Keep interaction policy in a renderer-neutral frame/state-machine contract with typed decisions and dirty outputs.
- [ ] Implement `box-shadow` parsing with structured errors or reject it before the public animation API.
- [ ] Give animation instances stable identity and explicit replacement, cancellation, and reversal semantics.
- [ ] Introduce an `InteractionFrame` shared by input, state, style invalidation, layout, animation, paint, and semantics.

### 10. Frontend compiler and host

[Audit](../.planning/log/sections/10-frontend-compiler-and-host/improvements.md)

- [ ] Reject absolute, traversal, and symlinked frontend entrypoint/import paths outside the module root.
- [ ] Keep service payloads Rust-owned unless the consumer has the resolved read capability; event subscriptions alone must not expose state.
- [ ] Reload primary and contribution roots together as one atomic catalog generation.
- [ ] Validate contribution roots against interface requirements, availability, and version ranges.
- [ ] Validate root and nested expression scopes uniformly using parser/runtime symbol tables and source spans.
- [ ] Compile expressions once into shared semantics so preview, live, translation, and composition paths behave identically.
- [ ] Split the frontend host into a renderer-neutral ABI and shell adapters with typed capability-scoped effects.
- [ ] Build one reverse dependency graph over primary and contribution roots for invalidation.
- [ ] Scope local component aliases by owner and canonical source, rejecting collisions.
- [ ] Replace line-oriented Luau symbol discovery with parser/compiler metadata or remove it as a source of truth.
- [ ] Make expression scanning UTF-8 safe and diagnose non-ASCII input instead of panicking.
- [ ] Publish runtime props transactionally so Rust and Luau cannot diverge after a failed update.
- [ ] Preserve typed diagnostic categories and source spans through AST, compiler, shell, LSP, and debug paths.
- [ ] Prevent stale catalog rollback from overwriting a newer generation using a coordinator or compare-and-swap.
- [ ] Enforce distinct popover and overflow child-surface lifecycle, focus, dismissal, placement, and ownership semantics.
- [ ] Dispatch frontend `mount` and `unmount` hooks in every initialization, reload, deactivation, and replacement path.
- [ ] Contain expression/runtime failures with last-known-good trees or bounded error placeholders and actionable diagnostics.
- [ ] Publish normalized public-prop schemas and validate imported/contribution props at the import boundary.
- [ ] Reject local/module import alias collisions or resolve them through one typed import namespace.
- [ ] Introduce a coherent revisioned `FrontendFrame` boundary for tree, catalog, runtime, services, invalidation, diagnostics, and effects.
- [ ] Emit immutable compiled frontend revisions and authorize typed effects only when catalog/runtime revisions still match.

### 11. Luau runtime and sandbox

[Audit](../.planning/log/sections/11-luau-runtime-and-sandbox/improvements.md)

- [ ] Enforce one sandbox/resource policy for every Luau realm, including instruction, memory, output, queue, storage, and child-process budgets.
- [ ] Keep service state Rust-owned, generation-aware, and capability-filtered instead of exposing shared globals.
- [ ] Route every backend early return, callback failure, stop, and crash through idempotent cleanup and one terminal lifecycle record.
- [ ] Make unsubscribe-safe event iteration and independent subscriber failure reporting explicit.
- [ ] Give stream subprocesses stable identities, bounded queues, exit events, awaited reaping, and shutdown semantics.
- [ ] Move `mesh.exec` off the async backend loop and enforce cancellation, deadlines, executable policy, and output limits.
- [ ] Replace basename executable grants with canonical path/argument capability policy.
- [ ] Move default runtime storage to secure durable XDG state with permissions, quotas, atomic recovery, and revisioned writers.
- [ ] Make backend command dispatch typed, correlated, transactional, coalescable, generation-aware, and bounded.
- [ ] Gate event and side-effect queues through one typed authorization/resource boundary before acceptance.
- [ ] Represent locale as a host-owned per-context cell updated with the translation snapshot.
- [ ] Treat Luau `nil` writes as explicit deletions in Rust state and dependency invalidation.
- [ ] Restrict provider event publication to provider-owned handles and isolate subscriber callback failures.
- [ ] Preserve one stable backend `self` and event/storage handles for each runtime generation.
- [ ] Publish validated initial provider state before atomically marking the provider ready.
- [ ] Compile and swap a fresh Luau environment on reload, preserving only explicit durable storage.
- [ ] Stage backend top-level execution so host side effects cannot escape a failed `start(self)`.
- [ ] Bound command/event ingress, JSON depth/bytes, event counts, and aggregate runtime resource budgets.
- [ ] Separate effective read and control grants and enforce them through both proxy and shell paths.
- [ ] Reconcile changed poll intervals after every stream callback.
- [ ] Return recoverable host-installation errors instead of panicking during backend setup.
- [ ] Introduce a per-module `RuntimeSession` combining realm, host, resource broker, lifecycle, state, supervisor, health, backoff, and quarantine.

### 12. Rendering and paint

[Audit](../.planning/log/sections/12-rendering-and-paint/improvements.md)

- [ ] Lower asymmetric four-edge borders and four-corner radii correctly.
- [ ] Use one cumulative affine transform/clip model across paint, damage, blur, descendants, and interaction.
- [ ] Lower node opacity and blend mode as isolated compositing groups instead of per primitive.
- [ ] Include paint-order/topology changes in display-list generations and stable equal-z ordering.
- [ ] Unify dirty contracts and paint signatures for all content/style fields, including controls, text, icons, and variables.
- [ ] Include font/resource revisions in text style painting and all glyph/font/text cache keys.
- [ ] Make backdrop blur an explicit renderer/backend capability with validated fallback behavior.
- [ ] Use one fractional-scale rounding policy across layout, buffer, paint, copy, and protocol damage.
- [ ] Preserve release batch/barrier metrics in diagnostics and debug telemetry.
- [ ] Validate caller lineage before trusting generation shortcuts.
- [ ] Include resource revisions in retained-present decisions and cache invalidation.
- [ ] Move resource decode and rasterization off the frame thread through a bounded resource broker.
- [ ] Enforce byte- and dimension-based cache budgets for decoded assets, fonts, glyphs, text, Skia, PixelBuffer, and SHM allocations.
- [ ] Introduce a typed frame paint plan with immutable inputs, topology, transforms, effect regions, replay spans, and exact logical/device damage.

### 13. Surface policy and configuration

[Audit](../.planning/log/sections/13-surface-policy-and-configuration/improvements.md)

- [ ] Enforce the author-only `promotable` guard for settings, IPC, and live role changes.
- [ ] Remove or formally protect `promotable` from user-overridable/ejected settings.
- [ ] Validate manifest surface enums and role constraints with canonical parsers before graph resolution.
- [ ] Include blur and window decorations in semantic presentation change detection.
- [ ] Route settings-driven role reloads through the same transactional transition supervisor as explicit promotion.
- [ ] Share role-field metadata across manifest diagnostics, settings validation, ejection, and protocol lowering.
- [ ] Preserve localization identity and distinguish effective values from overrides during configuration ejection.
- [ ] Use typed unmeasured/content/surface/wire extents at the shell/presentation boundary.
- [ ] Replace split surface field lists with a revisioned semantic policy diff and generation.
- [ ] Build the policy compiler producing declared contracts, effective snapshots, and typed transition plans.

### 14. Wayland platform and presentation

[Audit](../.planning/log/sections/14-wayland-platform-and-presentation/improvements.md)

- [ ] Return typed create/configure/present/lost outcomes, cache only accepted generations, and retain damage for missing surfaces.
- [ ] Route close, dismiss, parent destruction, role replacement, and connection loss through one idempotent teardown supervisor.
- [ ] Gate popup reposition by negotiated protocol version and validate popup role, parent, identity, and reparenting.
- [ ] Use one authoritative logical extent for paint, buffer validation, viewport, regions, attach, and queries.
- [ ] Commit opaque, blur, and other surface-state changes even when pixel damage is empty.
- [ ] Separate frame-callback waiting from buffer-release backpressure and prevent hot retry loops.
- [ ] Make input ownership per seat and cancel pointer, touch, gesture, focus, and repeat transactions during teardown.
- [ ] Validate buffer length/stride/scale, propagate attach/region errors, expose connection loss, and reap clipboard children.
- [ ] Replace the recorder backend with a deterministic protocol-state lifecycle simulator plus a focused live compositor matrix.
- [ ] Complete IME/text-input-v3 support with composition decoration and live compositor/lifecycle coverage; per-seat objects, atomic `done` transactions, deletion, surrounding-text publication, and inline preedit projection are now wired.
- [ ] Build the transactional, capability-aware presentation engine with version reporting, reactive popups, multi-output membership, per-seat input, and shared SHM/dmabuf/GPU frame contracts.

### 15. Shell core and orchestration

[Audit](../.planning/log/sections/15-shell-core-and-orchestration/improvements.md)

- [ ] Replace split profile/runtime mutation with one immutable activation plan and atomic runtime-generation commit.
- [ ] Reconcile backend enable/disable and every graph delta through the same activation coordinator.
- [ ] Tag backend tasks, bridges, messages, events, results, and restart deadlines with activation generations and provider epochs.
- [ ] Make frontend/backend stop, unmount, storage flush, worker join, and shutdown cleanup explicit and idempotent.
- [ ] Give detached workers owned wake handles and lifecycle guards that outlive the eventfd until join.
- [ ] Isolate component/runtime callback, tick, build, render, and reload failures with placeholders, diagnostics, and quarantine.
- [ ] Replace static startup watching with a managed generation-aware watch set and bounded polling fallback.
- [ ] Process CoreRequest effects through one fair bounded scheduler with causal budgets and cycle detection.
- [ ] Publish provider unavailable/recovery transitions from committed provider generations.
- [ ] Route control-plane writes through declared durable revisions and ordered settings/theme/locale effect batches.
- [ ] Distinguish intentional legacy/no-profile operation from invalid configured graph/profile recovery.
- [ ] Connect package journal commit/rollback to runtime activation so disk and live state share one recoverable transaction.
- [ ] Introduce `ActiveSnapshot` with candidate preview in hidden surfaces and an explicit quiescing-to-stopped shutdown state machine.

### 16. Developer and authoring tools

[Audit](../.planning/log/sections/16-developer-and-authoring-tools/improvements.md)

- [ ] Make uninstall path-safe and reject traversal/symlink escapes before recursive deletion.
- [ ] Make package mutations atomic, journaled, crash-recoverable, and failure-injection tested.
- [ ] Make live profile switching use typed acknowledgements and exact-generation recovery.
- [ ] Make CLI and shell use one package ownership and transaction contract.
- [ ] Refresh one canonical graph-authoring snapshot for CLI, doctor, LSP, and runtime consumers.
- [ ] Generate LSP manifest/schema validation from runtime contracts instead of duplicated under-approximations.
- [ ] Convert all LSP positions to UTF-16 code units at the protocol boundary.
- [ ] Make `--replace` and CLI flags enforce their documented typed behavior.
- [ ] Make LSP analysis syntax-aware with recoverable partial ASTs and useful mid-edit diagnostics.
- [ ] Replace heuristic Luau/JSON scanners with standards-compliant parsers and source-span data.
- [ ] Support workspace-folder-only initialization and versioned `didChange` refresh generations.
- [ ] Select manifest flavors from parsed structure, not textual substring checks.
- [ ] Make hover consume the current registry and expose service field/command documentation.
- [ ] Restrict definition resolution to existing module-contained imports.
- [ ] Preserve Unicode JSON escapes, registry generations, and secure import provenance in authoring diagnostics.

## Foundation contracts

## Module system

The 2026-06-18 redesign largely shipped: canonical `module.json` with
`mesh.uses` / `mesh.provides` / `mesh.implements`, the graph as single source of
truth, typed graph diagnostics, library modules, and resource packs. Remaining:

- [ ] Close module filesystem escapes before any package mutation or source
      read: validate canonical module IDs, contain all entry/import/asset paths,
      reject Git symlinks, and remove CLI uninstall traversal. [Audit](../.planning/log/sections/02-module-system-and-installation/improvements.md).
- [ ] Make installed-graph activation fail closed and build catalogs/provider
      registries only from its enabled, compatible candidate set; preserve the
      last known-good runtime when graph validation fails. [Audit](../.planning/log/sections/02-module-system-and-installation/improvements.md).
- [ ] Replace the duplicate CLI/shell package implementations with one locked,
      journaled transaction engine covering source, root, lock, profiles, and
      runtime preparation/commit/rollback. [Audit](../.planning/log/sections/02-module-system-and-installation/improvements.md).
- [ ] Enforce required/optional module dependencies, module/interface version
      ranges, composition pins/closure, and duplicate contract/contribution IDs
      before activation rather than leaving conflicts diagnostic-only.
- [ ] Make the canonical `module.json` loader the only production path; legacy
      manifests remain migration diagnostics, never runnable compatibility
      inputs.
- [ ] Make module lifecycle and health authoritative across graph, frontend, and
      backend states, including explicit unload/recovery/quarantine and service
      unavailability delivery.
- [ ] Align the lock schema with the specification and populate dependency and
      composition provenance so uninstall/update/rollback decisions operate on
      complete state.
- [ ] Move the remaining built-in debug and theme/locale service behavior
      behind generic providers. Startup sounds and backend profiling use the
      generic contract/runtime path; core-owned service state still branches.
      _(detail: "Module system — remaining open follow-ups")_
- [ ] **Deferred — unify the four contribution schemas.** Theme, icons, i18n,
      and keybinds under one `contributes` shape, only where they share honest
      structure. Revisit after profiles land. Capability inference and a
      parallel inline-interface path were both rejected: they trade conceptual
      simplicity for typing simplicity, which is the failure mode that redesign
      set out to avoid.

## Service contracts

- [ ] Replace the additive interface registry with one immutable, atomic
      graph-derived service catalog binding each consumer to the active compatible
      contract/provider/version/policy generation. [Audit](../.planning/log/sections/03-service-contracts/improvements.md).
- [ ] Compile and enforce complete contracts end to end: canonical unique names,
      recursive named/array types, consistent optional semantics, typed method
      inputs/results, last-known-good state, and event payloads.
- [ ] Make service methods correlated request/response transactions with typed
      invocation failures, deadlines/cancellation, explicit coalescing outcomes,
      and rollback/settlement for optimistic `stateBinding` writes.
- [ ] Gate provider/profile commits on validated readiness and buffered initial
      state; immediately deliver unavailability and reject all state/events/results
      from stopped or obsolete provider generations.
- [ ] Diff the complete compiled contract in consumer and provider directions,
      load external candidate contracts, align LSP/static analysis/codegen with
      the runtime ABI, and remove the unused transitional `ServiceRegistry`.

## Themes

- [ ] Make graph-authorized theme descriptors the only catalog/loader, with
      contained mode sources and one composed base/pack/module/user cascade;
      remove the private `config/themes` manifest and legacy JSON paths. [Audit](../.planning/log/sections/04-themes/improvements.md).
- [ ] Make theme selection, profile switching, graph changes, and hot reload one
      durable prepare/commit transaction that preserves the last-known-good
      snapshot, refreshes watches/catalog state, and never exits on bad CSS.
- [ ] Share the restricted CSS/token/keyframe lowering path between themes and
      components so pseudo-states, inherited custom properties, token recipes,
      and general theme keyframes work or produce source-located diagnostics.
- [ ] Complete `mesh.theme` and settings with modes, sparse token overrides,
      provenance, explicit color-scheme/contrast, and revisioned authoritative
      events instead of ID heuristics and provider-overwritable render facts.

## Localization / i18n

- [ ] Replace per-surface locale engines and the cross-module global key pool
      with one graph-authorized immutable catalog snapshot, strict module/interface
      scopes, provenance, and atomic last-known-good refresh. [Audit](../.planning/log/sections/05-localization-i18n/improvements.md).
- [ ] Make locale selection a durable revisioned settings/profile transaction:
      normalize BCP 47, derive the full fallback chain and direction, apply each
      module's default terminally, and remove stale/per-component locale mutation.
- [ ] Compile bounded typed catalogs with per-entry diagnostics, interpolation,
      CLDR plural/select and formatting; add targeted language-pack identities,
      ordered key-level precedence, and deterministic duplicate validation.
- [ ] Route templates, Luau, manifests, props, generated settings, and debug data
      through one owner-aware resolver with visible misses; align `mesh.i18n`,
      `mesh.locale`, service events, and read/write capability enforcement.
- [ ] Drive locale CLI/LSP/doctor, extraction, missing-key and provenance tooling
      from the canonical graph/snapshot, and either implement `<i18n>` component
      blocks with explicit precedence or reject them instead of discarding them.

## Host resources and icon packs

- [ ] Replace discovery-time global resource registration with one immutable,
      graph/profile-authorized icon-and-font snapshot and atomic last-known-good
      lifecycle reconciliation. [Audit](../.planning/log/sections/06-host-resources-and-icon-packs/improvements.md).
- [ ] Make icon resolution deterministic and complete: canonical owner-scoped
      pack IDs/aliases, typed multicolor mappings, semantic/dash fallbacks,
      ordered chains, and required/optional results with provenance.
- [ ] Remove legacy icon config and discovery authorities, and drive bounded
      pack validation, effective-state diagnostics, CLI/LSP/doctor inspection,
      and resource coverage previews from the canonical snapshot.

## Component language

- [ ] Replace lossy `.mesh` block extraction and ad-hoc template preprocessing
      with a span-preserving parser that rejects unknown/duplicate blocks,
      malformed expressions, unsupported script languages, and discarded `<i18n>` data. [Audit](../.planning/log/sections/07-component-language/improvements.md).
- [ ] Make component compilation resolve imports per owner, enforce module-root
      path containment, detect alias collisions/cycles, and validate child prop
      names, visibility, values, and `prop()` references before runtime.
- [ ] Make the `<props>` contract one normalized typed value pipeline: reject
      invalid constraints/coercions and retain the highest valid precedence value
      when a later settings or instance override is invalid.

## UI element core

- [ ] Establish unique `NodeId` tree identity and reject duplicate/unknown
      element definitions before layout, events, and accessibility projection.
- [ ] Generate element contracts, runtime types, attributes, events, and
      pseudo-state matching from one canonical schema with complete coverage.
- [ ] Consolidate interaction, style invalidation, retained layout, and
      accessibility into a coherent frame transaction with pointer capture,
      failure-safe layout, and explicit dirty-node output.
- [ ] Normalize ARIA/visibility/name semantics after child construction and
      invalidate text measurement caches on font/measurer generation changes.

## Interaction and motion

- [ ] Unify visibility, transformed geometry, disabled/inert eligibility, and
      target filtering across interaction, rendering, focus, scrolling,
      tooltips, and accessibility. [Audit](../.planning/log/sections/09-interaction-and-motion/improvements.md).
- [ ] Define a canonical interaction transaction for focus, pointer capture,
      press origin, gesture/scroll ownership, and typed invalidation instead of
      splitting policy between `mesh-core-interaction` and the shell.
- [ ] Complete motion semantics: pause/resume, stable animation identity,
      cancellation/reversal, reduced-motion policy, discrete visibility timing,
      and per-keyframe easing propagation.
- [ ] Implement or safely gate the public `box-shadow` parser and add the
      Section 9 interaction/render/animation regression matrix.

## Frontend compiler and host

- [ ] Compile, validate, watch, invalidate, and reload primary and
      extension-point roots as one atomic frontend catalog revision, including
      contribution interface checks and contribution-only dependencies. [Audit](../.planning/log/sections/10-frontend-compiler-and-host/improvements.md).
- [ ] Gate service payload publication on capabilities and replace the
      revision-light host effects with a coherent, typed frontend frame/effect
      boundary that rejects stale catalog/runtime requests.
- [ ] Complete frontend runtime lifecycle and recovery: dispatch mount/unmount,
      make prop publication transactional, and preserve truthful typed,
      source-located diagnostics on failures.
- [ ] Unify template expression semantics and root-scope validation around the
      real Luau parser/runtime, then enforce imported public-prop and
      child-surface contracts.
- [ ] Split the renderer/Wayland/package/debug policy out of the
      compiler-facing frontend host ABI.

## Shell core and orchestration

- [ ] Replace split profile/runtime mutation with one revisioned activation
      coordinator: immutable candidate graph/interfaces/resources, full root and
      provider identities, ready hidden replacements, atomic commit, and
      post-commit retirement. [Audit](../.planning/log/sections/15-shell-core-and-orchestration/improvements.md).
- [ ] Reconcile every live graph delta, including backend module enable/disable;
      buffer provider readiness state and generation-tag all runtime messages,
      events, results, and restart deadlines.
- [ ] Add explicit frontend unmount and graceful backend stop/join, then route
      normal shutdown and every shell-loop error through one bounded lifecycle
      supervisor that owns workers, IPC, eventfd, storage, and surfaces.
- [ ] Replace the static startup watcher with a healthy, generation-aware watch
      set covering graph/profile/catalog/contribution/resource/import changes,
      with immediate bounded-poll fallback and last-known-good reloads.
- [ ] Centralize CoreRequest effects in one fair bounded scheduler and isolate
      component callback/tick/build failures into errored placeholders instead
      of allowing cycles or one module to terminate the shell.
- [ ] Make shell control-plane propagation coherent: publish provider
      unavailable/recovery transitions, settings revisions, theme/locale effects,
      and invalid graph/profile diagnostics through the committed generation.

## Developer and authoring tools

- [ ] Move CLI install/update/rollback/uninstall/profile mutations behind one
      journaled, path-contained package transaction with typed live-activation
      acknowledgements and exact-generation recovery. [Section 16 audit](../.planning/log/sections/16-developer-and-authoring-tools/improvements.md).
- [ ] Derive and refresh one canonical graph-authoring snapshot for CLI, doctor,
      and LSP; eliminate duplicated manifest/schema validation and stale or
      silently lossy module/service indexes. [Section 16 audit](../.planning/log/sections/16-developer-and-authoring-tools/improvements.md).
- [ ] Make LSP parsing and protocol boundaries correct and syntax-aware:
      UTF-16 positions, workspace folders, Unicode JSON, versioned updates,
      secure definitions, and recoverable Luau/component AST diagnostics.
      [Section 16 audit](../.planning/log/sections/16-developer-and-authoring-tools/improvements.md).

## Settings

The single sparse store shipped 2026-07-30: one `config/settings.json`
namespaced by `shell` / module id / interface id, replacing `shell-settings.json`,
`settings-default.json`, and the per-module `config/settings.json` files.

## Popovers

In-tree `<popover>` nodes are promoted to `xdg_popup` child surfaces, with core
owning the hover bridge, one-open-per-trigger exclusivity, and compositor
dismiss sync. _(detail: "Embeddable popovers via `<popover>` surface
promotion")_

## Performance

Full history, baselines, and the **rejected-experiments table** are in
[`.planning/log/performance-log.md`](../.planning/log/performance-log.md).
Check it before starting: several of the obvious approaches below have already
been measured and reverted.

Every optimization lands with a representative benchmark, and a checked relative
gate where the win is structural.

### Render pipeline

- [ ] **The narrow service path never engages for real modules.** It requires a
      template to interpolate the service field directly, but every shipped
      component reads services in Luau and binds derived variables — so
      `narrow_nodes` is empty, invalidation falls back to `TREE_REBUILD`, and
      every poll is a full rebuild plus 100%-of-surface damage. Measured on the
      navigation bar 2026-08-08: 240/240 frames full-surface, ~4 such frames per
      second at rest. Needs script-level service reads to feed
      `service_field_reads`, or a different narrowing signal.
- [ ] A root-level `backdrop-filter` collapses all partial damage to the whole
      surface (`expand_damage_for_blur_regions` unions with the full blurred
      region), and `.nav-shell` carries one. Latent today because damage is
      already full-surface for the reason above; it becomes the next ceiling as
      soon as that is fixed.
- [ ] Continue widening generation shortcuts to per-node dirty scoping and
      unify changed-node fingerprints across the retained, render, and display
      layers; geometry-only retained snapshots are split out now.
- [ ] Display-list segment/rope command storage → v1.21. Command arrays are
      still flattened per ancestor. Replay must consume segments directly
      instead of eagerly re-flattening them — an eager reconstruction was tried
      and reverted (see log).
- [ ] Close the Section 12 correctness gaps before further paint optimization:
      unify paint fingerprints and topology generations, implement complete
      transforms/effect compositing/border lowering, and revision text/icon
      caches. [Section 12 audit](../.planning/log/sections/12-rendering-and-paint/improvements.md)

### Style

- [ ] Typed style declarations end-to-end: resolve theme tokens to typed values
      once per theme load; `apply_declaration` consumes typed values, strings
      only for diagnostics (E). Static literals now pre-lower; typed property
      values and one-time token lowering remain. _(detail: "P2 — typing &
      interning")_
- [ ] Interaction frames still re-apply string style declarations per node —
      folds into typed declarations and narrower invalidation.
      _(detail: "P2 — architecture")_
- [ ] A tree-rebuild frame restyles memo-reused subtrees too. Component memo
      entries are stored pre-restyle (position-independent by design), so a
      reused page pays a full style walk and copy-on-write anyway — 2.8ms of an
      8.6ms Appearance service frame. Needs styled memo entries, or a
      "styles still valid" mark the restyle walk can skip.

### Typing and interning

- [ ] Interned `Symbol` / `TagId` types and a typed `WidgetNode`. Attributes,
      module ids, and element tags are done; widget-tree **tags**, attribute
      **values**, and the broader symbol types remain. Profiling now puts the
      dominant remaining build cost in style resolution, not further attribute
      work. _(detail: "P2 — typing & interning")_

### Composition

### Threading

- [ ] Parallelize paint across surfaces: phase-split `render_components` into a
      serial VM-bound phase and a parallel paint/SHM phase (rayon) (K).
- [ ] Pipeline paint of frame N against script work of frame N+1, after the
      per-surface split.
- [ ] Tile-parallel raster for large damage, above a measured threshold only.
- [ ] Move blocking file IO off the shell thread — i18n catalog mounts,
      settings and theme reloads, and icon/SVG cache-miss rasterization on the
      paint path — via `spawn_blocking` plus completion events.

### Runtime boundary

- [ ] Apply one authoritative sandbox/resource policy to every Luau realm:
      enforce instruction, memory, output, queue, storage, and child-process
      budgets, with timeout cleanup and quarantine. [Section 11 audit](../.planning/log/sections/11-luau-runtime-and-sandbox/improvements.md).
- [ ] Replace backend task aborts and early-return cleanup with an idempotent
      lifecycle supervisor that stages provider generations, flushes storage,
      reaps streams, and publishes one truthful terminal result. [Section 11 audit](../.planning/log/sections/11-luau-runtime-and-sandbox/improvements.md).
- [ ] Move `mesh.exec` and stream handling behind bounded cancellable workers
      with stable stream IDs, exit/reap events, output limits, and executable
      path/argument policy. [Section 11 audit](../.planning/log/sections/11-luau-runtime-and-sandbox/improvements.md).
- [ ] Make backend commands and events typed, correlated, generation-aware,
      transactional, and bounded; define explicit coalescing keys and terminal
      overflow/timeout results. [Section 11 audit](../.planning/log/sections/11-luau-runtime-and-sandbox/improvements.md).
- [ ] Move default runtime storage to secure durable XDG state with user-only
      permissions, quotas, recovery, and single-writer/revision semantics.
      [Section 11 audit](../.planning/log/sections/11-luau-runtime-and-sandbox/improvements.md).
- [ ] Make reload transactional and backend callback handles generation-stable:
      replace stale Lua environments, preserve one backend `self`, and prevent
      pre-ready or old-generation updates from reaching consumers. [Section 11 audit](../.planning/log/sections/11-luau-runtime-and-sandbox/improvements.md).

- [ ] Push-based backend host API primitives (D-Bus signal subscribe, fd/socket
      watch, stream adoption) so providers are event-driven and polling is the
      fallback (C). **Measured 2026-08-08:** the shipped polls fork ~3 processes
      per second (`hyprctl` 500ms, `wpctl` 1000ms, `brightnessctl` 2000ms) and
      cost 6.4% of a core continuously — 32x the whole shell render loop at rest
      (0.2%) — mostly in dynamic-linker startup, for no state change. Includes evaluating `pw-dump --monitor` as a real volume
      event source; `pw-mon` emits no `changed:` block for volume.
- [ ] Handler sync still reads compound table globals, because nested in-place
      mutations never assign through `_ENV`. Eliminating those reads needs
      recursively tracked tables or Rust-owned reactive values (R).
      _(detail: "P1 — boundary & dispatch")_
- [ ] Storage reads still clone per Lua access. Needs shared immutable JSON
      values or lock avoidance — two cache designs were measured and reverted
      (I; see log).

### Surface policy and configuration

- [ ] Validate manifest surface enums and role-specific fields before
      resolution; invalid declarations must produce diagnostics instead of
      silently falling back. [Section 13 audit](../.planning/log/sections/13-surface-policy-and-configuration/improvements.md).
- [ ] Protect the author-only `promotable` contract and route settings-driven
      role changes through the same transactional transition path as explicit
      promotion requests. [Section 13 audit](../.planning/log/sections/13-surface-policy-and-configuration/improvements.md).
- [ ] Unify role-field metadata, settings/ejection semantics, and manifest
      diagnostics so inert window/layer fields are handled consistently.
      [Section 13 audit](../.planning/log/sections/13-surface-policy-and-configuration/improvements.md).
- [ ] Replace split surface-config field lists with a revisioned semantic
      policy diff that includes blur, decorations, padding, geometry, keyboard
      mode, and role transitions. [Section 13 audit](../.planning/log/sections/13-surface-policy-and-configuration/improvements.md).
- [ ] Make unmeasured/content/padded/physical surface extents typed at the
      shell/presentation seam, with checked geometry and transactional reload
      regressions. [Section 13 audit](../.planning/log/sections/13-surface-policy-and-configuration/improvements.md).

### Rendering and paint

- [ ] Establish one canonical render-frame snapshot and transform/clip model;
      unify invalidation, display-list reuse, damage, blur regions, and hit
      testing around cumulative affine transforms. [Section 12 audit](../.planning/log/sections/12-rendering-and-paint/improvements.md).
- [ ] Complete paint semantics for opacity layers, four-edge borders,
      four-corner radii, text physical scaling, and stable equal-z ordering;
      add retained and pixel regressions. [Section 12 audit](../.planning/log/sections/12-rendering-and-paint/improvements.md).
- [ ] Replace path-only font/glyph/text caches and synchronous icon/font decode
      with generation-aware bounded resources and an asynchronous paint-safe
      resource broker. [Section 12 audit](../.planning/log/sections/12-rendering-and-paint/improvements.md).
- [ ] Make partial-present capability, layer balance, diagnostics, and
      backend fidelity explicit contracts; derive compositor blur and uploaded
      damage from the validated frame spans. [Section 12 audit](../.planning/log/sections/12-rendering-and-paint/improvements.md).

### Presentation

- [ ] Make presentation lifecycle transactional and observable: typed
      create/configure/present/lost results, last-known-good role replacement,
      and one idempotent close/dismiss/destroy teardown path. [Section 14 audit](../.planning/log/sections/14-wayland-platform-and-presentation/improvements.md).
- [ ] Replace presentation fingerprints and warm region caches with typed diffs
      and object/configure/frame generations so state-only changes and recreated
      objects commit compositor state. [Section 14 audit](../.planning/log/sections/14-wayland-platform-and-presentation/improvements.md).
- [ ] Make popup promotion capability- and identity-safe: carry click seat/serial,
      gate reposition by xdg-shell version, validate role/parent/reparenting, and
      correlate reactive configures. [Section 14 audit](../.planning/log/sections/14-wayland-platform-and-presentation/improvements.md).
- [ ] Unify resolved logical/physical surface extents and make SHM buffer-release
      backpressure, callback generations, output membership, and input ownership
      explicit without hot retries or stale routing. [Section 14 audit](../.planning/log/sections/14-wayland-platform-and-presentation/improvements.md).
- [ ] Replace the recording-only presentation test backend with a deterministic
      lifecycle simulator and a small live compositor conformance matrix covering
      close, popup, scaling, occlusion, and multi-output behavior. [Section 14 audit](../.planning/log/sections/14-wayland-platform-and-presentation/improvements.md).
- [ ] Direct Skia paint into the mapped SHM canvas for full-present frames,
      keeping `PixelBuffer` as the retained compare copy (H). Design:
      [`.planning/todos/pending/2026-08-02-direct-shm-paint.md`](../.planning/todos/pending/2026-08-02-direct-shm-paint.md).
- [ ] Rotation transforms allocate a temp `PixelBuffer` and repaint the subtree
      per frame. Low priority until rotation ships; scratch-buffer reuse was
      measured and rejected (see log).

### Startup and catalog

- [ ] Narrow frontend catalog index rebuilds to graph deltas. Compiled sources
      now survive live graph changes by manifest/source fingerprint, but slot
      and validation indexes still rebuild across the catalog.

### Architecture

- [ ] GPU rendering backend after retained layout, smart invalidation, and
      damage tracking ship → v1.25. Plan:
      [`gpu-rendering-backend`](../.planning/todos/pending/2026-07-15-gpu-rendering-backend.md).
      Skia-GL (Ganesh) first — same Canvas API as the shipped raster backend,
      and EGL buffer-age partial present preserves the damage pipeline.

---

## Attack order

Updated 2026-07-30.

1. **Structural-sharing memo hits**, narrow invalidation, and affected-subtree
   re-evaluation.
2. **Runtime style-diagnostic invalidation** and typed declarations.
3. **Incremental shared frontend catalog**, single retained renderer, and the
   per-surface prepare/paint/present split with batched Wayland commits.
4. **Direct SHM paint** and fractional-scale partial damage, re-tested with
   upload instrumentation (D).
