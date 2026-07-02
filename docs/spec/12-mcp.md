# 12 — MCP for LLMs

> Part of the [MESH Specification](README.md).

`mesh-mcp` is a **thin, separate binary** that speaks the Model Context
Protocol on stdio and translates to the automation IPC
([11](11-automation-ipc.md)). It contains no shell logic: no tree walking,
no privileged access, no state of its own. If the IPC can't do it, MCP
can't either — by construction.

**Status: target.**

## 1. Why a separate binary

- The shell process never links an MCP/LLM dependency; protocol churn stays
  out of the core.
- `mesh-mcp` authenticates like any automation client (its own client name,
  its own read/act grants) — the user consents to *the AI client*
  specifically, through the same trusted-UI prompt.
- Any MCP host (Claude Code/Desktop, editors, agent runtimes) configures it
  as a stdio server:

```json
{ "mcpServers": { "mesh": { "command": "mesh-mcp" } } }
```

Flags: `--read-only` (never requests `automation.act`), `--client-name`
(distinct grants per agent).

## 2. Tool surface

Deliberately small and semantic. Read tools (need `automation.read`):

| Tool | Maps to | Purpose |
| ---- | ------- | ------- |
| `mesh_tree` | `tree.snapshot` | Semantic tree of one surface or all; the LLM's "eyes" |
| `mesh_find` | `tree.find` | Locate nodes by role/name/surface query |
| `mesh_surfaces` | `surfaces.list` | What exists, where, visible or not |
| `mesh_settings_get` | `settings.get` | Effective value + supplying layer |
| `mesh_modules` | `modules.graph` | Installed graph: interfaces, providers, health, diagnostics |

Act tools (need `automation.act`):

| Tool | Maps to |
| ---- | ------- |
| `mesh_click` | `element.click` |
| `mesh_focus` | `element.focus` |
| `mesh_set_value` | `element.set_value` |
| `mesh_scroll` | `element.scroll_into_view` / `scroll_to` |
| `mesh_surface` | `surface.show/hide/toggle` |
| `mesh_settings_set` | `settings.set` / `unset` |

Tool results return structured JSON (the IPC payloads) plus a compact
human-readable rendering of trees (role/name/state outline) — LLMs act on
names and roles, not pixels. Stale-node errors are surfaced with a "re-run
`mesh_tree`" hint so agents self-correct.

## 3. Semantics over screenshots

The design bet ([09 §7](09-accessibility.md)): an accessible shell is an
AI-legible shell. The MCP server exposes no screen capture in v1; an agent
that can read roles, names, values, and states — and act through the same
semantic actions users' keyboards trigger — is more reliable than one
guessing at pixels, and the permission story stays clean.

## 4. Safety posture

- Read vs act granted separately; `--read-only` for observation agents.
- Every action carries the client name into shell diagnostics — "what did
  the agent do" is answerable from the debug panel.
- No raw input synthesis, no arbitrary channel publishes, no settings-file
  writes outside validation — inherited from the IPC's non-goals
  ([11 §6](11-automation-ipc.md)).
- The consent prompt is core-owned trusted UI; a module (or an agent) cannot
  fake or overlay it.
