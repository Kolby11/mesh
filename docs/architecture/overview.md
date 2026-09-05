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
        ├──► retained render data and software paint
        ├──► input, focus, gestures, and accessibility
        └──► diagnostics and profiling
        │
        ▼
presentation layer ──► Wayland surfaces
```

The CLI creates `mesh_core_shell::Shell`. The shell discovers modules, loads
the installed graph, registers interface contracts and providers, compiles
frontend roots, executes service/component Luau, renders surfaces, and handles
Wayland events.

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
