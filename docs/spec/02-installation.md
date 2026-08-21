# 02 — Installation & Health

> Part of the [MESH Specification](README.md).

Installation lands a module on disk in a state the shell can load; health
makes every gap after that visible and fixable. Both read the **same
declarations** in `module.json` — there is no duplication between "what the
installer checks" and "what the runtime verifies".

## 1. Installer v1 — path + Git, editable source

**Status: shipped.** Local-path and Git installation, composition
installation, capability gates, kind-aware activation, profile commands,
validation, `mesh.lock` v3 (version, direct dependency requirements, source,
resolved revision, content digest, requesters, and active composition),
`update`/`rollback`/`uninstall`/`lock verify`, the interface
compatibility gate, and capability re-approval are available through
`mesh-shell` and the typed `mesh.packages` service. A CLI or package component
is a client of that service, not a privileged management layer.

v1 deliberately ships without a registry or package archives. Detached module
signing is supported through `module.sig` and root-graph trust anchors; the
design must not block registry distribution (§6), and the first installer is:

```
mesh install <path>              # copy a local module directory into the modules dir
mesh install <git-url>[#ref]     # clone a module repo into the modules dir
mesh update [<module-id>|--all]  # fetch candidates, gate, then commit (§1.3)
     [--dry-run] [--keep|--replace]
mesh rollback [<generation>]     # restore a previous lock generation
mesh lock verify                 # recompute digests; report local edits
mesh uninstall <module-id>       # refuses while something still requires it
mesh profile add <profile> <module-id>     # add a root or provider choice
mesh profile remove <profile> <module-id>  # unwire it from that profile
mesh list                        # installed modules, kinds, versions, health
mesh providers [<interface>]     # implementers + active provider; set with:
mesh providers <profile> <interface> <module-id>
mesh doctor                      # full health + dependency report (§5)
mesh new <kind> <name>           # scaffold a module from a kind template
```

Semantics:

- **Install = copy/clone + validate.** The module directory is placed under
  the active dotfiles modules directory, its manifest is validated (closed-core schema,
  dependency buckets, kind-scoped sections), and graph diagnostics run once.
  Nothing else is written inside the module. Installed source remains directly
  editable.
- **Profiles hold composition decisions.** Root components, provider choices,
  resources, and scoped overrides belong to named profiles
  ([01 §5.2](01-module-system.md)). The modules directory is available source,
  not a list of running units.
- **Updates preserve edits.** `mesh.lock` records a content digest of the
  installed tree, so *"has the user edited this?"* is decidable rather than
  assumed. An update never silently discards local changes: it refuses by
  default and requires `--keep` (pin it) or `--replace` (discard) explicitly.
- **Dependencies are reported, not fetched.** v1 does not resolve transitive
  module dependencies from a registry; it validates that `mesh.uses.modules`
  and `mesh.uses.interfaces` are satisfiable by what is installed and prints
  what is missing (with the module ids that need it). Fetching is a registry
  feature (§6).
- **System dependencies are detected, never installed.** Binaries
  (`mesh.uses.binaries`), native libraries, and fonts are the system package
  manager's job. The installer probes (`$PATH` lookup or explicit executable
  path, fontconfig) and prints per-distro hints from the manifest's
  `packages` map. This is a trust boundary: MESH does not run package
  managers.
- **Capability review.** Install prints the module's requested capabilities
  grouped by privilege level; `elevated` requires confirmation, `high`
  requires explicit opt-in ([01 §7](01-module-system.md)).

### 1.1 Activation after installation

Installation and activation are separate state transitions presented as one
kind-aware user action. A direct frontend install defaults to **install and
add**: it creates an active `#default` root in the selected profile and inherits
the module's declared placement and props without copying them into user
overrides. Dependency installs never create roots.

| Installed kind | Default composition action |
| --- | --- |
| `frontend` | Add/re-enable one active `#default` root instance. |
| `backend` | Select only when it is the sole provider required by the profile; a second provider stays available and inactive. |
| `component`, `interface`, `library` | Make available to dependency resolution; no independent enabled state. |
| `theme`, `icon-pack`, `font-pack`, `language-pack` | Make available; applying/reordering a resource is a separate explicit action. |

Modules requiring unapproved elevated/high capabilities are never activated.
The package service stages source, validates the candidate graph and capability
decision, commits source/lock/profile changes, and asks the profile service to
apply the candidate. Failure before the visible commit leaves the old profile
active; the downloaded module may remain installed and available with the
activation error reported.

### 1.2 Multi-provider installs

Installing a second provider for an interface is a feature, not a conflict:

```
$ mesh install ~/src/pulseaudio-audio
→ @mesh/pulseaudio-audio provides mesh.audio.
  mesh.audio is already provided by @mesh/pipewire-audio (active).
  @mesh/pulseaudio-audio is installed and inactive.
  Bind in a profile with:
  mesh providers desktop mesh.audio @mesh/pulseaudio-audio
```

The sole compatible implementer of an interface may be auto-selected; an
explicit profile binding is required where several modules implement one
interface.

### 1.3 The update transaction

**Status: shipped.**

An update is a transaction with a pre-commit refusal point, not a fetch. Every
step before the commit can refuse without touching the running shell:

1. **Resolve candidates.** For each git source, fetch the ref and read
   `git show <rev>:module.json` — the candidate version without a checkout. A
   revision is reproducible but has no update semantics; the version is what
   answers *"is there a compatible newer release"*.
2. **Build the candidate closure** (one version per module id, [01 §5.2](01-module-system.md)).
3. **Interface compatibility gate.** For every consumer, diff the
   `InterfaceContract` between locked and candidate. Because contracts are
   **data**, this runs without executing a line of module code:

   | Change | Class |
   | --- | --- |
   | added state field / method / event / optional trailing argument | additive |
   | removed or renamed state field, method, event, or payload field | breaking |
   | changed type of an existing field, argument, or return | breaking |
   | required argument added | breaking |
   | newly required consumer capability | breaking |

4. **Capability diff.** New `elevated`/`high` capabilities anywhere in the
   closure require explicit re-approval, exactly as at install.
5. **Local-edit gate.** Any module whose recomputed digest differs from the lock
   is reported and not overwritten.
6. **Commit**, in order: source, then the lock generation, then the profile
   switch. A **lock write failure fails the transaction** — the lock is the
   rollback record, so a composition whose lock did not land cannot be rolled
   back. This is why the earlier warn-and-continue behavior was removed.
7. **Rollback.** `mesh rollback` restores a previous lock generation and
   re-materializes its revisions. Trees are re-fetched by revision rather than
   archived: git is content-addressed, so the revision is an exact and cheap way
   back. The last ten generations are kept under
   `~/.local/state/mesh/lock-history/`.

`--dry-run` prints the resolution, normalized graph diff (module additions,
removals, updates, enable/disable changes, provider selection, and profile
layout/slot effects), contract diff, and capability diff without changing live
source, lock, or profile state. It is the primary review surface.

### 1.4 `mesh.lock`

**Status: shipped.**

```json
{
  "schemaVersion": 3,
  "generation": 7,
  "composition": { "module": "@alice/desk", "version": "2.1.0" },
  "modules": {
    "@mesh/navigation-bar": {
      "version": "3.1.0",
      "source": { "kind": "git", "url": "https://github.com/mesh/navigation-bar", "reference": "v3" },
      "revision": "abc123…",
      "digest": "sha256:…",
      "dependencies": { "@mesh/shell-ui": "^1.0.0" },
      "requestedBy": ["@alice/desk"]
    }
  }
}
```

`digest` hashes relative path, executable bit, and bytes of every source file in
sorted order, excluding `.git` and the detached `module.sig` sidecar. Compiled
output lives in `~/.cache/mesh` and never inside a module directory, so an
ordinary shell run cannot make a module read as edited. `requestedBy` is what
lets `uninstall` refuse safely. A v2 lock is upgraded in memory to v3 on load;
the next successful package transaction persists the migrated schema and direct
dependency metadata.

### 1.5 Immutable activation objects

**Status: shipped.** Each successful package transaction also publishes every
locked module into `.mesh-store/objects/sha256/<digest>` and records the module
set, versions, composition, and lock generation in an immutable activation
snapshot under `.mesh-store/activations/<generation>/`. The active generation is
advanced only after the matching lock bytes land. Graph and shell discovery use
the active object paths when a snapshot exists, while the installed module tree
remains the directly editable authoring source and is retained for update and
local-edit checks.

## 2. Directories

```
~/.config/mesh/modules/         directly editable installed module source
~/.config/mesh/profiles/        saved shell compositions
~/.config/mesh/active-profile   selected profile id
~/.config/mesh/mesh.lock        source provenance and resolved revisions
~/.config/mesh/overrides/       optional user-owned cross-module overrides
~/.local/state/mesh/            durable service state, logs, and health
~/.cache/mesh/                  compiled components and rebuildable indexes
```

The development workspace currently uses `config/` plus `modules/` in-repo;
the profile directory shape is target behavior. Default MESH modules are copied
or cloned into the same editable module tree as third-party modules. They are
preinstalled defaults, not privileged system units, and can be removed from a
profile or replaced.

## 3. Validation at install and load

The same installed-graph diagnostics run at install time and on every shell
start ([01 §9](01-module-system.md)). Severity policy:

| Finding | Effect |
| ------- | ------ |
| Invalid/legacy manifest, ambiguous manifest files | Module fails to load; replacement diagnostic |
| Missing required interface provider | Frontend loads; consuming UI sees interface health `unavailable` |
| Missing required binary | Module loads; health `unavailable` until present |
| Missing optional binary / optional icon / pack coverage gap | Health `degraded` or informational diagnostic |
| Capability misdeclaration, undeclared events, unknown shell channels | Non-fatal typed diagnostics with author actions |

Missing things degrade visibly; they do not block unrelated modules.

## 4. Development loop

```
mesh dev <path>        # load a module from a working directory with hot reload
```

**Status: partially shipped** — source/settings watching and reload exist in
the shell; the dedicated `dev` entry is target. Dev modules load at the
`local` trust tier. Hot reload preserves `self.storage`; UI state is
best-effort.

## 5. Health

**Status: target in this unified form** (diagnostics plumbing, binary
availability states, and the debug inspector exist; the three-state record and
periodic re-probe are the contract to implement against).

Every active module has a health state — a first-class runtime primitive frontends
can subscribe to, so a missing daemon becomes "Audio unavailable: install
playerctl", not a silently broken widget.

| State | Meaning |
| ----- | ------- |
| `healthy` | All required deps present, declared features available. |
| `degraded` | Running; one or more optional features unavailable. |
| `unavailable` | Cannot run: required dep missing, daemon down, or unrecoverable error. Loaded but inert. |

The health record carries structured context: `reason`, `fix_suggestion`,
`missing[]` (kind, name, version, per-distro `packages`), `degraded_features`,
`since`, `recoverable`. `reason`/`fix_suggestion` come from the manifest's
dependency declarations — authors write them once.

How health is set:

1. **Install-time probe** writes the initial record.
2. **Load-time probe** re-runs the same checks each shell start.
3. **Runtime reports** — `mesh.diagnostics.healthy() / degraded(reason) /
   unavailable(reason)`.
4. **Periodic re-check** for `recoverable` modules (default 30s when
   `unavailable`, 5m when `degraded`) — installing a missing binary revives
   the module without a shell restart.
5. **Propagation** — interface health = active provider health. A frontend
   consuming `mesh.audio` from an `unavailable` backend sees the *interface*
   as unavailable; "no provider" and "broken provider" are one case.

An unavailable selected backend does not silently switch the shell to another
provider. The active profile remains deterministic and exposes the failure;
the user, distribution, or an explicitly configured policy service may choose
another provider. Providers advertise supported optional contract features at
registration; unsupported calls raise `unsupported_operation` and report
`degraded` with the feature name.

Health flows on the normal event bus:

```
module.health/<module-id>
interface.health/<interface-name>
module.health                     # fan-out for the diagnostics UI
```

```luau
mesh.events.on("interface.health/mesh.audio", function(h)
  service_available = h.state ~= "unavailable"
  health_reason = h.reason or ""
end)
```

Optional interfaces use `pcall(require, …)`; a failed require and an
unavailable interface render the same fallback path.

## 6. Future: registry and archives; signing integration

**Status: registry and archive distribution are explicitly deferred; detached
signing is shipped without registry key distribution.**

- Module identity (`@scope/name` + semver) and the manifest's kinded
  dependency buckets are already registry-shaped; a registry adds *fetching*,
  not a new model.
- Signing attaches at the module-directory boundary in `module.sig`, a
  detached Ed25519 signature over the canonical module id/version/digest
  payload. Root-graph `trustPolicy.keys` provide the local trust anchors;
  unsigned sources retain `local`/`community` tier behavior.
- Registry integrity and registry key distribution can extend the same lock
  provenance without changing the editable module layout.
- Lock provenance now carries a typed `trust` tier and optional detached
  `signature` record. The root graph's optional `trustPolicy.minimum` is
  enforced before activation and candidate graph review reports blocked tiers;
  candidate planning and graph activation verify signatures before accepting a
  `verified` tier; registry key distribution remains pending.
- Update flows must re-show capability diffs and require re-approval when a
  new version adds `elevated`/`high` capabilities.
