# 02 — Installation & Health

> Part of the [MESH Specification](README.md).

Installation lands a module on disk in a state the shell can load; health
makes every gap after that visible and fixable. Both read the **same
declarations** in `module.json` — there is no duplication between "what the
installer checks" and "what the runtime verifies".

## 1. Installer v1 — path + Git, editable source

**Status: target** (the CLI is currently a shell launcher and inspection
adapter). Installation behavior is exposed through a package service; a CLI or
package component is a replaceable client of that service, not a privileged
management layer.

v1 deliberately ships without a registry, package archives, or signing. The
design must not block them (§6), but the first installer is:

```
mesh install <path>              # copy a local module directory into the modules dir
mesh install <git-url>[#ref]     # clone a module repo into the modules dir
mesh uninstall <module-id>
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
- **Updates preserve edits.** `mesh.lock` records source provenance and the
  resolved revision. An update never silently discards local changes; it must
  keep the local tree, merge/rebase explicitly, or replace only after the user
  chooses that action.
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

### 1.1 Multi-provider installs

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

## 6. Future: registry, archives, signing

**Status: explicitly deferred; the v1 design keeps the door open.**

- Module identity (`@scope/name` + semver) and the manifest's kinded
  dependency buckets are already registry-shaped; a registry adds *fetching*,
  not a new model.
- Signing attaches at the module-directory boundary (a detached signature
  over the tree); trust tiers ([01 §7](01-module-system.md)) already define
  the policy that signatures will enforce. Unsigned = `local`/`community`
  tier behavior.
- The existing source-provenance lock can grow registry integrity and signature
  metadata without changing the editable module layout.
- Update flows must re-show capability diffs and require re-approval when a
  new version adds `elevated`/`high` capabilities.
