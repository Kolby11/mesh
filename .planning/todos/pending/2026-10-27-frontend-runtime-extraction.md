# Frontend runtime extraction (SH-08)

Move the component host from `mesh-core-shell` into a testable
`mesh-core-frontend-runtime` crate without changing rendering or interaction
behavior. Shell remains responsible for orchestration and translates its
control-plane services to the host's interfaces.

The current `shell/component.rs` imports shell service helpers, surface-layout
resolution, and the component lifecycle/input/frame types from `shell/types`.
Its catalog also depends on `ShellRunError` and component memoization. Tests
rely on shell fixtures and project-relative module paths. Moving the directory
wholesale would therefore introduce a dependency cycle or export shell
internals as the runtime API.

1. Extract independent runtime identity and service-field indexing, retaining
   their behavior and tests. Use the new crate from shell before moving callers.
2. Define the host-facing lifecycle, input, frame, diagnostics and service
   contracts at the boundary; adapt shell orchestration to those contracts.
   Keep policy and capability checks authoritative in their existing owners.
3. Move catalog compilation and component implementation with their fixtures;
   replace direct shell service and settings dependencies with injected or
   lower-level contracts. Keep shell-only integration tests in shell.
4. Remove shell component implementation and stale aliases; update crate
   boundaries and architecture, and run both runtime and shell suites.

Each stage must preserve stable node identities, module isolation, incremental
rendering, input and service behavior. The task is complete only when the
component implementation is hosted and tested outside shell.
