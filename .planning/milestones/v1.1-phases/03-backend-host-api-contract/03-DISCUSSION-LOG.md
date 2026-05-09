# Phase 3: Backend Host API Contract - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-05-03
**Phase:** 3-Backend Host API Contract
**Areas discussed:** mesh.exec contract, mesh.exec failure shape, mesh.exec_shell scope, mesh.config contract, mesh.log levels, mesh.log call style, poll interval bounds, poll interval timing

---

## mesh.exec Contract

| Option | Description | Selected |
|--------|-------------|----------|
| Structured-only public contract | `mesh.exec(program, args)` is documented; legacy string splitting may remain only as compatibility. | |
| Keep both forms public | Plugin authors can use structured args or a single command string. | |
| Strict structured-only | Remove or reject the string-splitting form now. | ✓ |

**User's choice:** Strict structured-only.
**Notes:** The user later clarified that shell parsing can be done in Luau, so dynamic command values should be passed as structured args.

---

## mesh.exec Failure Shape

| Option | Description | Selected |
|--------|-------------|----------|
| Always return result table | `{ success=false, stdout="", stderr="...", code=nil }`; scripts branch on `success`. | ✓ |
| Throw Luau error on spawn failure | Missing command or OS spawn failure raises; non-zero exit still returns a table. | |
| Throw on any failure | Spawn failure and non-zero exit both raise Luau errors. | |

**User's choice:** Always return result table.
**Notes:** Spawn failures and non-zero exits should be handled by plugin scripts through `result.success`.

---

## mesh.exec_shell Scope

| Option | Description | Selected |
|--------|-------------|----------|
| Keep but discourage | Keep `mesh.exec_shell(cmd)` for pipelines and legacy providers; docs prefer `mesh.exec`. | |
| Compatibility-only | Keep existing providers working but do not document it as public MVP API. | |
| Remove from MVP | Migrate providers away from shell strings and delete or hide `mesh.exec_shell`. | ✓ |

**User's choice:** Remove from MVP.
**Notes:** User asked for the difference between `mesh.exec` and `mesh.exec_shell`, then chose to keep only `mesh.exec`; parsing can happen in Luau.

---

## mesh.config Contract

| Option | Description | Selected |
|--------|-------------|----------|
| Whole config table only | `local cfg = mesh.config()` returns the plugin settings table; no key lookup helpers yet. | ✓ |
| Table plus key lookup | Support `mesh.config()` and `mesh.config.get("path.to.key")`. | |
| Namespace object | Use `mesh.config.get_all()` and `mesh.config.get(...)`, avoiding callable `mesh.config()`. | |

**User's choice:** Whole config table only.
**Notes:** No config helper APIs in Phase 3.

---

## mesh.log Levels

| Option | Description | Selected |
|--------|-------------|----------|
| Fixed four levels | `debug`, `info`, `warn`, `error`; unknown levels warn rather than throw. | ✓ |
| Only info/warn/error | No debug in the MVP public API. | |
| Strict levels | Invalid level raises a Luau error. | |

**User's choice:** Fixed four levels.
**Notes:** Invalid levels should be visible but non-fatal.

---

## mesh.log Call Style

| Option | Description | Selected |
|--------|-------------|----------|
| Support both | Named methods plus generic dynamic level call. | ✓ |
| Named methods only | `mesh.log.info/warn/error/debug`; no callable `mesh.log(level, msg)`. | |
| Generic only | `mesh.log(level, msg)`; no level methods. | |

**User's choice:** Support both.
**Notes:** Public API includes both `mesh.log("info", "msg")` and `mesh.log.info("msg")`.

---

## Poll Interval Bounds

| Option | Description | Selected |
|--------|-------------|----------|
| Clamp to sane bounds | Values below 50ms become 50ms; optionally cap very large values if needed. | ✓ |
| Strict reject invalid/too-low values | Bad values raise or return failure. | |
| Accept as-is | Plugin controls interval exactly, even very low values. | |

**User's choice:** Clamp to sane bounds, but raise a warning.
**Notes:** The warning is important so plugin authors can see the correction.

---

## Poll Interval Timing

| Option | Description | Selected |
|--------|-------------|----------|
| After the current callback | No interruption mid-callback; the next poll uses the new interval. | ✓ |
| Immediately reset timer | Changing the interval restarts the timer right away. | |
| Only after next poll | Command-handler changes wait until the following poll cycle. | |

**User's choice:** After the current callback.
**Notes:** Applies to `init()`, `on_poll()`, and command handlers.

---

## the agent's Discretion

- Choose the exact implementation path for migrating existing bundled providers off `mesh.exec_shell`.
- Choose exact warning wording and whether warnings are tracing-only or also diagnostics-backed.
- Choose whether malformed `mesh.exec` argument types are Luau argument errors or structured failure tables, as long as process failures return tables.

## Deferred Ideas

- Config lookup helper APIs.
- Public shell-pipeline API.
- Process execution sandboxing/allowlists.
- Service command response propagation.
- Backend reference plugin/docs.
