# Architecture

## System overview

MESH is a modular Wayland shell runtime. A root module graph selects frontend
and backend modules; frontend `.mesh` files compile into component trees;
sandboxed Luau code implements component and service behavior; typed interfaces
connect consumers to providers; and the shell maps retained UI state to Wayland
surfaces. The architecture follows a microkernel-like policy boundary without
turning MESH into a process or privilege supervisor.

## Current runtime flow

```text
config/module.json + optional active profile
        │
        ▼
module discovery and installed graph
        │
        ├──► resolved service catalog ──► selected backend providers
        │                                  │
        ▼                                  ▼
frontend entrypoints ──► .mesh compiler ──► Luau service proxies
        │
        ▼
component/runtime tree
        │
        ├──► style and Taffy layout
        ├──► retained render data and Skia paint (GPU or CPU)
        ├──► input, focus, gestures, and accessibility
        └──► diagnostics and profiling
        │
        ▼
presentation layer ──► Wayland surfaces (dmabuf or SHM buffers)
```

The CLI creates `mesh_core_shell::Shell`. The shell discovers modules, loads
the installed graph, registers interface contracts and providers, compiles
frontend roots, executes service/component Luau, renders surfaces, and handles
Wayland events.

Shell tree builds defer layout until runtime annotation and the final cascade
are applied. Overflow then uses that geometry. Component memo entries preserve
pre-animation styles and semantic projections while retaining original authored
inputs; previews continue to produce a complete laid-out tree.

GPU frames compose a complete immutable Skia scene on the UI thread with pinned
raster resources and explicit frame paint settings. Each surface reuses bounded
node recordings by retained payload identity, scale, clip and module; effects and
scroll scopes remain live. Resource completions, asset-page publication and font/resource changes
invalidate recordings, while submitted scenes retain their original resources. A bounded worker owns EGL,
retained targets and fenced dmabuf exports; Wayland receives only descriptors
and release leases. Presentation submits the scene and commits it when the worker
publishes, so scripts and layout overlap GPU raster; the surface is gated until then, while
partial damage, compositor backpressure and SHM fallback remain authoritative.
CPU frames with several painting surfaces record their paint serially and
rasterize the recordings in parallel before presenting.
See [renderer ownership](../renderer-ownership.md).

Interaction restyles and animation ticks preserve the surface's retained pixels;
the display-list diff selects damage. Paint-only hover rules reuse layout, while
layout declarations and custom variables conservatively request geometry work.
Geometry-preserving animation-only frames sample registered nodes through the
immutable frame snapshot's child paths. Their coalesced change classes select
retained updates without a clean-tree geometry scan. Interaction frames over
reused geometry project only the nodes whose annotation inputs changed and
restyle their state-changed nodes through the same paths; with no selector
combinators, only inherited values reach other nodes. The exact restyled set
drives semantics and retained updates. Fixed-shape keyed native lists validate ordered keys and refresh only changed
items' captured binding locals and handlers. A list read directly from a script member
takes its changed items from the state sync, which records which array items differ
when it stores the member, so only those items are compared and key-checked. Their property writes feed the same frame dirty
set and restyle targets, including writes made by hover handlers. Inherited-value
changes alone propagate beyond those targets. Binding writes stage only their target nodes, then commit parents before children
after all expressions succeed; failure leaves the live tree untouched without
sharing its whole child vectors for rollback. Style diagnostics reuse that exact
set across intervening animation frames. Structural list changes retain the rebuild
path. Layout-class patches lay
out only the patched branch from the restyled set and report the nodes whose
geometry changed. Write-back rounds the nodes it projects exactly as Taffy
would, so Taffy's own whole-tree rounding pass is disabled. Text-only leaf patches first remeasure the bounded set of
intrinsic width constraints used by retained layout. When every result and the
layout style are unchanged, they retain Taffy geometry and report no geometry
changes. Changed measurements, incomplete probe history, resizing and text
resource revisions use layout. Full layout passes report geometry changes and
update their projection during the same bottom-up write, including fit extents
and visibility. Stable frames synchronize producer-reported nodes and subtrees
through snapshot paths, including scroll metadata and sampled style writes;
retained fingerprint discovery is test-only. Rebuilds and mismatched topology
reconcile the full retained tree before publishing the frame.
Retained restyles sample the exact restyled and registered animation nodes;
previous transition values come from immutable frame records. Child lifecycle
markers retain and restore their base classes, including when bindings write
before paint. Child enter/exit transitions leave covered binding writes on the
patch path, so timer ticks do not rebuild the owning surface. A shape-stable
service update that misses binding coverage may still reuse clean authored
branches during child lifecycle. Reuse strips prior lifecycle markers before
rebuild finalization captures the authored classes; retained synchronization
remains conservative on these fallback frames. Display ancestors use retained paths, walking a shared path
prefix once. Stable text, semantic and local geometry changes replace
ordinary node commands through a bounded persistent command tree, without
rebuilding sibling lists, and re-collect only their own display entries.
Equal visual bounds preserve ancestor coverage and effect metadata. Moved
bounds update ancestor coverage and spans in place when every ancestor owns one
command; a shrink that gives up its parent's coverage edge re-unions that
parent's children. Other command shapes, layers above a moved node, and boxes
that define a descendant's clip or transform use subtree rebuilds.
Text-only binding patches also retain focus scopes, disclosures and interaction
targets when all dirty paths preserve styles, state, geometry and non-text
attributes. Other input changes keep the ordinary reconciliation path.
Promoted tooltip animation ticks update only their popup paint generation;
child-only render work reconciles and paints child targets without painting the
parent. Inline tooltip animation still invalidates the parent surface.
Shared-tree popover animation still advances style and layout; when its parent
damage is empty, painting opens no canvas session and retains the parent's
complete GPU scene. Only damaged targets record and draw pixels.
Child-surface requests
retain dependencies on trigger geometry, ancestor policy and descendant paint
extents, and refresh from the finalized dirty set. Identity/element validation
runs on structural changes; semantic reindexing reuses validated structure.
Asynchronous icon, glyph and image jobs track their requesting component instances,
including promoted child targets, and repaint only those consumers on completion.
A file icon or image whose pixels are not ready paints nothing until its job
publishes; it is rasterized on the frame only when no worker can run it. GPU
scenes draw box shadows and rounded gradients natively; the cached CPU images
behind them serve CPU raster only.
An icon lookup whose job also finds nothing is remembered for the resource
revision and repaints nothing.
Showing a hidden surface restyles its retained tree rather than rebuilding it,
and a frame requested with nothing dirty reuses the retained tree, drawing
only when there is pixel damage.
A re-shown layer surface whose content is already measured sends its configure
at once and paints a single frame after the compositor answers; top-level
surfaces open straight into their resting styles, while promoted popups play
their entering frame. Once startup is idle, each never-shown hidden layer
surface is painted once into an unpresented canvas so its first show takes the
same path and its resources are already prepared.
The event loop includes input-repeat, temporary focus-grab and pending-frame
escape deadlines alongside component ticks and reload maintenance.

Hot reload uses a managed filesystem watcher that blocks on inotify and a
control eventfd; watch replacement and shutdown wake that wait directly. With
a healthy watcher, theme, settings, frontend metadata, and watch-set maintenance
share a two-second recovery deadline. Filesystem events request immediate
checks, while an unavailable watcher retains the shorter metadata-poll fallback.

During the profile migration, absence of `config/active-profile` preserves the
legacy root-graph decisions. When it exists, `config/profiles/<id>.json` owns
the desired composition: explicit roots/background services/providers/resources
seed an activation closure, and declared dependencies plus sole providers are
inferred before contributions are indexed. Installed source remains available
even when it is not part of that closure.

## Platform boundary

The [platform philosophy](../spec/00-philosophy.md) owns the boundary rules.
The core owns mechanisms that must remain consistent and enforceable:

- module loading, validation, graph resolution, and lifecycle;
- component and service execution;
- typed state, method, and event transport;
- capabilities, sandbox policy, and failure isolation;
- layout, rendering, input, accessibility, and Wayland presentation;
- scoped persistence, authoritative settings/package/profile transactions,
  runtime inspection, and structured diagnostics.

Modules own domain behavior and finished experiences. Settings UI, developer
tools, and package UI are ordinary `.mesh` components consuming built-in core
services. Panels, launchers, themes, and system integrations are replaceable
modules with no hidden privilege. Core owns the management mechanisms behind
those interfaces, including validation, precedence, and commit semantics.

Module execution runs in the shell process. Current scripting contexts on the
same thread share a sandboxed Luau realm with separate environments; public
references and typed interfaces provide explicit communication. Environment
privacy and capability checks do not imply a VM or a process per module.
Script errors require local diagnostics and bounded failure handling; native
process crashes are outside that isolation guarantee.

## Key abstractions

| Abstraction | Ownership | Purpose |
| --- | --- | --- |
| `ModuleManifest` | `mesh-core-module` | Canonical `module.json` representation |
| `SystemResourceCatalog` | `mesh-core-resources` | Cached host XDG icon themes and font families |
| `InstalledModuleGraph` | `mesh-core-module` | Resolved modules, interfaces, providers, and diagnostics |
| `InterfaceContract` | `mesh-core-service` | Typed service state, methods, events, and shared types |
| `ScriptContext` | `mesh-core-scripting` | Isolated frontend Luau execution context |
| `BackendRuntime` | `mesh-core-scripting` | Luau service-provider execution and host APIs |
| `WidgetNode` | `mesh-core-elements` | Retained UI node with style, layout, and semantics |
| `FrontendSurfaceComponent` | `mesh-core-shell` | Component instance attached to a shell surface |
| `Shell` | `mesh-core-shell` | Development-runtime integration point |

## Service architecture

Backend native event sources share a bounded, generation-owned stream lifecycle.
D-Bus, PipeWire, route netlink, inotify, Unix sockets and wlr foreign-toplevel
subscriptions wake backend coroutines; stream hooks can suspend without blocking
commands. Providers retain domain interpretation and fallback policy. Native
PipeWire loops own their protocol objects on an executor thread and wake on
cancellation; the other subscriptions use Tokio readiness. See
[backend host events](../modules/backend-events.md).

A component depends on an interface name and compatible version rather than a
provider module ID. Providers own their domain state and implement declared
methods and events. The core validates and transports those records; it should
not compute audio-, network-, or power-domain behavior or frontend display
state. Providers normalize host-specific data into stable domain contracts.
Built-in platform services are explicit core authorities, including settings
resolution and configuration transactions.

Provider selection is explicit when several compatible providers are enabled.
When exactly one compatible provider exists, the current graph may select it
automatically. Missing optional services degrade locally; missing required
services are graph diagnostics.

## Shell profiles

Named shell profiles replace a single root layout decision. **Shipped:**
profile documents, scoped preferences, multiple root instances, the activation
closure, transactional live switching, and composition instantiation. A
profile is an instance of a **composition module** (`mesh.kind: "composition"`,
e.g. the shipped `@mesh/desk`) plus the user's deltas: root component
instances, surface placement, ambiguous service providers, resources, root
background services, and profile-scoped configuration. Required services
remain inferred from component contracts. A composition binds — it selects
providers and resources — but never owns them and holds no privilege of its
own; it can only select among what its member modules already declare.

Live switching is transactional: validate and prepare a candidate graph,
preserve identical service instances, initialize new surfaces, commit the
visible switch, and only then remove orphaned runtime objects. Durable
service-owned data remains shared while configuration is profile-scoped.

The root `config/module.json` remains the installed-module inventory and
legacy fallback for the absence of `config/active-profile`. Profiles are the
active composition boundary once `active-profile` exists. Core-provided
management operations are described in
[01 §5.4](../spec/01-module-system.md#54-core-provided-interfaces); further
service and tooling coverage follows the detailed specs rather than a blanket
target label for all management APIs.

## Directory structure

```text
crates/
  core/
    foundation/     cross-cutting contracts, data, and host resource discovery
    extension/      modules and service interfaces
    ui/             components, elements, interaction, animation
    frontend/       compilation, hosting, rendering
    runtime/        sandboxed Luau and backend execution
    platform/       Wayland integration
    shell/          running-shell composition
  tools/            CLI and LSP
modules/             shipped editable module sources
config/              current development graph and settings
docs/spec/           public shipped/target contract
docs/                current author and maintainer guidance
docs/BACKLOG.md      the single list of open work
.planning/STATUS.md  what is in flight now
.planning/log/       dated, append-only history and measurements
```

Detailed crate dependency rules are documented in
[crate boundaries](../crate-boundaries.md).
