---
created: 2026-08-08T00:00:00.000Z
title: Author-declared node slots and a replaceable visual composition module
area: module-system
files:
  - docs/spec/01-module-system.md
  - docs/spec/03-components.md
  - docs/spec/08-settings.md
  - crates/core/ui/component/src/template.rs
  - crates/core/ui/component/src/parser/markup.rs
  - crates/core/frontend/compiler/src/lib.rs
  - crates/core/extension/module/src/package/profile.rs
  - crates/core/extension/module/src/package/composition.rs
  - crates/core/extension/module/src/package/installed_graph/contributions.rs
  - crates/core/shell/src/shell/discovery.rs
  - crates/core/shell/src/shell/runtime/request.rs
  - crates/core/shell/src/shell/component/composition.rs
  - modules/frontend/composition-editor/module.json
---

# Goal

Give MESH users two coherent customization levels:

1. A non-coder uses a visual node editor to add, remove, configure, and reorder
   components inside regions the component author explicitly made customizable.
2. A coder edits the owning module's `.mesh`, Luau, and CSS for changes outside
   those regions or beyond the public prop surface.

The visual editor is an ordinary frontend module. It receives no module-id
allowlist, direct profile-file access, parser privilege, or Rust-owned UI. Core
owns only the mechanisms every possible editor needs: typed discovery,
validation, profile persistence, transactional activation, and mounting.

The first proof is the navigation bar. Its author keeps the outer row and its
start/center/end layout in source, but declares those three regions as
customizable. A user can visually arrange compatible launcher, workspace,
clock, status, and tray components within them.

# Product boundary

This is a **component placement editor**, not a second UI language and not a
serialization of MESH's runtime `WidgetNode` tree.

Version 1 persists only:

- a stable placement id;
- a reference to a public component contribution;
- exposed, literal prop overrides; and
- order within one author-declared slot.

It does not persist base elements (`row`, `box`, `text`, etc.), arbitrary tree
structure, Luau expressions, handlers, bindings, CSS declarations, or surface
policy. Those remain source-level authoring. This line is important: attempting
to round-trip all `.mesh` behavior through a node graph would create the
parallel "lite" model rejected by spec 01 §2.6 and would either lose behavior
or grow into another compiler frontend.

Nested customization does not require arbitrary nested JSON nodes. A mounted
component may declare its own slots; those become a separate, recursively
addressable customization scope in a later phase. The first release supports
slots on profile root components only.

# Reuse the existing module model

The design extends the shipped extension-point machinery rather than creating
a second component registry:

- an interface module declares a named, versioned extension point;
- frontend/component modules contribute public `.mesh` entries to it;
- a host declares and renders the extension point;
- a composition/profile selects placements for a **named customizable host
  slot**; and
- each contribution still runs with its source module's VM, capabilities,
  settings namespace, failure boundary, and public props.

Ordinary extension-point slots retain their current automatic behavior: every
resolved contribution is rendered in deterministic order. A customizable slot
uses explicit profile selection instead. The two modes share contracts,
catalog entries, compilation, and runtime isolation; only selection differs.

Use the public term **placement node** for the persisted record. Do not call it
`WidgetNode`, which already means the core retained UI node, and do not call a
component an element.

# Authoring contract

## Named customizable slots

Extend `<slot>` with a stable local `name` and a static `mode`:

```html
<template>
  <row class="navigation-bar">
    <row class="start">
      <slot name="start"
            extension-point="mesh.navigation.item"
            mode="customizable" />
    </row>
    <row class="center">
      <slot name="center"
            extension-point="mesh.navigation.item"
            mode="customizable" />
    </row>
    <row class="end">
      <slot name="end"
            extension-point="mesh.navigation.item"
            mode="customizable" />
    </row>
  </row>
</template>
```

Rules:

- `name` is component-local, required for `mode="customizable"`, and unique in
  that compiled component entry.
- `extension-point` stays the compatibility contract. It is never a host or
  contributor module id.
- all three attributes are static. Dynamic slot identity or mode is a compile
  diagnostic because persisted configuration could not address it reliably.
- a customizable slot is ordered and multiple by default. The extension-point
  contract may constrain cardinality; a later slot option may narrow it but
  never widen the contract.
- the host manifest must continue to declare the compatible point in
  `mesh.hosts`; markup alone does not silently grant the trust decision.
- existing `<slot extension-point="…"/>` means `mode="automatic"` and retains
  current behavior.

## Defaults

Defaults belong to the component author and are compiled with the component,
not copied into every user profile. Add a `defaults` list to the host record,
keyed by local slot name:

```json
"hosts": {
  "mesh.navigation.item": {
    "version": ">=1.0",
    "layout": "row",
    "slots": {
      "start":  { "defaults": ["@mesh/navigation-bar:launcher"] },
      "center": { "defaults": ["@mesh/navigation-bar:workspaces"] },
      "end":    { "defaults": ["@mesh/navigation-bar:clock",
                                  "@mesh/navigation-bar:status"] }
    }
  }
}
```

Each reference is the existing stable contribution identity
`<module-id>:<contribution-id>`, never a source path. Every referenced default
must resolve to the slot's extension-point contract. This permits private
implementation components inside the host module while ensuring anything the
user may place is an explicitly public alternate root.

Absence of a user/composition override renders these defaults. Resetting a slot
deletes its sparse override and immediately follows newer author defaults after
an update.

## Persisted profile delta

Add `nodeSlots` to `CompositionSpec` and `ShellProfile`, keyed first by root
instance and then by the component-local slot name:

```json
{
  "schemaVersion": 3,
  "from": { "module": "@mesh/desk", "version": "2.2.0" },
  "nodeSlots": {
    "@mesh/navigation-bar#top": {
      "start": {
        "nodes": [
          {
            "id": "launcher",
            "use": "@mesh/navigation-bar:launcher",
            "props": { "compact": true }
          },
          {
            "id": "weather",
            "use": "@alice/weather:nav-item",
            "props": { "units": "metric" }
          }
        ]
      },
      "center": { "nodes": [] }
    }
  }
}
```

Semantics:

- absence of a slot key inherits the next less-specific layer;
- an explicit empty `nodes` list intentionally empties the slot;
- an override replaces the complete ordered list. Ordered lists never
  deep-merge;
- `id` is unique within the slot and supplies stable runtime identity across
  reorder and prop edits;
- `use` resolves through the installed extension-point contribution index;
- `props` contains only literal JSON values validated against the target
  component's derived `<props>` schema and the extension-point contract;
- a placement can use the same contribution more than once when its contract
  permits it, but each placement id must differ; and
- source defaults → base composition → derived composition → profile, with the
  most specific whole slot list winning.

Profile schema version 3 should fail with a focused migration diagnostic when
read by an older/newer incompatible runtime; do not add a compatibility reader.
The migration from schema 2 is mechanical: add an empty `nodeSlots` map and set
the version to 3.

# Validation and lifecycle

Validation happens before any profile mutation is committed. For every node:

1. Resolve the root instance and compiled slot descriptor.
2. Verify that the slot exists and is customizable.
3. Resolve `use` to an enabled or install-available contribution.
4. Check the contribution's extension-point name and version.
5. Validate props and reject unknown/private props.
6. Validate cardinality, duplicate placement ids, and module activation.
7. Build the candidate effective composition and frontend catalog.
8. Prepare affected component instances, then atomically persist and commit.

Invalid edits return structured diagnostics and leave both the active UI and
profile file unchanged. At minimum add:

| Diagnostic | Condition |
| --- | --- |
| `customizable_slot_missing_name` | customizable slot has no static name |
| `duplicate_customizable_slot` | two slots in one entry use the same name |
| `unknown_node_slot` | profile names no compiled slot on the root |
| `node_slot_not_customizable` | profile targets an automatic slot |
| `node_contribution_incompatible` | contribution does not satisfy the slot contract/version |
| `invalid_node_props` | props fail the public component schema |
| `duplicate_node_placement_id` | ids repeat within one slot |
| `node_slot_cardinality` | list violates the contract/host limit |
| `orphaned_node_slot_override` | an update removed/renamed the root or slot |
| `orphaned_node_contribution` | an update removed/renamed a selected contribution |
| `node_edit_generation_conflict` | editor wrote against stale composition state |

Like existing profile overrides, orphaned node data is retained and reported,
not silently deleted. Reset/prune is explicit. A missing selected component
renders the bounded error placeholder for an already-active profile, while a
new edit that introduces the missing reference is rejected.

The activation closure must include every module selected by an effective node
slot plus its ordinary dependencies, interfaces, and resources. A placement
does not grant capabilities; install/review shows the selected module's existing
capability requirements exactly as if it were mounted elsewhere.

# Generic core interface

Add a core-provided `mesh.composition` interface. It is the only authority the
visual editor needs and is available to any module declaring the same contract
and capabilities.

Read-only state:

- active profile id and composition generation;
- root instances and their labels/health;
- each customizable slot's stable address, localized metadata, contract,
  limits, defaults, effective placements, source layer, and diagnostics;
- compatible contribution palette entries with localized label/description,
  icon, module identity, health, and derived public prop schema; and
- capability/install status sufficient to explain why a candidate is
  unavailable.

Methods:

```text
apply_node_slot(profile_id, root_instance, slot, nodes, expected_generation)
reset_node_slot(profile_id, root_instance, slot, expected_generation)
prune_orphaned_node_slots(profile_id, expected_generation)
```

`apply_node_slot` replaces one whole ordered list through the validation and
transaction above. Whole-list replacement makes drag/reorder atomic and avoids
an order-dependent sequence of `insert/move/delete` calls. The generation is
optimistic concurrency control, so two editors cannot silently overwrite one
another. Successful mutation increments generation and republishes state.

Capabilities:

- `service.composition.read` for catalog and effective state;
- `service.composition.control` for apply/reset/prune.

Do not fold control into `service.packages.control`: arranging an already
installed component is materially less authority than installing/removing
code. If a chosen node is not installed, the editor may separately offer an
install action through `mesh.packages`, which requires that existing stronger
capability.

The shell implementation may use `ProfilePaths`, composition resolution, the
installed graph, and the frontend catalog internally. The interface must not
expose filesystem paths, Rust structs, or editor-specific canvas concepts.

# Runtime behavior

Build customizable slots on the existing extension contribution mounting seam
in `shell/component/composition.rs`:

- `automatic` reads the graph's resolved ordered contribution list, unchanged;
- `customizable` reads the effective placement list for its root/slot address;
- each placement instantiates the referenced alternate root under an identity
  derived from `(root instance, slot name, placement id)`;
- reordering preserves the component VM and `self.storage` for unchanged ids;
- prop-only edits update the same instance through normal reactive props;
- removal unmounts exactly that instance; addition creates exactly one;
- contributor VMs/capabilities remain isolated from the host; and
- one failed placement produces its bounded placeholder without blanking the
  navigation bar or sibling placements.

Catalog invalidation becomes slot-specific: a change to one effective list
invalidates that host slot and affected mounted entries, not every frontend.
The existing changed-extension-point and compiled-source reuse paths are the
starting point; include a release benchmark only if implementation changes a
measured hot path.

# Visual editor module

Ship the first editor as `modules/frontend/composition-editor`, an ordinary
window-role frontend module. It declares:

- `mesh.composition` as a required interface;
- `service.composition.read` and `service.composition.control`;
- `mesh.packages` plus `service.packages.control` only if the first version
  includes installation; and
- no filesystem or scripting capability.

The minimum UI is deliberately narrow:

1. choose a root instance;
2. show the author-declared slots as columns/regions;
3. show effective placement nodes in order;
4. add a compatible palette entry, remove it, or move it within/between
   compatible slots;
5. edit a node's public props using the same generated controls as settings;
6. show validation/capability/health diagnostics before apply;
7. reset one slot to author/composition defaults; and
8. undo/redo locally by retaining prior whole-list documents until commit.

The editor may render navbar-specific spatial affordances when supplied by
generic slot metadata such as localized label and layout hint, but core never
branches on "navigation bar", "start", "center", or "end". Another editor
module can consume the same interface and replace it completely.

# Coder workflow and update behavior

The escape hatch is source, not a more complicated node type:

- edit the component's `.mesh` to change fixed structure or expose/remove a
  slot;
- edit `<props>` to change what the visual inspector may configure;
- edit Luau for behavior and CSS for presentation;
- add a public extension-point contribution to make a new component placeable;
  and
- edit the profile JSON directly when a textual declarative change is enough.

Hot reload recompiles slot descriptors and revalidates effective placements.
Author default changes affect users who have no override. Explicit user lists
remain stable. Renamed/removed slots or contributions become retained orphan
diagnostics, never heuristic remaps. Stable names and contribution ids are part
of the author's compatibility surface and should get LSP rename warnings.

# Implementation sequence

## Phase 1 — Specify the contract and data types

1. Amend spec 01 §4.3 and §5.2–5.4 with automatic/customizable slot modes,
   placement nodes, composition layering, activation closure, and
   `mesh.composition`.
2. Amend spec 03 with named customizable `<slot>` syntax and the source/node
   boundary.
3. Amend spec 08 with sparse `nodeSlots`, precedence, reset, and generated prop
   inspector reuse.
4. Add serde types for `NodeSlotOverride` and `ComponentPlacement`; bump profile
   schema to 3 and add the explicit migration diagnostic.

Checkpoint: schema round-trips, rejects unknown fields, and merge tests prove
whole-list precedence including explicit empty lists.

## Phase 2 — Compile and index slot descriptors

1. Extend `SlotNode` parsing with static `name` and `mode`.
2. Emit compiled slot descriptors containing entrypoint, local name, contract,
   mode, host metadata, and resolved defaults.
3. Validate host declarations/default contribution references in the installed
   graph and expose compatible contribution + prop-schema records.
4. Update the LSP syntax/manifest schemas, completion, hover, duplicate-name
   diagnostics, and stable-id rename warning.

Checkpoint: a fixture navbar exposes three descriptors and a palette containing
only compatible contributions, with no shell-specific Rust constants.

## Phase 3 — Resolve, mount, and reconcile placements

1. Layer source defaults → compositions → profile into an effective slot map.
2. Include selected contribution modules in the activation closure.
3. Branch slot rendering by mode and reuse alternate-root compilation/runtime
   isolation.
4. Key instances by placement id and reconcile add/remove/move/prop changes.
5. Retain/report orphans and bound individual failures.

Checkpoint: changing a profile fixture rearranges the real navbar; reorder
preserves Lua state, and one broken node does not affect its host or siblings.

## Phase 4 — Add transactional `mesh.composition`

1. Register the typed contract/provider and publish composition state.
2. Route apply/reset/prune through new core requests.
3. Validate and prepare a candidate before atomic profile write and visible
   commit; reject stale generations.
4. Publish generation/state/diagnostic changes after commits and graph reloads.

Checkpoint: a fixture frontend with the capability can rearrange a slot; one
without it is denied; malformed and stale writes leave disk and UI unchanged.

## Phase 5 — Build the replaceable editor module

1. Build the generic root/slot/palette canvas and generated prop inspector.
2. Add keyboard-accessible move controls alongside drag and drop; node editing
   must not require a pointer.
3. Add reset, validation preview, local undo/redo, health, and capability review.
4. Mount it through the normal composition/profile as a window surface.

Checkpoint: removing/replacing the editor module changes no core behavior and a
third-party fixture editor can perform the same transaction.

## Phase 6 — Navigation-bar proof and documentation

1. Extract placeable navbar items into stable extension-point contributions.
2. Declare `mesh.navigation.item` and the start/center/end customizable slots
   with current layout as defaults.
3. Add author and user guides showing visual placement versus source editing.
4. Record the shipped work in the monthly log, remove the backlog item, and
   update `STATUS.md` when all acceptance gates pass.

# Test and acceptance plan

Unit/fixture coverage:

- profile v3 parse/round-trip and schema-2 migration diagnostic;
- every merge layer, explicit empty list, stable ordering, and orphan retention;
- every diagnostic listed above;
- extension-point version/cardinality and public-prop validation;
- activation closure includes selected node modules but grants nothing new;
- optimistic generation conflict and atomic-write failure;
- capability allow/deny for read and control; and
- catalog delta invalidates only affected slots.

Runtime integration coverage:

- three navbar slots render their defaults;
- add/remove/reorder/move and prop update apply live;
- unchanged placement ids retain script state across reorder;
- reset adopts a changed author default;
- removed contribution produces one bounded placeholder and an orphan record;
- profile switch changes all node-slot lists transactionally; and
- automatic extension slots behave exactly as before.

Editor/accessibility coverage:

- the full operation set works by keyboard;
- slot and node semantics expose role, label, position, compatibility, and
  validation state;
- focus survives a successful reorder where the placement id survives; and
- destructive reset/prune is explicit and reports what will be removed.

Full gates: focused crate tests, relevant real-surface integration tests,
`cargo test --workspace` under `nix develop` compared with the recorded baseline,
`cargo fmt --all -- --check`, and `git diff --check`.

# Deliberately deferred

- arbitrary base-element nodes or general nested layout editing;
- expressions, event wiring, scripts, styles, and source generation;
- editing slots below a root component (requires a stable nested instance-path
  contract);
- free placement/canvas coordinates (the host owns layout);
- cross-slot moves between different contracts;
- collaboration/remote locking beyond optimistic generation checks; and
- automatic migration after author renames.

These can be proposed later without changing the v1 placement record's meaning.
None is required to prove the two-level customization model.

# Outcome

Implemented on 2026-08-08. Profile schema 3 now persists sparse placement
nodes for named customizable slots; the compiler, graph, catalog, and runtime
validate and mount public extension-point contributions with stable placement
identity. The generic capability-gated `mesh.composition` provider publishes
the palette and effective slots and accepts generation-checked apply/reset
transactions.

The shipped navigation bar exposes start, center, and end regions with its
previous layout as author defaults. `@mesh/composition-editor` is an ordinary
frontend window module mounted by the desk composition; it supports keyboard
add/remove/reorder/cross-region movement, reset, and generated controls for
public scalar props. Source remains the only customization level for elements,
Luau, handlers, CSS, and surface policy.

Focused module, parser/compiler, request-routing, shipped-module compilation,
and real navigation-surface tests pass; the module suite reached 198 passed and
3 ignored. The full shell library run reached 655
passed and 125 ignored with three unrelated existing debug/theme fixture
failures; no node-slot or navigation regression remained.
