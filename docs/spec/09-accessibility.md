# 09 — Accessibility

> Part of the [MESH Specification](README.md).

Accessibility is a **by-design property of the element model**, not an
opt-in layer. Every rendered node carries semantic metadata; the same
semantic tree serves assistive technology (via AccessKit), keyboard
navigation ([10](10-keyboard.md)), and machine consumers — automation and
LLMs ([11](11-automation-ipc.md)). One tree, three audiences.

## 1. The semantic tree

**Status: shipped** (roles, names, states, AccessKit mapping for shipped
element families); tree export over IPC is **target**.

Each `WidgetNode` contributes a semantic node with:

| Field | Source |
| ----- | ------ |
| **role** | Element kind (`button`, `slider`, `checkbox`, `menu`, `menuitem`, …) or explicit override |
| **name** | Visible text → `label` → `aria-label` (first available) |
| **description** | `title` / `aria-description` |
| **value** | Control value metadata (slider value/min/max, input text, progress) |
| **state** | checked / selected / expanded / disabled / focused / pressed |
| **focus metadata** | focusable, tab position, keyboard shortcut metadata from resolved keybinds |
| **relationships** | parent/child order, tooltip ownership, popover trigger ↔ surface |

Core elements come with correct default semantics — a `button` is focusable
with role `button` without author work. Authors add meaning, not plumbing.

## 2. Authoring contract

```xml
<button
  title="Open audio controls"          <!-- tooltip + AT description -->
  aria-label="Open audio controls"     <!-- AT name when visible text isn't it -->
  aria-pressed="{isMuted}"             <!-- state, reactive -->
  onclick={onVolumeClick}
>
  <icon name="audio-volume-high" aria-hidden="true"/>
  <text class="volume-value">{volume_label}</text>
</button>
```

Rules:

- Interactive controls must have an accessible name: visible text, `label`,
  or `aria-label`. Prefer `aria-label` for screen-reader text and `title`
  for visible tooltip text when both are needed.
- `aria-hidden="true"` removes decorative nodes (icon glyphs duplicating a
  label) from the semantic tree.
- State attributes (`aria-pressed`, `checked`, `selected`, `expanded`) bind
  reactively like any attribute.
- All author-facing text is localizable ([07](07-i18n.md)) — accessible
  names go through the same catalogs as visible strings.
- Surface roots declare `mesh.accessibility: { role, label }` in the
  manifest; enabled frontends omitting it get
  `missing_frontend_accessibility` graph diagnostics.

## 3. AccessKit integration

**Status: shipped for the covered element families.**

The compiler/render pipeline maps semantic nodes to AccessKit: roles, names,
values, states, and keyboard shortcut metadata flow into the platform
accessibility tree per surface, including promoted child surfaces (popovers)
— focus and semantics cross the surface boundary with the component tree,
so a popover reads as part of its logical parent.

## 4. Keyboard access

Keyboard behavior is part of the accessibility contract and specified in
[10 — Keyboard](10-keyboard.md): everything clickable is keyboard-reachable
and activatable; focus is visible (themeable `:focus` styles); menus support
roving arrow navigation, Tab transfer, Escape, and return focus; resolved
keybinds surface as shortcut metadata on subscribed controls.

## 5. Reduced motion and contrast

**Status: target.**

- Themes provide modes for contrast needs (high-contrast is a mode like
  dark/light — [04 §1](04-styling.md)).
- A shell prop `shell.motion.reduced` (settings store) exposes the user's
  reduced-motion preference; when set, the animation engine clamps
  non-essential transition/keyframe durations to zero. Components can read
  the token but normally shouldn't need to — the engine handles it.

## 6. Diagnostics

Accessibility gaps are authoring diagnostics, same machinery as everything
else ([01 §9](01-module-system.md)):

- missing accessible name on an interactive control;
- invalid nesting / role misuse for the covered families;
- invalid state attribute values;
- interactive popover/dialog containers without labels;
- missing surface-root role/label (graph-level).

Diagnostics identify the tag, attribute, reason, and the concrete fix.

## 7. One tree, machine consumers

The same semantic tree — not a parallel structure — is what the automation
IPC snapshots and the MCP server exposes to LLMs
([11](11-automation-ipc.md), [12](12-mcp.md)). This is a deliberate
incentive: making a module accessible *is* making it automatable and
AI-legible. A module with good roles, names, and states needs zero extra
work to be scriptable; a module with `aria-hidden` soup is broken for every
consumer at once and the diagnostics say so.
