<!-- generated-by: gsd-doc-writer -->
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
config/module.json
        │
        ▼
module discovery and installed graph
        │
        ├──► interface registry ──► selected backend providers
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

## Platform boundary

The core owns mechanisms that must remain consistent and enforceable:

- module loading, validation, graph resolution, and lifecycle;
- component and service execution;
- typed state, method, and event transport;
- capabilities, sandbox policy, and failure isolation;
- layout, rendering, input, accessibility, and Wayland presentation;
- generic persistence primitives and structured diagnostics.

Modules own policy and finished experiences. Settings UI, developer tools,
package tooling, panels, launchers, themes, and system integrations should be
replaceable modules with no hidden privilege.

## Key abstractions

| Abstraction | Ownership | Purpose |
| --- | --- | --- |
| `ModuleManifest` | `mesh-core-module` | Canonical `module.json` representation |
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
not compute audio-, network-, power-, or settings-specific policy.

Provider selection is explicit when several compatible providers are enabled.
When exactly one compatible provider exists, the current graph may select it
automatically. Missing optional services degrade locally; missing required
services are graph diagnostics.

## Target shell profiles

The accepted next composition model replaces a single root layout decision with
named shell profiles. A profile will select root component instances, surface
placement, ambiguous service providers, resources, root background services,
and profile-scoped configuration. Required services remain inferred from
component contracts.

Live switching must be transactional: validate and prepare a candidate graph,
preserve identical service instances, initialize new surfaces, commit the
visible switch, and only then remove orphaned runtime objects. Durable
service-owned data remains shared while configuration is profile-scoped.

This section is target architecture. The current code still reads the
repository graph from `config/module.json` and does not implement live profile
switching.

## Directory structure

```text
crates/
  core/
    foundation/     cross-cutting contracts and data
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
.planning/           historical plans, evidence, and design records
```

Detailed crate dependency rules are documented in
[crate boundaries](../crate-boundaries.md).
