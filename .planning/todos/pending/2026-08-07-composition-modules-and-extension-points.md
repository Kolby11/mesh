---
created: 2026-08-07
title: Composition modules, contract-named extension points, and a real lock
area: module-system
supersedes_partially:
  - .planning/todos/pending/2026-05-15-define-module-install-requirement-resolution.md
files:
  - crates/core/extension/module/src/package/module_manifest.rs
  - crates/core/extension/module/src/package/profile.rs
  - crates/core/extension/module/src/package/installed_graph/contributions.rs
  - crates/core/shell/src/shell/component/catalog.rs
  - crates/tools/cli/src/main.rs
  - docs/spec/01-module-system.md
  - docs/spec/02-installation.md
  - docs/spec/08-settings.md
---

## Problem

MESH can compose one shell, but it cannot **distribute, version, update, or
fork** a composition. Five concrete gaps:

1. **Extension points are keyed by module id.** `provides_slots` produces slot
   ids like `@mesh/settings:custom-settings`. Every contributor names the host
   module, so replacing `@mesh/settings` with `@alice/settings` silently drops
   every contributed page. This is the coupling `spec/01` §2 rule 3 forbids for
   backends, unfixed for UI.
2. **Slots can only be filled from Rust.** `module_manifest.rs:173` still sets
   `slot_contributions: HashMap::new()` — the canonical loader parses slot
   *definitions* but no module can declare a *contribution*. Consequently
   `catalog.rs` hardcodes `SETTINGS_HOST = "@mesh/settings"` and
   `SETTINGS_SLOT = "@mesh/settings:custom-settings"`. That is core policy in
   the wiring layer: the same class of bug as `if service == "audio"`.
3. **A profile is a config file, not a module.** It has no version, no
   dependencies, no manifest, no lock entry, no capability review, no health.
   A whole shell composition cannot be installed, published, updated, or forked.
4. **The lock is not a lock.** `mesh.lock` records git provenance only, keyed by
   module id, with **no module version, no content digest, no transitive
   closure**, and it is written *best-effort* — `persist_git_provenance`
   (`crates/tools/cli/src/main.rs:823`) downgrades a write failure to a warning.
   Rollback is impossible; "did the user edit this?" is undecidable.
5. **There is no update path at all.** No `mesh update`, no version resolution,
   no interface-compatibility gate, no capability re-approval.

## Outcome

```
mesh install https://github.com/alice/desk-shell#v2.1.0
  → resolves the recursive module closure, pins it, reviews capabilities,
    activates one composition

mesh update
  → fetches candidate revisions, diffs interface contracts as data,
    refuses breaking or unapproved changes before anything is committed

mesh rollback
  → restores the previous lock generation

# forking a shell family, without touching any upstream module:
{ "kind": "composition", "extends": "@alice/desk-shell",
  "compose": { "slots": { "mesh.settings.page": {
      "replace": { "@mesh/audio": "@me/my-audio-page" } } } } }
```

## Invariants (decide once, enforce everywhere)

- **I1 — Contracts, never module ids.** Every cross-module reference targets a
  named versioned contract: interfaces for services, *extension points* for UI.
  Module ids appear only in a composition's explicit bindings.
- **I2 — Three distinct edges.** `needs` (`mesh.uses`) ≠ `composes` (a
  composition's roots) ≠ `binds` (provider/slot choice). Never collapse them
  into containment.
- **I3 — Compositions bind; they never own.** A composition selects a provider;
  it does not contain it. Durable service data stays shared, configuration
  stays profile-scoped.
- **I4 — A composition holds no privilege.** It declares no capabilities and
  receives none. It can only *select among* what its members already declare.
- **I5 — One version per module id per closure.** Module id is also the settings
  namespace and the surface-instance key; two copies would fork the settings
  store. This is already physically true (`modules/<id>/` is one directory) —
  now it becomes a stated resolution rule.
- **I6 — Precedence is one ladder, everywhere.** base composition → derived
  composition → module default → user profile. Same shape as
  `spec/08` value precedence.
- **I7 — Nothing is committed before it validates.** Staging, contract diffs,
  and capability review all happen against a candidate; the visible commit is
  last and atomic.

---

# Stage 1 — Extension points as contracts

*Closes gaps 1 and 2. Leaves the tree working; deletes the hardcoding.*

## 1.1 Declaration

An extension point is data, like an interface, and lives in an `interface`
module. New `mesh.extensionPoints` map:

```json
{
  "name": "@mesh/shell-ui-interface",
  "version": "1.0.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "interface",
    "extensionPoints": {
      "mesh.settings.page": {
        "version": "1.0",
        "description": "A page mounted into a settings frontend",
        "multiple": true,
        "props": [
          { "name": "namespace", "type": "string" },
          { "name": "title", "type": "string" },
          { "name": "icon", "type": "string?" }
        ]
      }
    }
  }
}
```

`props` reuses the existing interface type grammar and its validator, so
contributor props are typechecked at graph build with the code that already
exists. `multiple: false` means at most one contribution wins (highest
precedence); `> 1` contribution to a single-valued point is
`extension_point_overfilled`.

## 1.2 Hosting

```json
"mesh": { "hosts": { "mesh.settings.page": ">=1.0" } }
```

In markup the slot names the contract, not itself:

```html
<slot extension-point="mesh.settings.page" />
```

Hosting is a **trust decision** — a host renders foreign UI inside its own
surface — so it is explicit, versioned, and visible in the manifest.

## 1.3 Contributing

```json
"mesh": {
  "provides": {
    "extensionPoints": {
      "mesh.settings.page": [
        {
          "id": "audio",
          "entry": "src/settings.mesh",
          "order": 100,
          "props": { "title": { "t": "audio.settings.title", "fallback": "Audio" } }
        }
      ]
    }
  }
}
```

`mesh.provides.*` is already the open contribution namespace (`spec/01` §3.1),
so this needs no new namespace concept.

## 1.4 Resolution

`installed_graph/contributions.rs` gains a `ResolvedExtensionPoint` index:

```
extension_point_name → [ ResolvedContribution {
    source_module_id, contribution_id, entry_path, order, props, precedence
} ]
```

Matching rule: a contribution resolves into **every** enabled host declaring a
compatible version of that point. Two settings frontends installed → both get
the pages. Correct, and no special case.

Deterministic order key, in this order:
`(composition explicit index, declared order, source_module_id, contribution_id)`.

## 1.5 Runtime

The recently landed generalization in `runtime.rs`
(`render_embedded_compiled_instance`) is exactly the right seam and is kept.
Each contribution compiles as an alternate root and gets **its own VM, its own
capabilities, and its own settings namespace** — it is the contributing module
running inside the host's tree, not the host's code.

Then delete, from `catalog.rs`:

- `const SETTINGS_HOST` / `const SETTINGS_SLOT` and the whole synthetic block;
- the `settings_ui`-specific `HashMap` (replaced by a generic
  `extension_point_entries: HashMap<(module_id, point, contribution_id), Shared…>`);
- the `settings_ui` fallback in `resolve_module_component_alias`.

Compilation reuse across graph-only rebuilds (the `manifest_fingerprint` +
`source_fingerprint` check just added) carries over unchanged — it is generic
already.

## 1.6 Delete the module-keyed slot machinery

`SlotDefinition`, `SlotContribution`, `provides_slots`, and
`slot_contributions` in `manifest/model.rs` and `module_manifest.rs` are
**removed outright**, not aliased (`feedback_no_backward_compat`). The
`@mesh/settings` manifest loses `providesSlots` and gains `hosts`.

## 1.7 Diagnostics

| id | condition |
| --- | --- |
| `unknown_extension_point` | contributes/hosts a point no installed interface declares |
| `extension_point_version_mismatch` | host range excludes the declared version |
| `invalid_extension_point_props` | contributor props fail the declared type grammar |
| `extension_point_overfilled` | >1 contribution to `multiple: false` |
| `unhosted_contribution` | contribution resolves to zero enabled hosts (informational — a valid state when the host is not composed) |

## 1.8 Stage 1 checkpoint

`@mesh/navigation-bar`'s settings page renders inside `@mesh/settings` with
**zero module ids in Rust**. Renaming the settings module in a fixture keeps
every contributed page working. That test is the proof Stage 1 landed.

---

# Stage 2 — Composition modules

*Closes gap 3.*

## 2.1 The kind

`ModuleKind::Composition`. Its manifest carries `mesh.compose`, whose value is
**structurally the current `ShellProfile`**, plus `slots`:

```json
{
  "name": "@alice/desk",
  "version": "2.1.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "composition",
    "extends": "@mesh/default-desk",
    "uses": {
      "modules": { "@mesh/navigation-bar": "^3.0.0", "@mesh/settings": "^1.2.0" },
      "sources": {
        "@mesh/navigation-bar": { "git": "https://github.com/mesh/navigation-bar", "ref": "v3" }
      }
    },
    "compose": {
      "roots": {
        "@mesh/navigation-bar#top": { "module": "@mesh/navigation-bar", "entrypoint": "main",
                                      "surface": { "anchor": "top" } }
      },
      "backgroundServices": [],
      "providers": { "mesh.audio": "@mesh/pipewire-audio" },
      "resources": { "theme": "@alice/desk-theme", "icons": ["@mesh/icons-default"] },
      "slots": {
        "mesh.settings.page": {
          "replace":  { "@mesh/audio": "@alice/desk-audio-page" },
          "suppress": ["@mesh/navigation-bar"],
          "order":    ["@alice/desk-audio-page", "@mesh/network"]
        }
      },
      "settings": { "shell": { "i18n": { "locale": "en-US" } } }
    }
  }
}
```

Validation, per **I4**: `mesh.uses.capabilities` on a composition is a hard
error (`composition_declares_capability`). A composition has no `entry`, no
`surface`, no `implements`, and contributes no extension points.

## 2.2 A profile becomes an instance of a composition

```json
{
  "schemaVersion": 2,
  "from": { "module": "@alice/desk", "version": "2.1.0" },
  "roots":    { "@mesh/navigation-bar#top": { "active": false } },
  "settings": { "shell": { "i18n": { "locale": "sk-SK" } } }
}
```

The profile stores **only the user's deltas**. `from` is optional: a profile
without it is a hand-built composition, exactly today's behavior, which is how
this stage lands without breaking the existing tree.

## 2.3 The merge

One function, `resolve_composition(...) -> EffectiveComposition`, in a new
`package/composition.rs`:

```
extends chain (base → derived, cycle-rejected)
  ⊕ composition.compose
  ⊕ profile deltas
  = EffectiveComposition
```

Merge semantics per field:

| Field | Rule |
| --- | --- |
| `roots` | keyed union; later layer's fields override per-field (`surface` is sparse-merged, matching today's per-instance override) |
| `providers`, `resources.theme` | scalar override |
| `resources.icons/fonts/languages` | later layer **replaces** the chain (ordered chains are coherence-sensitive; merging two orderings is meaningless) |
| `backgroundServices` | set union |
| `settings` | deep sparse merge, existing `spec/08` semantics |
| `slots` | `suppress` unions, `replace` overrides per key, `order` replaces |

**Orphan policy** (a real gap, must be explicit): a user delta keyed to a root
the composition no longer declares is **retained and reported**
(`orphaned_profile_override`), never silently dropped — dropping it loses user
work on every upstream rename. `mesh profile prune` clears them on request.

Deactivation vs deletion: a user may set `active: false` on an inherited root
but cannot delete it — deletion is not expressible in a delta layer, and an
update would resurrect it anyway.

## 2.4 Where the code goes

`ShellProfile::active_module_ids` (`profile.rs:198`) already computes the
activation closure — roots + services + providers + resources, then declared
deps, interface modules, and sole providers. That function moves to operate on
`EffectiveComposition` unchanged. `apply_to_root` likewise. The shell's
transactional switch (`shell/profile.rs:244 apply_switch_profile`) needs no
change: it already takes a candidate and commits last.

## 2.5 Composition health

A composition is `unavailable` when a declared root/provider/resource module is
missing or fails to load, `degraded` when an optional one does, `healthy`
otherwise — reusing the `spec/02` §5 record verbatim. `mesh doctor` prints the
composition's tree with per-node health.

## 2.6 Stage 2 checkpoint

`mesh install ./fixtures/desk-composition && mesh profile use desk` produces the
same running shell as the hand-written profile it replaces, and
`extends`-forking it swaps one settings page without editing any upstream
module.

---

# Stage 3 — Resolution and a real lock

*Closes gaps 4 and 5 (resolution half).*

## 3.1 Version resolution

Given the composition's `uses.modules` ranges plus every member's transitive
ranges: pick, per module id, the **highest version satisfying all ranges**
(**I5**). No satisfying version → `module_version_conflict`, naming every
requirer and its range. There is deliberately no multi-version fallback: it
would fork the settings namespace and the surface instance key.

## 3.2 Sources

Without a registry, ranges need a source map. `mesh.uses.sources` maps a module
id to `{ git, ref }` or `{ path }`. Resolution order: profile-local override →
composition `sources` → already-installed copy. A registry later populates the
same map from an index — the model does not change.

## 3.3 Lock v2

Promoted out of the CLI into `package/lock.rs` as a library type (it is graph
state, not a CLI concern):

```json
{
  "schemaVersion": 2,
  "generation": 7,
  "composition": { "module": "@alice/desk", "version": "2.1.0" },
  "modules": {
    "@mesh/navigation-bar": {
      "version": "3.1.0",
      "source": { "git": "https://github.com/mesh/navigation-bar", "ref": "v3" },
      "revision": "abc123…",
      "digest": "sha256:…",
      "requestedBy": ["@alice/desk"]
    }
  }
}
```

Three additions carry all the weight:

- **`digest`** — content hash over the installed tree (sorted relative paths,
  file mode + content, `.git` excluded). Recomputing it answers *"has the user
  edited this module?"*, which is the predicate that makes
  update-vs-clobber decidable. `spec/02` §1 promises "updates preserve edits"
  and today has no way to detect one. **This is the single highest-value item in
  the plan.**
- **`version`** — a git SHA gives reproducibility but no update semantics; you
  cannot ask "is there a compatible newer release" from a commit.
- **`requestedBy`** — makes `mesh uninstall` safe (refuse while something still
  requires it) and explains conflicts.

**Lock writes become transactional.** `persist_git_provenance`'s
warn-and-continue is right for a one-off install and wrong here: for a
composition the lock *is* the rollback record. Write via the existing
`atomic_write` (`profile.rs:485`), and fail the transaction on error.

Previous generations are kept in `~/.local/state/mesh/lock-history/`
(last 10).

---

# Stage 4 — Update, compatibility gate, rollback

*Closes gap 5.*

## 4.1 The transaction

`mesh update [<module-id>|--all]`:

1. **Resolve candidates.** Per git source: fetch the ref, take its revision,
   and read `git show <rev>:module.json` — the candidate version without a
   checkout.
2. **Build the candidate closure** recursively (Stage 3 resolution).
3. **Interface-compatibility gate.** For every consumer in the closure, diff the
   `InterfaceContract` it consumes between locked and candidate:

   | Change | Class |
   | --- | --- |
   | added state field / method / event / optional arg | compatible |
   | removed or renamed state field, method, or event | breaking |
   | changed type of an existing field/arg/return | breaking |
   | required arg added | breaking |

   Because contracts are data, this runs **without executing a line of module
   code** — a genuine strength of the existing design. A breaking change with no
   satisfying alternative refuses the update and names the consumer that breaks.
4. **Capability diff.** New `elevated`/`high` capabilities anywhere in the
   closure require re-approval (`spec/02` §6 requires this; nothing implements
   it). For a composition, the review is the **union over its closure**, shown
   as a diff against the locked closure.
5. **Local-edit gate.** Any module whose recomputed digest ≠ locked digest is
   reported and **not overwritten**. Resolutions, all explicit:
   `--merge` (git merge upstream into the working tree, git sources only),
   `--keep` (pin it, exclude from this update),
   `--replace` (discard local edits, requires confirmation).
6. **Stage.** Clone/checkout into `~/.cache/mesh/staging/<txn>/`, run the full
   installed-graph validation on the candidate — the *same* diagnostics the
   shell runs at startup (`spec/02` §3), not a parallel checker.
7. **Commit**, in this order: move staged trees into place → write lock
   generation `n+1` atomically → ask the running shell to switch to the
   candidate via the existing transactional profile switch. Failure before the
   shell switch leaves the previous lock generation authoritative.
8. **Rollback.** `mesh rollback [<generation>]` restores the previous lock and
   re-materializes those revisions. Trees are re-fetched by revision (git is
   content-addressed, so this is cheap and exact); the immediately previous
   generation is additionally cached under `~/.local/state/mesh/rollback/` so
   one step back works offline.

## 4.2 CLI surface

```
mesh update [<module-id>|--all] [--merge|--keep|--replace] [--dry-run]
mesh rollback [<generation>]
mesh uninstall <module-id>          # refuses while requestedBy is non-empty
mesh lock verify                    # recompute digests, report local edits
mesh doctor                         # + composition tree, closure, lock drift
```

`--dry-run` prints the resolution, the contract diff, and the capability diff
without staging. It is the primary review surface and should land with the
first cut, not after.

---

# Stage 5 — Settings UI ownership

*Resolves the `settings_ui` question; depends on Stages 1 and 2.*

Three tiers, one ladder (**I6**):

```
composition slot override  >  module-provided page  >  generated-from-<props> fallback
```

- The **generated fallback** (shipped, valuable) is kept — without it a
  third-party module has zero settings UI until someone writes an adapter,
  which is worse centralization than the hardcoding being removed.
- A **module-provided page** is an ordinary `mesh.settings.page` contribution.
  It is the module's opinion, not a privilege.
- A **composition** may `replace`, `suppress`, and `order` pages, so a shell
  family restyles or replaces `@mesh/audio`'s page without touching the audio
  module.

The uniformity trick: the generated fallback is itself a contribution, whose
component is the settings module's own `generated-page.mesh` taking a
`namespace` prop. Core emits one homogeneous list of
`(component-ref, props)`; the host renders `<slot extension-point=…/>` with no
branch, and no "generated vs custom" concept exists below the settings module.

`mesh.entrypoints.settings_ui` is **deleted** — it is the module-keyed spelling
of a contribution that now has a contract-keyed one. Touches
`module_manifest.rs:638`, `contributions.rs:115/188/197/369`,
`ContributedSettingsSchema.settings_ui`, and the navigation-bar manifest.

## Dependency

Stage 5's endpoint (a *replaceable* settings frontend) also needs the two open
backlog items — `mesh.settings` as a service, and typed profile/package
services — because a settings module that reads profile JSON directly is still
privileged. Sequence them after Stage 2 (which defines what the package service
manages) and before declaring Stage 5 done.

---

# Gap ledger

| # | Gap | Closed by |
| --- | --- | --- |
| 1 | Slots keyed by module id | 1.1–1.4 |
| 2 | Contributions unwired; `SETTINGS_HOST` hardcoded | 1.3, 1.6 |
| 3 | Compositions not installable/versionable | 2.1 |
| 4 | Profile has no provenance | 2.2 |
| 5 | No composition forking | 2.1 `extends`, 2.3 |
| 6 | Lock has no version/digest/closure | 3.3 |
| 7 | Lock written best-effort | 3.3 |
| 8 | No transitive resolution | 3.1 |
| 9 | Diamond/version conflicts undefined | 3.1, I5 |
| 10 | No update command | 4.1 |
| 11 | No interface-compat check | 4.1 step 3 |
| 12 | No capability re-approval | 4.1 step 4 |
| 13 | Local edits undetectable | 3.3 digest, 4.1 step 5 |
| 14 | No rollback | 4.1 step 8 |
| 15 | `uninstall` unsafe | 3.3 `requestedBy` |
| 16 | Composition privilege creep | I4, 2.1 validation |
| 17 | Orphaned user overrides on update | 2.3 orphan policy |
| 18 | Settings UI ownership | Stage 5 |
| 19 | Composition health invisible | 2.5 |

**Deliberately out of scope:** registry and package archives (`spec/02` §6 —
Stage 3's source map is registry-shaped, so a registry adds fetching, not a
model); signing (attaches at the same tree boundary the digest already hashes);
multi-version coexistence (rejected by I5).

# Risks

- **Extension points become an uncontrolled plugin surface.** Mitigation:
  hosting is explicit and versioned; props are typechecked; a contribution runs
  in its own VM with its own capabilities, so a bad page cannot exceed its
  module's grants. The bounded error placeholder (`spec/01` §8) already prevents
  a broken contribution from blanking its host.
- **Composition becomes a god-object.** Mitigation: I4 plus the
  `composition_declares_capability` hard error. A composition can only select.
- **Contract diffing produces false "breaking".** Mitigation: `--dry-run`
  prints the diff; `--force` exists but re-shows it and requires confirmation.
- **Digest churn from generated files.** Mitigation: hash the source tree only;
  `~/.cache/mesh/` compiled output is never inside a module directory.

# Test plan

Per stage, in the crate that owns the behavior:

- **1** — resolution unit tests in `installed_graph`; each diagnostic in 1.7 has
  a fixture; the rename test in 1.8 is the gate. Existing catalog reuse tests
  extend to extension-point entries.
- **2** — merge-precedence table tests in `package/composition.rs`, one per row
  of 2.3; `extends` cycle rejection; orphan retention; an integration test that
  a composition-backed profile and the equivalent hand-written profile produce
  identical `EffectiveComposition`.
- **3** — resolution over a diamond fixture; conflict diagnostic content;
  digest stability across a no-op reinstall and change on a one-byte edit;
  lock round-trip.
- **4** — a local bare-git fixture repo with two tagged revisions covering:
  compatible update, breaking-contract refusal, new-`high`-capability refusal,
  local-edit refusal, and rollback to generation n−1.
- **5** — precedence ladder (composition replace > module page > generated) with
  one fixture module per tier.

Full-tree gate each stage: `cargo test --workspace` under `nix develop`. The
pre-existing red baseline (`.planning/log/`) applies — compare against it rather
than expecting green.

# Sequence

Stages are ordered by dependency and each leaves a working tree:

1. **Stage 1** — independent; also un-breaks what is currently uncommitted.
2. **Stage 3.3 (lock v2 + digest)** — independent of 1; can run in parallel.
3. **Stage 2** — needs 1 (for `slots`) and 3.3 (for `from` provenance).
4. **Stage 3.1–3.2** — needs 2.
5. **Stage 4** — needs 3.
6. **Stage 5** — needs 1 and 2, plus the two settings/package service backlog
   items.

Spec amendments land with their stage, not at the end: `01` §3.2 (kind), §4
(extension points), §5.2 (profile = composition + deltas); `02` §1 (lock,
update, rollback), §5 (composition health); `08` §5 (precedence ladder).
The LSP manifest schema (`crates/tools/lsp/src/manifest/schema.rs`) must be
updated in the same commit as each manifest change.
