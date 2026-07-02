# 02 — Installation & Health

> Part of the [MESH Specification](README.md).

Installation lands a module on disk in a state the shell can load; health
makes every gap after that visible and fixable. Both read the **same
declarations** in `module.json` — there is no duplication between "what the
installer checks" and "what the runtime verifies".

## 1. Installer v1 — path + git, decisions-only

**Status: target** (the CLI is currently a bare shell launcher).

v1 deliberately ships without a registry, package archives, or signing. The
design must not block them (§6), but the first installer is:

```
mesh install <path>              # copy a local module directory into the modules dir
mesh install <git-url>[#ref]     # clone a module repo into the modules dir
mesh uninstall <module-id>
mesh enable <module-id>          # remove from root-graph "disabled"
mesh disable <module-id>         # add to root-graph "disabled"
mesh list                        # installed modules, kinds, versions, health
mesh providers [<interface>]     # implementers + active provider; set with:
mesh providers <interface> <module-id>
mesh doctor                      # full health + dependency report (§5)
mesh new <kind> <name>           # scaffold a module from a kind template
```

Semantics:

- **Install = copy/clone + validate.** The module directory is placed under
  the user modules dir, its manifest is validated (closed-core schema,
  dependency buckets, kind-scoped sections), and graph diagnostics run once.
  Nothing else is written inside the module.
- **All state = root-graph edits.** Enabled/disabled, provider selection, and
  layout entrypoints are edits to `config/module.json`
  ([01 §5](01-module-system.md)). There is no lockfile in v1 — the modules
  dir *is* the state, and the root graph holds the decisions.
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
  Switch with: mesh providers mesh.audio @mesh/pulseaudio-audio
```

The sole implementer of an interface is auto-selected; an explicit `providers`
entry exists only where several modules implement one interface.

## 2. Directories

```
/usr/share/mesh/modules/        system-installed modules (distro / core)
~/.local/share/mesh/modules/    user-installed modules (installer target)
~/.local/share/mesh/user-icons/ auto-managed user icon-pack module (05 §6)
~/.config/mesh/module.json      root graph (decisions)
~/.config/mesh/settings.json    the single sparse settings store (08)
~/.config/mesh/themes/          user-authored theme packs (04)
```

The development workspace uses `config/` + `modules/` in-repo with the same
shapes. User modules override system modules with the same id; system modules
cannot be uninstalled, only disabled or shadowed.

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

Every module has a health state — a first-class runtime primitive frontends
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

An `unavailable` backend **does not register** its implementation; the
next-priority provider (if any) takes over. Providers advertise supported
optional contract features at registration; unsupported calls raise
`unsupported_operation` and report `degraded` with the feature name.

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
- A lockfile becomes meaningful only with registry fetching; v1's "modules
  dir is the state" rule is forward-compatible with adding one.
- Update flows must re-show capability diffs and require re-approval when a
  new version adds `elevated`/`high` capabilities.
