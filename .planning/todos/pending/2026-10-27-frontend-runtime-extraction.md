# Frontend runtime extraction (SH-08)

Move the component host from `mesh-core-shell` into a testable
`mesh-core-frontend-runtime` crate without changing rendering or interaction
behavior. Shell remains responsible for orchestration and translates its
control-plane services to the host's interfaces.

Runtime identity, service-field and generation indexes, catalog compilation
and reload, service dispatch, and shared locale projections now live in the
runtime crate. The lifecycle, input and frame contracts are in the frontend
shell-adapter crate.

The component implementation still depends on surface settings and directly
accesses shell-facing types and helpers. Its tests combine private component
fixtures with shell orchestration, live surface, IPC and profiling tests.

1. Move the component host into the runtime crate, replacing remaining shell
   dependencies with lower-level interfaces or injected adapters. Keep policy
   and capability checks authoritative in their existing owners.
2. Move component-only tests with the host and adapt fixtures to runtime-owned
   contracts. Keep shell orchestration and live integration tests in shell
   without depending on private component implementation details.
3. Remove stale shell aliases, update crate boundaries and architecture, and run
   both runtime and shell suites.

Each stage must preserve stable node identities, module isolation, incremental
rendering, input and service behavior. The task is complete only when the
component implementation is hosted and tested outside shell.
