# Section 10 — Frontend compiler and host: improvement audit

**Audited:** 2026-08-20  
**Scope:** `mesh-core-frontend` and `mesh-core-frontend-host`: frontend
entrypoint compilation, `.mesh` parsing and import/composition handling,
expression and style evaluation, props, `WidgetNode` construction, catalog
records and invalidation, service observations, host lifecycle, child-surface
and input/core requests, diagnostics, and shell integration.

This is an audit record, not a second task tracker. Open work belongs in
[`docs/BACKLOG.md`](../../../../docs/BACKLOG.md). No production code was
changed for this review.

## Logical instruction/process tree

Section 10 is a compiler-to-runtime boundary, not only a parser. Its output is
an executable, stateful frontend instance whose compiled roots, capabilities,
source dependencies, service observations, and diagnostics must stay aligned.

```text
module graph + manifest + profile + module source
  │
  ├─ frontend entrypoint resolution
  │    ├─ primary surface/component entrypoint
  │    └─ extension-point contribution entrypoints
  │
  ├─ source loading and parsing
  │    ├─ source-root containment and canonical paths
  │    ├─ markup/template AST
  │    ├─ script/import declarations
  │    ├─ props/style/i18n blocks
  │    └─ source spans and diagnostics
  │
  ├─ import and composition graph
  │    ├─ owner-scoped local component bindings
  │    ├─ declared module component imports
  │    ├─ interface API imports
  │    ├─ cycle/alias/path validation
  │    └─ watched source dependency set
  │
  ├─ static contract validation
  │    ├─ symbol and expression scope
  │    ├─ child props and handler names
  │    ├─ extension-point/slot declarations
  │    ├─ interface/version/capability requirements
  │    └─ primary-root and contribution-root validation
  │
  ├─ compiled catalog publication
  │    ├─ primary compiled roots
  │    ├─ contribution compiled roots
  │    ├─ reverse dependencies and source fingerprints
  │    └─ one atomic catalog revision
  │
  ├─ frontend host lifecycle
  │    ├─ runtime/VM creation and capability context
  │    ├─ props/settings and locale/theme snapshots
  │    ├─ service payload cache and observation summary
  │    └─ mount/reload/catalog-change/recovery transitions
  │
  ├─ runtime evaluation and tree construction
  │    ├─ Luau state and host expressions
  │    ├─ style/prop resolution
  │    ├─ template conditionals and keyed loops
  │    ├─ imports, slots, contributions, and child instances
  │    ├─ typed handlers and source metadata
  │    └─ `WidgetNode` tree plus dirty/revision outputs
  │
  ├─ shell-facing effects
  │    ├─ service/core/input requests
  │    ├─ child-surface requests
  │    ├─ diagnostics and profiling
  │    └─ capability/authorization boundary
  │
  └─ style/layout/retained rendering → Wayland presentation

feedback loops:
  source edit → watched paths → catalog compile → atomic revision → runtime
  service update → capability gate → runtime state → observation/invalidation
                 → selective rebuild → style/layout/render
  handler/input → typed host effect → shell decision → component state/event
  settings/theme/locale/catalog change → snapshot refresh → rebuild/repaint
  failed compile/reload → diagnostic + last-known-good catalog/runtime
```

### Required invariants

1. Every source path is canonicalized and contained by its owning module root;
   imports cannot read or execute files outside that root.
2. Local imports are resolved in the scope of their owning source component,
   not through one alias-keyed global map.
3. Primary roots and contribution roots use the same validation, dependency,
   reload, and interface/capability contract.
4. A catalog revision is atomic: compiled roots, reverse dependencies, watch
   paths, and affected runtime instances describe the same generation.
5. Service data enters Luau only after capability authorization. Event
   subscription does not implicitly grant state-read access.
6. Preview, composition, and live paths implement one expression/type/coercion
   contract, preferably through one parser/IR and one semantics implementation.
7. Failed compilation, prop publication, script execution, or catalog reload
   preserves a usable last-known-good state and emits a truthful, source-located
   diagnostic.
8. The frontend host exposes a small renderer-neutral effect contract; shell,
   Wayland, package, debug, and authorization policy live in adapters.

## Verification

- `nix develop -c cargo check -p mesh-core-frontend -p
  mesh-core-frontend-host` passed.
- `git diff --check` passed before this report was written.
- `nix develop -c cargo test -p mesh-core-frontend --lib` passed: 64 passed,
  20 ignored, 0 failed.
- `nix develop -c cargo test -p mesh-core-frontend-host --lib` passed: 0
  tests, 0 failed.
- The requested Luna xhigh process mapper, logical/order reviewer, direct
  code-error reviewer, and focused compiler/host/runtime seam reviewer were
  launched. All four reports returned and converged on the findings below; no
  worker edited files. Findings were checked against the local source and
  package tests.

## Confirmed findings

### 1. P1 — Frontend imports can escape the module directory

`compile_frontend_entrypoint()` joins the manifest entrypoint directly at
`crates/core/frontend/compiler/src/compile.rs:83-90`. Local component imports
accept absolute paths and `..` traversal in `compile.rs:277-288`, then read the
result at `compile.rs:217-220`. The script import parser also accepts these
targets at `crates/core/ui/component/src/parser/script.rs:134-140`.

A module can therefore cause compilation to read arbitrary `.mesh` sources and
their scripts outside its module root.

**Improve it:** reject absolute paths, canonicalize targets, enforce
containment under the canonical module root, and reject symlink escapes. Add
negative tests for absolute paths, `@src/../../...`, traversal through nested
imports, and symlinked components.

### 2. P1 — Service payloads can bypass read capabilities

In `crates/core/shell/src/shell/component/shell_component/mod.rs:247-260`, a
runtime without `has_read` is allowed to receive the payload when it observes
the event. `apply_service_payload_with_fingerprint()` places the value in the
Lua-visible service state; the underlying publication path is in
`crates/core/runtime/scripting/src/context/runtime/state.rs:54-60`.

An event subscription can consequently make service state readable without
the corresponding `service.<name>.read` capability.

**Improve it:** keep undeclared payloads in a Rust-owned cache only. Treat
interface events as event-only unless the contract explicitly grants state
access, and publish service fields into Luau only after authorization. Add a
denied-capability proxy/global access regression test.

### 3. P1 — Hot reload leaves contribution roots stale

Catalog construction compiles primary and extension-point contribution roots in
`crates/core/shell/src/shell/component/catalog.rs:26-42`, and contribution paths
join the watch set. `FrontendSurfaceComponent::reload_source()` instead calls
only `compile_frontend_module()` at
`crates/core/shell/src/shell/component/shell_component/mod.rs:1253-1268`.

Reloading a host can therefore replace its primary compilation while leaving
compiled contribution entries and their source/watch state stale.

**Improve it:** reload through the complete catalog compilation path and
publish one atomic generation containing primary and contribution records. Test
editing a contribution, then editing its host, without restarting the shell.

### 4. P1 — Contribution roots bypass interface validation

The catalog validates interface imports for primary entries in
`crates/core/shell/src/shell/component/catalog.rs:524-555`, while contribution
entries are compiled separately at lines 466-505. The validation loop does not
walk those contribution roots, so an imported interface can evade graph
contract validation and fail only at runtime.

**Improve it:** validate every compiled root—including contributions—against
the source module’s declared interface requirements, availability, and version
range. Add a negative contribution-import test.

### 5. P1 — Root template expressions are not statically validated

`validate_standalone_imports()` starts the root with `strict_scope = false` at
`compile.rs:291-307`. Expression and attribute validation is skipped in that
mode at `compile.rs:364-390`, so unknown root symbols can reach runtime without
a compiler diagnostic.

**Improve it:** validate root and nested component scopes uniformly using
parser/runtime-produced symbol tables, while allowing only explicit built-ins,
props, loop locals, imports, and script declarations. Preserve source spans in
the resulting diagnostics.

### 6. P1 — Live and preview expression semantics are split

`crates/core/frontend/compiler/src/render/expr_eval.rs:10-33` routes
translation expressions and preview/no-composition paths through the handwritten
subset evaluator in `expr.rs:13-26`. Composition paths otherwise evaluate
through Luau in `crates/core/shell/src/shell/component/composition.rs:73-95`.

The same authored expression can therefore differ in accepted syntax, Luau
truthiness, coercion, escaping, function behavior, and service-read tracking
depending on how it is rendered.

**Improve it:** compile expressions once into a shared typed IR and evaluate
through one semantics implementation. Preview should provide a host for the
same contract, not a separate language. This also removes the need to expand
temporary handwritten parsing.

### 7. P1 — The frontend host contract owns too much shell policy

`mesh-core-frontend-host` directly depends on Wayland, render buffers/commands,
capabilities, debug, package installation, profile switching, and a large
`CoreRequest` enum (`crates/core/frontend/host/Cargo.toml:8-18` and
`crates/core/frontend/host/src/lib.rs:274-424,471-796`). This is wider than the
Section 10 isolation seam and makes shell policy part of the compiler-facing
ABI.

**Improve it:** split the crate into a small renderer-neutral frontend ABI and
a shell adapter. Replace raw requests with typed, capability-scoped effects
carrying module/instance identity, catalog revision, and source context; keep
Wayland, package, debug, and authorization policy in adapters.

### 8. P2 — Contribution-only dependencies do not fully invalidate hosts

Catalog reverse invalidation walks primary catalog entries in
`crates/core/shell/src/shell/component/catalog.rs:354-387`, while contribution
roots live separately in `extension_point_entries` at lines 54-58 and 495-505.
An imported frontend module used only by a contribution can change without
propagating the affected generation to the host surface.

**Improve it:** build one reverse dependency graph over primary and every
contribution root, propagate affected generations to host instances, and test a
contribution-only imported module change.

### 9. P2 — Local component aliases silently overwrite one another

`collect_imports()` inserts local components at `compile.rs:217-225`, but
`insert_local_component()` always overwrites the alias at `compile.rs:247-258`.
The recursively discovered components are kept in one alias-keyed map, so
different owning components can legally use the same alias yet resolve to the
last file processed.

**Improve it:** key bindings by owning component and canonical source identity;
reject conflicting aliases in one scope, and resolve each template reference
through its explicit import binding. Add duplicate-alias and nested-scope tests.

### 10. P2 — Script symbol discovery is a fragile line parser

`compile.rs:535-614` extracts functions, state, service bindings, and imports
by scanning source lines. `compile.rs:617-668` then applies ad-hoc string
parsing for assignments and arguments. Multiline declarations, comments,
local scopes, aliases, and ordinary Luau syntax can produce false positives or
false unknown-symbol errors.

**Improve it:** obtain symbol information from the actual Luau parser/compiler,
or remove this validation layer in favor of runtime/compiler diagnostics. Do
not make the line scanner the source of truth for the language contract.

### 11. P2 — The expression parser can panic on non-ASCII input

`crates/core/frontend/compiler/src/expr.rs:333-367` advances byte offsets and
then slices `expr[i..]`. A multibyte character outside a quoted literal can
make `i` point inside a UTF-8 code point before the slice, causing a panic
instead of a diagnostic.

**Improve it:** scan with `char_indices()` or validate byte boundaries before
slicing. Add expressions containing non-ASCII identifiers/literals around every
supported operator and assert they never panic.

### 12. P2 — Failed runtime prop publication can desynchronize Rust and Luau

`crates/core/shell/src/shell/component/runtime.rs:23-49` logs a failed
`set_member_state()` call and then writes only the fallback `ScriptState`. The
Lua environment can retain the old value while tree evaluation sees the newer
state value.

**Improve it:** make prop publication transactional, return the failure and
invalidate/recreate the runtime, or guarantee an equivalent Lua environment
update. Add a failure-injection test that checks both state surfaces.

### 13. P2 — Diagnostics lose category and source location

`drain_script_diagnostics()` labels every diagnostic as an unavailable
interface at `crates/core/shell/src/shell/component/runtime.rs:216-228`, even
when the cause is storage or another runtime failure. Separately, parser errors
such as `InvalidTemplate`, `UnexpectedClose`, and `UnclosedBlock` frequently
use line `0` in `crates/core/ui/component/src/parser/markup.rs:345-379`, and
`CompileFrontendError` preserves only the path/message at `compile.rs:27-38`.

**Improve it:** preserve typed diagnostic categories and byte spans through the
AST, compiler error, shell diagnostic, LSP, and debug paths. Add malformed
markup, storage, interface, and script-error assertions with actual locations.

### 14. P2 — Catalog publication can lose a newer generation

`FrontendCatalogHandle::replace()` snapshots and publishes independently at
`crates/core/shell/src/shell/component/catalog.rs:280-298`, while `restore()`
unconditionally overwrites state at lines 301-303. A stale reload failure can
therefore roll back a newer successful catalog generation.

**Improve it:** use a single-writer coordinator or compare-and-swap against the
expected revision. Add concurrent replacement and stale-rollback tests.

### 15. P2 — Child-surface kind is declared but not enforced

`ChildSurfaceRequest` distinguishes `Popover` and `Overflow` in
`crates/core/frontend/host/src/lib.rs:34-49`, but reconciliation in
`crates/core/shell/src/shell/runtime/render/child.rs:102-238` does not branch on
the kind; both follow the same popup ownership/configuration path.

**Improve it:** either define both kinds as one compositor primitive and remove
the distinction, or enforce separate lifecycle, focus, dismissal, placement,
and ownership rules. Add behavior tests for both request kinds.

### 16. P1 — Frontend lifecycle hooks are not dispatched consistently

The host mount path initializes a runtime at
`crates/core/shell/src/shell/component/shell_component/mod.rs:79-92`, and
runtime creation calls `init` around
`crates/core/shell/src/shell/component/runtime.rs:414-435`, but the documented
frontend `mount`/`unmount` lifecycle is not dispatched. Runtime cleanup clears
contexts directly during reload and catalog changes at
`shell_component/mod.rs:1269-1271,1298-1303`, even though `unmount` also flushes
storage in `crates/core/runtime/scripting/src/context/runtime/lifecycle.rs:155-191`.

**Improve it:** dispatch `mount(self)` after initialization and `unmount(self)`
before runtime removal, reload, deactivation, and replacement. Make lifecycle
completion and failure part of the runtime state transition, with tests for
storage flushes, subscriptions, and reload ordering.

### 17. P1 — Expression failures silently become incorrect trees

Runtime expression failures are logged and converted to `null` in
`crates/core/shell/src/shell/component/composition.rs:79-96`. The fallback
evaluator can also treat unsupported syntax as a variable path rather than
producing a diagnostic in `crates/core/frontend/compiler/src/expr.rs:193-250`.

The resulting tree may silently omit content, choose a false branch, or pass a
null prop while the module appears healthy.

**Improve it:** preserve source spans and emit structured compile/runtime
diagnostics. Distinguish an intentional `nil` result from parse/evaluation
failure, and use a last-known-good tree or explicit error placeholder policy
instead of silently changing UI semantics.

### 18. P2 — Imported component props lack an import-boundary contract

Catalog validation checks the target module kind and dependency at
`crates/core/shell/src/shell/component/catalog.rs:777-800`, but composition
forwards props into child runtimes at
`crates/core/shell/src/shell/component/composition.rs:145-193,255-310` without
the equivalent public-prop name, visibility, type, constraint, or required-value
validation.

**Improve it:** publish a normalized public-prop schema in each compiled
component record and validate imported and contribution props before rendering.
Report unknown/private/invalid values at the importing source span.

### 19. P2 — Local and module imports can collide silently

Local aliases are inserted independently of module-import aliases in
`crates/core/frontend/compiler/src/compile.rs:247-274`. Runtime resolution then
prefers local components in
`crates/core/shell/src/shell/component/composition.rs:119-135`.

An authored alias collision can silently change which component is rendered
when an import is added or reordered.

**Improve it:** reject cross-kind alias collisions during compilation, or use
one typed import namespace with explicit precedence and a diagnostic. This is
separate from owner-scoping aliases within nested local components.

### 20. P2 — The host frame boundary has no coherent revision token

`ShellComponent` exposes independent mutable operations for render, paint,
input, catalog changes, child-surface requests, and invalidation in
`crates/core/frontend/host/src/lib.rs:471-796`. These operations do not carry a
single catalog/runtime/service revision or frame transaction, making stale tree,
service, catalog, and child-surface data easier to combine accidentally.

**Improve it:** return a typed `FrontendFrame` containing tree generation,
catalog generation, service generations, invalidation class, diagnostics,
child-surface requests, and paint metadata as one coherent snapshot. Shell
adapters should reject effects from obsolete revisions.

## Better feature direction

The logical review suggests replacing the current “compiler plus a large shell
trait” boundary with explicit immutable artifacts and typed effects:

```text
CompiledFrontendRevision {
  module_id,
  primary_root,
  contribution_roots,
  owner_scoped_imports,
  interface_requirements,
  capability_requirements,
  watched_sources,
  reverse_dependencies,
  source_spans,
}

FrontendFrame {
  catalog_revision,
  runtime_revision,
  state_snapshot,
  service_observations,
  widget_tree,
  invalidation,
  effects,
  diagnostics,
}
```

The compiler would emit `CompiledFrontendRevision`; the host would evaluate it
against immutable settings/theme/locale/service snapshots and return a
`FrontendFrame`. Shell adapters would authorize and execute typed effects only
if their catalog and runtime revisions still match. This makes stale requests,
capability boundaries, contribution invalidation, and last-known-good reloads
testable without coupling compiler code to Wayland or package policy.

## Recommended implementation order

1. Close source-path escapes and make local import bindings owner-scoped.
2. Gate all service payload publication on capabilities; add event-only tests.
3. Compile, validate, watch, invalidate, and reload primary/contribution roots
   as one atomic catalog revision.
4. Validate root and contribution scopes/interfaces with real parser symbols and
   source spans.
5. Unify expression parsing/evaluation and remove the UTF-8 panic path.
6. Make prop publication transactional and diagnostics typed/source-located.
7. Split the frontend host ABI from shell/Wayland/package/debug adapters and
   enforce child-surface kind semantics.

## Regression matrix

- Absolute, traversal, and symlinked entrypoint/import paths fail closed.
- Duplicate aliases in one owner scope fail; identical aliases in separate
  component scopes resolve to their own files.
- Primary and contribution roots receive identical import, interface, prop,
  capability, watch, reload, and diagnostic validation.
- Contribution-only dependency edits invalidate and rebuild the host surface;
  stale catalog rollback cannot overwrite a newer revision.
- A runtime with an event subscription but no read grant receives event metadata
  only and cannot read service fields through globals, proxies, or cached state.
- Root, nested, preview, and live expressions share Luau semantics, including
  truthiness, coercion, translation, service reads, and non-ASCII input.
- Failed runtime prop publication leaves no divergent Rust/Lua values and
  produces a recoverable runtime/diagnostic outcome.
- Malformed markup, script, storage, interface, and composition errors retain
  actual source spans and truthful diagnostic categories.
- Popover and overflow child surfaces obey their declared ownership, placement,
  focus, dismissal, and lifecycle semantics.
