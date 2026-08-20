# Section 16 — Developer and authoring tools audit

**Audited:** 2026-08-20  
**Packages:** `mesh-tools-cli` and `mesh-tools-lsp`  
**Scope:** CLI lifecycle and package/profile operations; manifest, settings, and
`.mesh` diagnostics; formatting; completion, hover, definition, semantic
tokens; module/service knowledge indexes; and the public-contract boundary.
No production code was changed.

## Review execution

Three Luna xhigh passes were used as requested: a whole-process instruction-tree
review, an unconstrained logic/order review, and a direct code-error review.
Their findings were reconciled against the source and focused tests. The
review was not constrained to the existing flow; the recommended design below
introduces a shared graph-derived authoring snapshot and one transaction
boundary for package mutations.

## Logical process tree

```text
CLI argv / LSP client request
  ├─ CLI
  │    ├─ start -> Shell::new -> shell.run
  │    ├─ list/services -> discover -> resolve -> inspect public graph
  │    ├─ profile/config -> load files -> validate/merge -> persist or IPC
  │    ├─ install -> stage local/Git source -> validate manifest/graph
  │    │            -> mutate installed tree/profile -> digest/archive lock
  │    ├─ update -> plan Git candidates -> compare edits/contracts/capabilities
  │    │           -> checkout candidates -> save lock
  │    ├─ rollback -> load historical lock -> materialize revisions -> save lock
  │    └─ uninstall -> dependency check -> remove tree -> save lock
  │
  └─ LSP
       ├─ initialize -> discover workspace modules and service shapes
       ├─ didOpen/didChange -> classify URI
       │    ├─ module.json -> hand-authored schema -> JSON diagnostics
       │    ├─ settings.json -> registry-derived schema -> diagnostics
       │    └─ .mesh -> component parser + raw script/template extraction
       └─ requests -> cached document + independent registry/schema/knowledge
            ├─ completion / hover / definition
            ├─ formatting
            └─ semantic tokens
```

The intended invariant is that CLI, doctor, shell, and LSP all observe the
same canonical graph, contracts, paths, and generation; package mutations
should move source, lock, profile, and live activation as one recoverable
generation; and every LSP range should use the protocol's UTF-16 coordinates.
The current implementation does not preserve these invariants end to end.

## Severity-ranked findings

### P0 — Uninstall allows path traversal and arbitrary recursive deletion

`crates/tools/cli/src/main.rs:1060-1069` joins user input directly beneath the
modules directory after only trimming `@`, then calls `remove_dir_all`.
Absolute or `..`-containing IDs can escape the intended root. The same path
construction pattern is also used by install staging. Fix this before further
package operations: validate canonical module IDs, reject absolute/traversal
forms, reject symlink escapes, and verify canonical containment immediately
before every mutation.

### P1 — Package mutations are not atomic or recoverable

Install saves profile state before lock persistence (`main.rs:584-711`),
uninstall removes the module before lock persistence (`main.rs:1060-1074`), and
update/rollback mutate Git trees sequentially before archiving/saving the lock
(`update.rs:302-401`). A later checkout, digest, archive, or write failure can
leave source, lock, profile, and runtime at different generations. Rollback
also skips modules absent from the target lock and does not reconcile profiles.

Create one core-owned journaled transaction engine: stage all source changes,
validate the candidate graph/profile, atomically commit source + lock + profile,
request live activation with acknowledgement, and recover unfinished journals
at startup. Rollback must materialize the exact target module set and remove
modules absent from it.

### P1 — Live profile switching can report success while state diverges

`profile use` falls back to writing `active-profile` for every IPC error
(`main.rs:309-323`), including a live shell rejection; `try_send_ipc_command`
also treats EOF without a response as success (`main.rs:180-198`). Use typed
IPC results that distinguish absent shell, transport failure, rejection, and a
committed generation. Never persist a fallback pointer after an ambiguous live
switch. Define live versus restart-only semantics for every profile mutation.

### P1 — CLI and shell duplicate package ownership

Package installation/update/rollback/uninstall logic exists in the CLI and in
`crates/core/shell/src/shell/package.rs`. This violates the Section 16 seam and
allows transaction, validation, and recovery behavior to diverge. Make the CLI
a thin client of one public package transaction/service contract shared with
the shell and doctor.

### P1 — LSP authoring data goes stale and can disagree with runtime

`ModuleRegistry` is built only during initialization (`backend.rs:34-49`) and
does not refresh after module, manifest, backend-script, theme, locale, or pack
changes. It also uses a separate legacy-capable manifest loader
(`module_registry.rs:1`) and silently ignores load failures (`:107-125`);
duplicate IDs overwrite one another. Expose a graph-derived authoring snapshot
with canonical manifests, activation state, diagnostics, contracts, paths,
provenance, and a revision. Refresh it on workspace/module changes and make
all CLI, LSP, doctor, and runtime consumers use that snapshot.

### P1 — LSP manifest validation duplicates and under-approximates runtime rules

The hand-authored schema (`manifest/schema.rs:1-7`) is separate from canonical
`ModuleManifest` validation. Per-module diagnostics call the LSP JSON schema
(`manifest/mod.rs:48-73`) rather than `ModuleManifest::from_json_str`, so
interface requirements, capability syntax, composition restrictions, and
other runtime rules can be accepted by the editor and rejected at activation.
Generate authoring metadata from the canonical model or invoke canonical
validation while preserving source spans.

### P1 — LSP ranges use byte offsets instead of UTF-16 code units

The diagnostic range path (`diagnostics.rs:391-408`) computes characters from
UTF-8 byte offsets, while LSP requires UTF-16 units. Related conversion logic
also risks slicing at a non-character boundary (`util.rs:100-123`,
`hover.rs:122-124`). Centralize position conversion and test accents, emoji,
non-ASCII text in every block, and edits/diagnostics/completion/hover.

### P2 — `--replace` and CLI flags do not implement their advertised contract

`--merge` is advertised but not parsed (`main.rs:974-1024`), conflicting flags
are not rejected, and replace mode bypasses edit refusal without reliably
resetting/cleaning a Git checkout (`update.rs:176-182`, `314-334`). Use a typed
argument parser with strict unknown/conflict handling and make replace an
explicit force/reset/clean operation with a clear confirmation boundary.

### P2 — LSP analysis is syntax-blind and hides useful mid-edit diagnostics

Raw substring/line scanning of Luau (`document.rs:367-503`,
`diagnostics.rs:145-249`) can inspect comments and strings as `refs.*`, miss
valid multiline/nested forms, and infer backend service shapes from indentation
(`module_registry.rs:329-378`). Parser errors also short-circuit all secondary
`.mesh` diagnostics (`diagnostics.rs:7-14`). Use the real Luau/component AST
with source spans, keep recoverable partial trees, and treat heuristic service
inference as an explicit fallback only.

### P2 — Additional authoring correctness gaps

- The custom JSON parser mishandles valid Unicode escapes such as `\\u0065`
  (`json/diagnostics.rs:338-355`). Prefer a standards-compliant parser with
  source-span tracking or repair and fuzz it.
- Initialization uses only `root_uri` (`backend.rs:65-80`), ignoring clients
  that provide `workspace_folders` only.
- Manifest flavor fallback uses textual substring checks
  (`manifest/mod.rs:97-105`), so malformed JSON can select the wrong schema.
- Hover accepts a registry but ignores it (`hover.rs:11-20`), leaving service
  field/command documentation incomplete.
- Definition resolution accepts arbitrary relative/absolute import paths
  (`definition.rs:88-119`) without module-root containment or existence checks.

## Recommended implementation order

1. Close uninstall/install path traversal and symlink containment.
2. Establish the shared journaled package transaction and exact-generation
   rollback; add failure injection for every mutation boundary.
3. Replace IPC fallback with typed acknowledgement and explicit live/restart
   semantics.
4. Make CLI use the shared package contract and strict typed argument parsing.
5. Build and refresh one graph-derived authoring snapshot for CLI, LSP, doctor,
   and runtime.
6. Generate manifest/settings schemas from canonical contracts and preserve
   source spans through validation.
7. Centralize UTF-16 conversion; replace raw Luau/JSON scanners with parser/AST
   data, recoverable partial trees, and standards-compliant Unicode handling.
8. Add workspace-folder support, secure definition imports, registry-backed
   hover, revisioned didChange handling, and generated knowledge indexes.

## Focused verification

`nix develop -c cargo test -p mesh-tools-lsp --lib --tests` passed: 84 unit
tests, 1 real-manifest integration test, and 5 real-settings integration tests.
No production code was changed. Add failure-injection tests for package
transactions, traversal/symlink tests, typed IPC rejection/EOF tests, Unicode
UTF-16 tests, workspace-folder initialization, registry refresh generations,
JSON escape parsing, malformed manifest flavor selection, and rapid versioned
`didChange` updates.

