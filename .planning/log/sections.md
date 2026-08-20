# MESH Package and Concern Map

# prompt

i want you to take a look at the sections.md file, then i want you to go throgh the 3. section spawning luna xhigh agent to which you will add instruction to scan the whole logical process of that section so it
creates a picture of the instruction tree or something like that then you will spawn next two agents, one
will check for any logical errors or improvements in the code order, the second agent will look directly at
possible code errors. If you find any place where some other agents could be use to create a concrete list
of improvement for that section spawn them. After the work is finished make sure you output summary of the
findings into new improvements.md file under that section. Also dont constraint the logic agent on the
current code flow, if they see a possible better feature make sure that they suggest it

**Audited:** 2026-08-16

This is the current package-oriented split of the MESH codebase. Each section
names the workspace packages that can be reasoned about or tested together,
what they own, and the seam that keeps the concern replaceable.

This is not a second backlog. Open work lives in
[`docs/BACKLOG.md`](../../docs/BACKLOG.md); the present lives in
[`STATUS.md`](../STATUS.md); measurements and historical decisions live in the
dated log and [`performance-log.md`](performance-log.md).

## Package split at a glance

| Concern | Workspace package(s) | Primary isolation seam |
| --- | --- | --- |
| Core foundation contracts | `mesh-core-capability`, `mesh-core-config`, `mesh-core-diagnostics`, `mesh-core-events`, `mesh-core-debug` | serializable contracts, diagnostics, events, and policy types |
| Module system and installation | `mesh-core-module` | canonical manifests, graph, lifecycle, package state |
| Service contracts | `mesh-core-service` | interfaces, providers, typed state/method/event records |
| Themes | `mesh-core-theme` | token/default/keyframe/theme-engine API |
| Localization / i18n | `mesh-core-locale` | locale catalogs and translation lookup |
| Host resources and icon packs | `mesh-core-resources`, `mesh-core-icon` | system catalogs, semantic icon resolution, pack bindings |
| Component language | `mesh-core-component` | `.mesh` parsing, props, scripts, styles, templates |
| UI element core | `mesh-core-elements` | retained widget model, style, layout, events, accessibility primitives |
| Interaction and motion | `mesh-core-interaction`, `mesh-core-animation` | hit testing, focus, scroll, transitions, keyframes |
| Frontend compiler and host | `mesh-core-frontend`, `mesh-core-frontend-host` | compiled frontend modules and shell-facing host contracts |
| Luau runtime and sandbox | `mesh-core-runtime`, `mesh-core-scripting`, `mesh-core-backend` | sandbox policy, Luau contexts, backend loops and commands |
| Rendering and paint | `mesh-core-render` | retained display list, render objects, painter and adapters |
| Surface policy | `mesh-core-surface-config` | placement, sizing, surface settings, prop validation |
| Wayland and presentation | `mesh-core-wayland`, `mesh-core-presentation` | compositor protocol, SHM, damage, input, popup/surface commits |
| Shell core/orchestration | `mesh-core-shell` | discovery, profiles, lifecycle, scheduling, runtime composition |
| Developer and authoring tools | `mesh-tools-cli`, `mesh-tools-lsp` | CLI operations and `.mesh`/manifest language tooling |

The split follows package ownership rather than product features. A panel,
launcher, or settings page crosses several sections; it does not create a new
core package. Module-specific policy stays in modules and Luau scripts while
these packages provide reusable mechanisms and contracts.

## 1. Core foundation contracts

**Packages:** `mesh-core-capability`, `mesh-core-config`,
`mesh-core-diagnostics`, `mesh-core-events`, and `mesh-core-debug`.

**Owns:** capability values and privilege checks; settings/config models and
validation; health/lifecycle diagnostics; the typed event bus; debug, profiling,
allocation, invalidation, and inspection snapshots.

**Isolation seam:** These packages should expose data and policy contracts, not
shell composition or service-specific behavior. They are the lowest-level
cross-cutting core and should remain usable without rendering or Wayland.

**Source roots:**
`crates/core/foundation/{capability,config,diagnostics,events,debug}/`.

**Review:** [`improvements.md`](sections/01-core-foundation-contracts/improvements.md)
records the 2026-08-16 logical-flow, correctness, and security audit for this
section. Actionable follow-ups remain tracked only in `docs/BACKLOG.md`.

## 2. Module system and installation

**Package:** `mesh-core-module`.

**Owns:** canonical `module.json` manifests, module kinds and capabilities,
installed graphs, dependency/provider resolution, module lifecycle, source
loading, package provenance, locks, health, graph diagnostics, and contribution
records.

**Isolation seam:** The graph and manifest model are the source of truth for
what is installed and enabled. The module package must not own shell widgets,
service policy, or compositor behavior; it hands resolved records to their
consumers.

**Source roots:**
`crates/core/extension/module/{manifest,package,lifecycle}.rs` and
`crates/core/extension/module/src/package/installed_graph/`.

**Review:** [`improvements.md`](sections/02-module-system-and-installation/improvements.md)
records the 2026-08-17 process, correctness, transaction, resolver, and
filesystem-security audit for this section. Actionable follow-ups remain tracked
only in `docs/BACKLOG.md`.

## 3. Service contracts

**Package:** `mesh-core-service`.

**Owns:** versioned interfaces, typed state, method and event contracts,
provider compatibility, resolution records, and the transitional typed service
registry.

**Isolation seam:** Frontends consume interface records and backends implement
them. This package must not know how a provider polls Linux, paints a widget,
or presents a surface.

**Source root:** `crates/core/extension/service/`.

**Review:** [`improvements.md`](sections/03-service-contracts/improvements.md)
records the 2026-08-17 service-flow, contract-semantics, correctness, capability,
and provider-lifecycle audit for this section. Actionable follow-ups remain
tracked only in `docs/BACKLOG.md`.

## 4. Themes

**Package:** `mesh-core-theme`.

**Owns:** theme manifests and layers, token values, component defaults, modes,
keyframes, inheritance/cascade, revisions, and theme diagnostics.

**Isolation seam:** Theme consumers ask for resolved semantic tokens and
component defaults. Theme loading must not depend on widget painting, Wayland,
or a particular settings frontend.

**Source root:** `crates/core/foundation/theme/`.

**Review:** [`improvements.md`](sections/04-themes/improvements.md) records the
2026-08-19 theme-flow, correctness, cascade, parser, transaction, and
filesystem-security audit for this section. Actionable follow-ups remain
tracked only in `docs/BACKLOG.md`.

## 5. Localization / i18n

**Package:** `mesh-core-locale`.

**Owns:** translation catalogs, locale selection, fallback lookup, formatting,
plural/variant handling, and locale-aware module data.

**Isolation seam:** Components and scripts use translation keys and locale
records; catalog storage and fallback policy remain independent of rendering
and compositor code.

**Source root:** `crates/core/foundation/locale/`.

**Review:** [`improvements.md`](sections/05-localization-i18n/improvements.md)
records the 2026-08-19 locale-flow, correctness, catalog, fallback, capability,
and tooling audit for this section. Actionable follow-ups remain tracked only
in `docs/BACKLOG.md`.

## 6. Host resources and icon packs

**Packages:** `mesh-core-resources` and `mesh-core-icon`.

**Owns:** host XDG icon-theme/font-family discovery, semantic icon names,
icon-pack module bindings, pack chains, fallback targets, font assets, and
variable icon axes.

**Isolation seam:** Authors use semantic icon/font roles. Resource discovery
and asset resolution produce targets consumed by the renderer; they do not
own paint commands or shell layout.

**Source roots:**
`crates/core/foundation/resources/` and `crates/core/ui/icon/`.

**Review:** [`improvements.md`](sections/06-host-resources-and-icon-packs/improvements.md)
records the 2026-08-19 resource-discovery, resolution, lifecycle, safety,
caching, font-pack, and tooling audit for this section. Actionable follow-ups
remain tracked only in `docs/BACKLOG.md`.

## 7. Component language

**Package:** `mesh-core-component`.

**Owns:** `.mesh` single-file parsing, `<props>`, template/script/style blocks,
component imports, prop types and values, localized labels, and component-level
syntax diagnostics.

**Isolation seam:** This package produces source/AST/component contracts. It
does not evaluate Luau, resolve installed modules, compute layout, or paint.

**Source roots:**
`crates/core/ui/component/{parser,template,style}.rs`.

**Review:** [`improvements.md`](sections/07-component-language/improvements.md)
records the 2026-08-19 component-flow, parser, import-boundary, props, and
compiler-contract audit for this section. Actionable follow-ups remain tracked
only in `docs/BACKLOG.md`.

## 8. UI element core

**Package:** `mesh-core-elements`.

**Owns:** `WidgetNode`, `NodeId`, element contracts, attributes, event records,
component composition values, popover metadata, style resolution, layout/Taffy
state, retained geometry, text measurement contracts, and accessibility
metadata.

**Isolation seam:** This is the shared UI model package. It should remain
renderer- and compositor-neutral: it can describe a tree, computed style,
layout, and semantics without importing Skia or Wayland.

**Internal areas:**

- `crates/core/ui/elements/src/tree.rs`, `element/`, `attributes.rs`,
  `composition.rs`, and `events.rs` — element/tree contracts;
- `crates/core/ui/elements/src/style/` — CSS-like values, matching, state, and
  computed style;
- `crates/core/ui/elements/src/layout/` — Taffy lowering, retained layout,
  and text measurement;
- `crates/core/ui/elements/src/accessibility.rs` and `popover.rs` — semantic
  and surface-promotion metadata.

This package is intentionally broad today. Style, layout, and accessibility
are logical sub-concerns, but extracting them into more crates would only be
useful once their shared `WidgetNode` contracts stop changing.

**Review:** [`improvements.md`](sections/08-ui-element-core/improvements.md)
records the 2026-08-19 end-to-end process, contract, state, layout, event,
style, popover, and accessibility audit for this section. Actionable
follow-ups remain tracked only in `docs/BACKLOG.md`.

## 9. Interaction and motion

**Packages:** `mesh-core-interaction` and `mesh-core-animation`.

**Owns:** hit testing, focus traversal, keyboard/pointer/scroll behavior,
tooltip ownership, interaction state queries, easing, interpolation,
transitions, transforms, shadows, and keyframes.

**Isolation seam:** Interaction consumes the element tree and emits target or
state decisions. Animation consumes style/tree identities and emits visual
values. Neither package should own Luau execution or Wayland event polling.

**Source roots:**
`crates/core/ui/interaction/` and `crates/core/ui/animation/`.

**Review:** [`improvements.md`](sections/09-interaction-and-motion/improvements.md)
records the 2026-08-19 end-to-end process, geometry, target eligibility,
focus, animation, motion-policy, and interaction/render-boundary audit for this
section. Actionable follow-ups remain tracked only in `docs/BACKLOG.md`.

## 10. Frontend compiler and host

**Packages:** `mesh-core-frontend` and `mesh-core-frontend-host`.

**Owns:** compiled frontend module records, expression/template evaluation,
component composition, style-context assembly, widget-tree construction,
frontend catalog-facing contracts, surface component host types, service
observation summaries, child-surface requests, input/core requests, and the
shell component trait.

**Isolation seam:** The compiler converts component source into element data;
the host package defines the contract needed to run that data in a shell. The
compiler must not know the live shell event loop, and the host contract must
not implement module-specific UI.

**Source roots:**
`crates/core/frontend/compiler/{compile,expr,style,tags}.rs` plus
`crates/core/frontend/compiler/render/` and
`crates/core/frontend/host/src/lib.rs`.

**Review:** [`improvements.md`](sections/10-frontend-compiler-and-host/improvements.md)
records the 2026-08-20 end-to-end compiler, composition, catalog, runtime,
host-boundary, lifecycle, capability, diagnostics, and revision audit for this
section. Actionable follow-ups remain tracked only in `docs/BACKLOG.md`.

## 11. Luau runtime and sandbox

**Packages:** `mesh-core-runtime`, `mesh-core-scripting`, and
`mesh-core-backend`.

**Owns:** sandbox tiers and capability policy; per-thread Luau realms and
per-context `_ENV` isolation; frontend/backend script state; host APIs,
storage, streams, backend polling, command coalescing, provider lifecycle,
and backend event publication.

**Isolation seam:** Generic host APIs cross into Luau. Service-specific logic
stays in module scripts. The runtime must not import the painter, glyph caches,
surface buffers, or compositor protocol.

**Source roots:**
`crates/core/runtime/{sandbox,scripting,backend}/`.

**Review:** [`improvements.md`](sections/11-luau-runtime-and-sandbox/improvements.md)
records the 2026-08-20 end-to-end runtime, isolation, capability, lifecycle,
stream, command, storage, and security audit for this section. Actionable
follow-ups remain tracked only in `docs/BACKLOG.md`.

## 12. Rendering and paint

**Package:** `mesh-core-render`.

**Owns:** retained render objects, display-list commands and subtree spans,
damage selection, software/Skia paint backends, text and glyph rasterization,
icon painting, effects, render proofs, debug paint, and optional renderer
adapters.

**Isolation seam:** The renderer consumes element/style/layout data and emits
paint commands/pixels. Skia and other paint backends stay below the display-list
boundary; Wayland buffer ownership stays in presentation.

**Source roots:**
`crates/core/frontend/render/{display_list,render_object.rs,surface,proof.rs}`.

**Review:** [`improvements.md`](sections/12-rendering-and-paint/improvements.md)
records the 2026-08-20 process-tree, logic/order, direct code-error, and
render-boundary audit for this section. Actionable follow-ups remain tracked
only in `docs/BACKLOG.md`.

## 13. Surface policy and configuration

**Package:** `mesh-core-surface-config`.

**Owns:** surface placement and sizing policy, manifest/settings surface
validation, component prop override validation, content/padding extent rules,
and compositor-facing surface configuration decisions.

**Isolation seam:** This package resolves policy into surface configuration
records. It does not create Wayland objects, run the shell loop, or paint
content.

**Source root:** `crates/core/surface-config/`.

**Review:** [`improvements.md`](sections/13-surface-policy-and-configuration/improvements.md)
records the 2026-08-20 process-tree, logic/order, direct code-error, and
schema/lifecycle audit for this section. Actionable follow-ups remain tracked
only in `docs/BACKLOG.md`.

## 14. Wayland platform and presentation

**Packages:** `mesh-core-wayland` and `mesh-core-presentation`.

**Owns:** compositor-neutral surface roles and capability traits; Wayland
client protocol plumbing; layer-shell and popup surfaces; SHM allocation and
reuse; damage conversion; input regions; configure/frame callbacks; and
presentation commits.

**Isolation seam:** `mesh-core-wayland` contains small platform contracts and
stubs. `mesh-core-presentation` owns the concrete Wayland backend and consumes
render output; neither package owns module graphs, Luau policy, or component
authoring.

**Source roots:**
`crates/core/platform/wayland/` and
`crates/core/presentation/src/wayland_surface/`.

**Review:** [`improvements.md`](sections/14-wayland-platform-and-presentation/improvements.md)
records the 2026-08-20 process-tree, logic/order, direct code-error, protocol,
lifecycle, SHM, input, and test-fidelity audit for this section. Actionable
follow-ups remain tracked only in `docs/BACKLOG.md`.

## 15. Shell core and orchestration

**Package:** `mesh-core-shell`.

**Owns:** module discovery, profile activation, frontend catalog lifetime,
service/provider runtime coordination, component runtime instances, reloads,
scheduling, request/event routing, settings/theme/locale integration, render
orchestration, and the running shell lifecycle.

**Isolation seam:** The shell is the integration package, not the home for
service-specific policy or reusable low-level mechanisms. It wires the other
packages together and owns decisions that require the complete live runtime.

**Source roots:** `crates/core/shell/src/shell/`, including
`component/`, `runtime/`, `discovery.rs`, `profile.rs`, `package.rs`, and
`types.rs`.

**Review:** [`improvements.md`](sections/15-shell-core-and-orchestration/improvements.md)
records the 2026-08-20 process-tree, logic/order, direct code-error,
transaction, lifecycle, scheduler, watcher, and recovery audit for this
section. Actionable follow-ups remain tracked only in `docs/BACKLOG.md`.

## 16. Developer and authoring tools

**Packages:** `mesh-tools-cli` and `mesh-tools-lsp`.

**Owns:** `mesh-shell` commands, installation/doctor/profile/package
operations, manifest/schema validation, `.mesh` diagnostics, formatting,
completion/hover/definition support, semantic tokens, and author-facing
knowledge/indexes.

**Isolation seam:** Tools call the public module/config/compiler contracts.
They must not duplicate graph resolution, theme lookup, service routing, or
shell lifecycle policy.

**Source roots:** `crates/tools/cli/` and `crates/tools/lsp/`.

## Dependency direction

The intended package flow is:

```text
foundation/core contracts
        │
        ├── module system ──► frontend compiler/host ──► shell
        ├── service contracts ──► scripting/backend ──► shell
        ├── theme + locale + resources ──► compiler/elements/render
        └── component language ──► elements ──► interaction/layout/render
                                                   │
                           surface policy ◄────────┘
                                                   │
                         Wayland platform ◄─ presentation
```

Concrete rules:

- `mesh-core-elements`, `mesh-core-component`, and the foundation packages
  remain neutral about Skia, Wayland, and shell orchestration.
- `mesh-core-render` may consume UI data and icon/resource targets, but does
  not resolve modules or run scripts.
- `mesh-core-scripting` and `mesh-core-backend` may consume generic service,
  capability, locale, config, and event contracts, but do not paint or present.
- `mesh-core-presentation` may consume render output and Wayland contracts, but
  does not own surface policy, module activation, or component state.
- `mesh-core-shell` is allowed to depend on the boundary packages as the
  orchestration glue; lower-level packages must not depend back on it.
- CLI and LSP depend on public contracts and do not become alternate owners of
  module graph, settings, or runtime behavior.

Open work and performance priorities should be attached to one of these
sections in [`docs/BACKLOG.md`](../../docs/BACKLOG.md), while measurements and
rejected approaches stay in [`performance-log.md`](performance-log.md).
