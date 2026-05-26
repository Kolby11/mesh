# Roadmap: MESH

## Milestones

- ✅ **v1.14 Unified Luau Scripting Runtime** — Phases 74-80 shipped 2026-05-26 ([archive](milestones/v1.14-ROADMAP.md))
- ⏭️ **v1.15 Persistent Storage System** — next milestone
- ⏭️ **v1.16 Elements Improvements** — queued after v1.15
- ✅ **v1.13 Manifest I18n Contract** — Phases 70-73 shipped 2026-05-24 ([archive](milestones/v1.13-ROADMAP.md))
- ✅ **v1.12 Module Object Contract** — Phases 65-69 shipped 2026-05-23 ([archive](milestones/v1.12-ROADMAP.md))
- ✅ **v1.11 Surface Keybind Completion** — Phases 60-64 shipped 2026-05-23 ([archive](milestones/v1.11-ROADMAP.md))

## Last Shipped Milestone: v1.14 Unified Luau Scripting Runtime

**Goal:** Make frontend and backend Luau authors use one explicit model for
`require(...)` imports, runtime-provided `self` context, public/private script
members, frontend component definitions/instances, named event channels, and
automatic dependency rerendering.

**Key accomplishments:**

- Runtime-provided `self.meta` identity now exists for frontend components and backend providers.
- Frontend/backend `require(...)` now covers shell APIs, service/interface proxies, Luau helpers, and component definitions.
- Lua locals are private while non-local variables/functions are public component/provider members.
- Frontend component imports, markup instantiation, direct public-field attributes, and `bind:this` mounted instance references are supported.
- Service and local/provider events are exposed as named channels such as `audio.VolumeChanged:on(fn)` and `self.Changed:fire(payload)`.
- Render-read service fields, locale/theme data, and bound public fields participate in automatic rerendering.
- Shipped navigation/audio frontend examples and bundled backend providers now demonstrate the unified scripting contract.

**Known deferred items at close:** tech debt only; see [v1.14 audit](milestones/v1.14-MILESTONE-AUDIT.md).

**Archive artifacts:**

- `.planning/milestones/v1.14-ROADMAP.md`
- `.planning/milestones/v1.14-REQUIREMENTS.md`
- `.planning/milestones/v1.14-MILESTONE-AUDIT.md`
- `.planning/milestones/v1.14-phases/`

## Next Milestone: v1.15 Persistent Storage System

**Goal:** Implement `self.storage` as shell-backed, component/provider
instance-scoped persistent key-value storage using atomic JSON files under the
MESH/XDG data area.

**Planned scope:**

- Scoped storage identity derived from `self.meta` for frontend components and backend providers
- JSON-like table reads, writes, removals, snapshots, and invalid value diagnostics
- Atomic persistence through temp-file write plus rename under the MESH/XDG data area
- Non-fatal corrupt-file recovery with private scope isolation by module/component/provider identity
- Lifecycle loading before `mount/start` and flushing on `unmount/stop` plus orderly shell shutdown
- Render dependency integration so storage readers rerender only when watched values change
- Shipped proof through a real UI preference or provider setting path

**Out of scope:**

- Cross-module storage reads
- Schema-backed settings UI
- Remote synchronization
- Storing functions, userdata, component definitions, component instances, or event channels

## Queued Milestone: v1.16 Elements Improvements

**Goal:** Add common native markup controls that reduce custom component
workarounds and improve shipped UI behavior.

**Planned scope:**

- First-class `<select>` and `<option>` element support in MESH markup
- Visible dropdown/popup behavior with vertical option layout
- Keyboard navigation, focus, selection, disabled states, and accessibility metadata
- Value binding/change events suitable for Luau component state
- Styling hooks that fit the existing shell CSS profile without requiring browser CSS compatibility
- Shipped proof by replacing the navigation bar language selector's horizontal custom menu

## Backlog

### Future: Package Distribution

Remote package fetching, third-party dependency resolution, and LSP import
completion remain future work after the runtime import contract is stable.
