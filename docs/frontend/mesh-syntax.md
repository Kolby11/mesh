# `.mesh` Component Syntax

A `.mesh` file is a single-file component. It combines markup, logic, styles, and translations in one place.

The syntax described here is the current MESH UI authoring model. Historical
HTML compatibility tags have been removed in favor of shell-specific
primitives.

## Renderer Contract

Renderer migration rules for plugin-authored UI live in [the .mesh renderer contract](renderer-contract.md).
The broader [Native element model](elements.md) defines built-in element families, common attributes, state, events, diagnostics, accessibility expectations, and the relationship to HTML, Qt, and Flutter.

## File structure

```xml
<template>
  <!-- markup goes here -->
</template>

<script lang="luau">
-- Luau logic goes here
</script>

<style>
/* CSS goes here */
</style>
```

Only `<template>` is required. All other blocks are optional.

---

## Markup

### Tags

Use lowercase MESH UI tags for built-in shell primitives. PascalCase tags are
reserved for explicitly imported custom components.

| Tag              | Purpose                        |
| --------------- | ----------------------------- |
| `panel`          | Generic surface/container root |
| `box`            | Generic container              |
| `row`            | Horizontal layout container    |
| `column`         | Vertical layout container      |
| `grid`           | Conservative grid metadata     |
| `stack`          | Stacked layout container       |
| `text`           | Text content                   |
| `label`          | Input label                    |
| `scroll`         | Scrollable region              |
| `scroll-view`    | Semantic scrollable region     |
| `scroll-area`    | Canonical scrollable region    |
| `section`        | Semantic section container     |
| `header`         | Semantic header container      |
| `footer`         | Semantic footer container      |
| `group`          | Semantic grouped content       |
| `form-row`       | Semantic label/control row     |
| `badge`          | Compact status text            |
| `progress`       | Progress display metadata      |
| `tooltip`        | Tooltip metadata/content       |
| `avatar`         | Avatar image or icon metadata  |
| `shortcut`       | Keyboard shortcut label        |
| `button`         | Configurable clickable action  |
| `icon-button`    | Compatibility button alias     |
| `toggle-button`  | Compatibility button alias     |
| `command-button` | Compatibility button alias     |
| `link-button`    | Compatibility button alias     |
| `input`          | Text input                     |
| `textarea`       | Multiline input metadata       |
| `search`         | Search input metadata          |
| `password`       | Masked input metadata          |
| `text-input`     | Semantic text input            |
| `password-input` | Password text input            |
| `search-input`   | Search text input              |
| `number-input`   | Numeric text input             |
| `stepper`        | Numeric stepper metadata       |
| `email-input`    | Email text input               |
| `url-input`      | URL text input                 |
| `slider`         | Range input                    |
| `select`         | Static option choice control   |
| `option`         | Select option                  |
| `switch`         | Switch control                 |
| `checkbox`       | Checkbox control               |
| `radio-group`    | Exclusive radio group          |
| `radio`          | Radio choice                   |
| `segmented-control` | Configured choice group     |
| `menu`           | Roving-focus command list      |
| `menu-item`      | Menu command item              |
| `command-item`   | Command menu item              |
| `preference-row` | Configured preference row      |
| `icon`           | Icon or image asset            |
| `image`          | Image asset                    |
| `list`           | List container                 |
| `list-item`      | List item                      |
| `separator`      | Divider                        |
| `spacer`         | Flexible spacing node          |
| `surface`        | Surface composition primitive  |
| `widget`         | Widget composition primitive   |
| `tabs`           | Tab group container            |
| `tab`            | Activatable tab                |
| `accordion`      | Expandable section group       |
| `details`        | Expandable details container   |
| `popover`        | Popover container              |
| `dialog`         | Dialog container               |
| `sheet`          | Configured sheet container     |
| `empty-state`    | Empty collection content       |

HTML compatibility tags are intentionally not part of the component vocabulary.
Use classes, metadata, accessibility attributes, and component boundaries for
semantics.

Action controls use one native `button` behavior. Put visual content inside the
button, including dedicated `icon` elements:

```xml
<button onclick={toggle_audio} pressed="{audio_open}">
  <icon name="audio-volume-high" />
  <text>Audio</text>
</button>
```

The compatibility action tags lower to the same button runtime. Prefer
configured `button` markup unless a future native element needs distinct focus,
event, accessibility, value, or renderer behavior.

Choice controls use shared value and change semantics. Author selects with
static child options:

```xml
<select value="{language}" onchange={onLanguageChange} aria-label="Language">
  <option value="en">English</option>
  <option value="sk">Slovak</option>
</select>
```

`onchange` receives the selected option value. `checkbox` and `switch` receive a
boolean checked value. `radio` values are exclusive inside `radio-group`.
Menus use `menu-item` or `command-item` children and activate through `onclick`
or `onactivate`; put icons and shortcut labels inside the item markup.

Container and collection elements preserve shell-native source semantics. Use
`tabs` with activatable `tab` children, and `list` with activatable
`list-item` children:

```xml
<tabs label="Debug views">
  <tab selected="{current_view == 'overview'}" onactivate={showOverview}>Overview</tab>
  <tab selected="{current_view == 'surfaces'}" onactivate={showSurfaces}>Surfaces</tab>
</tabs>

<list label="Surfaces">
  <list-item selected="{is_active}" onactivate={openSurface}>
    <text>{surface_id}</text>
  </list-item>
  <empty-state hidden="{has_rows}">No rows</empty-state>
</list>
```

Custom component tags must be PascalCase and must be imported in the script
block before they can be used. New author-facing code uses Luau
`require(...)`:

```luau
local BatteryWidget = require("./components/battery-widget.mesh")
local VolumeBar = require("@mesh/volume-bar")
```

Local component imports resolve relative to the importing file, `@src/...`
resolves from the module's `src/` directory, module component imports resolve
through declared module dependencies. The older
`import Alias from "..."` component syntax is compatibility-era syntax and
should not be used in new examples.

Imported components are definitions, not mounted instances. Markup creates the
instance:

```xml
<VolumeBar device_id="{active_device}" bind:this={volume_bar} />
```

Markup attributes become public fields on the mounted instance. `bind:this`
stores a reference to that mounted instance, so the parent can use public
fields/functions such as `volume_bar.volume` and
`volume_bar.increase_volume(10)`.

Inside an imported component template, expressions and event handlers may
resolve only:
- that component's own private locals and public fields/functions
- public fields initialized by markup attributes
- built-in runtime bindings such as `t(...)`, `refs`, and `settings`

Imported components do not read parent template/script scope implicitly.

Conceptually, every built-in tag inherits the common `MeshElement` surface:
shared attributes like `class`, `id`, `ref`, `style`, accessibility metadata,
and runtime ref metrics such as `width`, `height`, and
`bounding_client_rect`. Control tags then layer on their own fields, so an
input-like tag is effectively `MeshElement` plus things like `value`,
`placeholder`, and `readonly`.

### Text interpolation

Embed dynamic values directly in element content using `{}`:

```xml
<text class="label">{active}</text>
<text class="value">{volume_label}</text>
<text>{t("greeting", { name = userName })}</text>
```

The runtime tracks the referenced variable and re-renders the text node when it changes. Do not use `:content=` — that syntax is not valid.

### Static attributes

Write static attributes as XML-style name/value pairs:

```xml
<button class="chip" title="Toggle Wi-Fi" aria-label="Toggle Wi-Fi">Wi-Fi</button>
<icon src="logo.png" alt="MESH logo" />
<input type="range" min="0" max="100" />
```

### Dynamic attribute binding

Use `{}` to bind an expression to any attribute value:

```xml
<button title="{volume_tooltip}" aria-label="{volume_aria_label}">
  <text>{volume_icon_name}</text>
</button>

<box class="chip {active ? 'chip--on' : 'chip--off'}">{label}</box>
```

### Two-way binding

Use `bind:attr="variable"` to sync an element's attribute back to script
state.

```xml
<input type="text" bind:value="searchQuery" />
<input type="range" min="0" max="100" bind:value="volume" />
<input type="checkbox" bind:checked="enabled" />
```

The runtime reads the initial value from script state and writes back on
change. The variable should be initialized in `<script>`.

Native element behavior is proven through shipped shell surfaces. Prefer the
semantic element that matches the workflow, but keep behavior shell-native:
`popover` for popover surfaces, `dialog` for dialog-like tools, `tabs`/`tab`
for view switching, and `list`/`list-item`/`empty-state` for bounded shell
collections. Browser form semantics, full modal traps, and rich table/tree
models are not implied by these tags.

### Component fields

Pass data into an imported component explicitly with attributes on the
component tag. Attribute names map to public fields on the mounted component
instance:

```xml
<WifiItem network_id="{network.id}" network_name="{network.name}" />
```

Inside `WifiItem`, those fields are normal public script members:

```luau
network_id = nil
network_name = ""
display_name = ""

function render(self)
  display_name = network_name
end
```

They are the supported boundary for parent-to-child data flow. Do not document
or use a separate `self.props` table for new components.

### Component instance binding

Use `bind:this` when the parent needs the mounted child instance:

```xml
<VolumeBar bind:this={volume_bar} />
```

The bound variable references the mounted instance, not the component
definition imported with `require(...)`.

`bind:this` is a **live reference**, not a snapshot. Every component in a single
frontend surface shares one Lua realm, so the bound variable forwards straight
to the child's live state:

- **Reads see the current value.** `volume_bar.percent` reads the child's
  `percent` at call time — no per-frame copy, no staleness.
- **Calls run synchronously and return real values.**
  `local p = volume_bar.set_volume(50)` runs the child's `set_volume` in the
  same tick and returns whatever it returns.
- **Events flow child→parent.** The child's `self.<Event>` channels are exposed
  on the bound reference, so the parent can subscribe and receive synchronous
  fires:

  ```lua
  -- parent
  volume_bar.Changed:on(function(event) audio_label = event.label end)

  -- child (capture the channel where `self` is available, then fire later)
  local changed
  function init(self) changed = self.Changed end
  function onDrag() changed:fire({ label = string.format("%d%%", percent) }) end
  ```

Only the child's **public members** and `self.<Event>` channels cross the
boundary. Host internals (`self`, `module`, `mesh`, `require`, `__mesh_*`) and
lifecycle hooks (`init`, `render`, `mount`, `unmount`) stay private to the child.

Cross-*surface* references (e.g. panel ↔ launcher) are a separate trust boundary
and remain a marshalled event bus — `bind:this` liveness is within one surface.

### Event handlers

Use `on...` event attribute names with a Luau function reference in `{}`:

```xml
<button onclick={onVolumeClick}>Volume</button>
<input type="text" oninput={onSearch} />
<box onmouseenter={onHover} onmouseleave={onBlur}>...</box>
```

Handlers receive an event object. For click handlers, that includes trigger
geometry under `event.current_target`, so a callback can position a surface
explicitly before showing it.

### Element metrics

After a surface has been laid out once, scripts can read render-derived
measurements from host-maintained state. Add `id` or `ref` to a node, then read
`refs.<name>` on the next render tick:

```xml
<box ref="volumeTrigger" onclick={onVolumeClick}>
  <text>{volume_label}</text>
</box>
```

Available fields include `width`, `height`, `left`, `top`, `right`, `bottom`,
`client_width`, `client_height`, `client_bound_rect`, `clientBoundRect`, and
`bounding_client_rect`. Runtime-generated keys are also available in
`elements`, but `refs` is the stable author-facing API.

`refs.<name>` is a **live element reference**, the closest analog to a DOM node
handle. The reference is stable across renders and its fields always report the
most recently painted layout (geometry is only known after layout, so a value
read during `render` reflects the previous committed frame — the same rule as
reading layout inside a DOM effect). `refs.<name>.present` (alias `exists`)
reports whether the element is in the current tree, so a script can guard a
conditionally rendered node.

Live references also expose **imperative methods** that act on the real widget
node — both `refs.x:focus()` (method) and `refs.x.focus()` (plain) call styles
work:

```lua
function onSearchOpen()
    if refs.search_input.present then
        refs.search_input:focus()   -- move keyboard focus to the input
    end
end

function onSearchClose()
    refs.search_input:blur()        -- release focus if this element holds it
end

function onSelectResult()
    refs.active_result:scroll_into_view()  -- scroll the list so the row is visible
end

function onResetList()
    refs.result_list:scroll_to(0)          -- jump the scroll container to the top
end

function onSmoothReveal()
    refs.active_result:scroll_into_view({ smooth = true })  -- animated reveal
    refs.result_list:scroll_to(0, { smooth = true, duration = 300 })
end
```

| Method                  | Effect                                                          |
| ---------------------- | ------------------------------------------------------------- |
| `:focus()`              | Routes through the canonical focus path (fires `onfocus`).     |
| `:blur()`               | Clears focus if this element currently holds it (fires `onblur`). |
| `:click()`              | Synthesizes a click on the node through the real dispatch path (fires `onclick`, or activation handlers for menu/list items). |
| `:scroll_into_view()`   | Scrolls each scrollable ancestor just enough to reveal the element (CSS "nearest" alignment; handles nested scroll regions). |
| `:scroll_to(top[,left])`| Sets this element's own scroll offset (DOM `element.scrollTop`), clamped to its scrollable range; omitted axes stay put. |
| `:set_value(text)`      | Sets an input's text (DOM `input.value = ...`); does not fire `oninput`/`onchange`. Equivalent to `refs.x.value = text`. |

Both scroll methods accept a trailing **options table** `{ smooth = true,
duration = <ms> }` (DOM `behavior: "smooth"`). With `smooth`, the offset eases
to the target (`EaseOut`, default 250 ms) instead of snapping; a later instant
scroll on the same container cancels the animation.

Scroll position and extent are readable live on the reference:
`refs.x.scroll_top` / `scroll_left` (current offset), `refs.x.scroll_height` /
`scroll_width` (full content size), and `refs.x.max_scroll_top` /
`max_scroll_left` (the clamp bounds).

On input-like elements `refs.x.value` is the **live editable text** (DOM
`input.value`) — readable and assignable. Reads reflect the latest paint;
`refs.x.value = "..."` (or `:set_value(...)`) updates the stored text without
firing input events. Every other field is read-only; assigning to it errors.

Method calls are queued and applied by the shell right after the handler
returns, so they compose with the handler's other state changes in one frame.

Common event attributes:

| Attribute      | Fires when                 |
| ------------- | ------------------------- |
| `onclick`      | element is clicked         |
| `oninput`      | input value changes        |
| `onchange`     | input value commits        |
| `onkeydown`    | key pressed while focused  |
| `onkeyup`      | key released while focused |
| `onfocus`      | element gains focus        |
| `onblur`       | element loses focus        |
| `onmouseenter` | pointer enters element     |
| `onmouseleave` | pointer leaves element     |

Do not use `@click=` or `:on*=` — those are not valid MESH syntax.

### Keyboard focus and traversal

- `Tab` and `Shift+Tab` move through the final rendered visual order by default.
- `tabindex` is supported as an override path for keyboard traversal.
- `tabindex="0"` keeps an element in normal traversal, positive values sort before visual-order defaults, and `tabindex="-1"` keeps the element pointer/script focusable but removes it from normal `Tab` traversal.
- `onkeydown` and `onkeyup` fire only for the currently focused element. They are not surface-global handlers.
- Focused-surface shortcuts are configured separately through surface settings and only run while that surface owns keyboard focus.

Keyboard event handlers receive the pressed key plus modifier state:

```lua
function onKeyDown(event)
  if event.key == "Enter" and event.modifiers.shift then
    mesh.log.info("shift-enter on " .. (event.current.key or "unknown"))
  end
end
```

### Selectable text

- `selectable="true"` opts a `text` node into passive pointer selection.
- Selection is currently bounded to that text node. MESH does not yet support document-style selection that spans multiple text nodes or containers.
- `Ctrl+C` copies the current selection when the surface is receiving keyboard input.
- Use this for status copy, proof surfaces, and read-only text such as the shipped navigation-bar status line. It does not make the element editable.

```xml
<text class="status-copy" selectable="true">
  {status_line}
</text>
```

### Accessibility attributes

Always include accessibility attributes where they add meaning. MESH treats these as first-class:

```xml
<button
  title="Open audio controls"
  aria-label="Open audio controls"
  aria-pressed="{isMuted}"
  onclick={onVolumeClick}
>
  <text aria-hidden="true">{volume_icon_name}</text>
  <text class="volume-value">{volume_label}</text>
</button>
```

`title` provides a tooltip and is also used by assistive technology when `aria-label` is absent. Prefer `aria-label` for screen reader text and `title` for visible tooltip text when both are needed.

---

## Script block

Logic lives in the `<script lang="luau">` block. Non-local variables declared
here are public reactive component members; the template re-renders when they
change. `local` variables and functions stay private to the script.

```xml
<script lang="luau">
active = "Dashboard"
volume = 42
local audio = require("mesh.audio@>=1.0")

function onVolumeClick(event)
  audio.toggle_mute()
end

function render(self)
  self.storage.last_volume = volume
end
</script>
```

`local` variables and functions are private to the component. Non-local
variables and functions are public fields/functions on the mounted component
instance. `self.meta` and `self.storage` are runtime-provided current-instance
context; external dependencies still come from `require(...)`.

### Receiving service data

Service data is produced by backend modules, routed by the core, and exposed
to frontend scripts as raw payload. Frontends should derive display state
locally in their own script code.

```xml
<script lang="luau">
volume_icon_name = "audio-volume-muted"
volume_label = "0%"
volume_tooltip = "Volume unavailable"

local audio = require("mesh.audio@>=1.0")

function render(self)
  local audio_percent = audio.percent or 0
  local audio_muted = audio.muted or false
  if audio_muted or audio_percent == 0 then
    volume_icon_name = "audio-volume-muted"
  elseif audio_percent < 34 then
    volume_icon_name = "audio-volume-low"
  elseif audio_percent < 67 then
    volume_icon_name = "audio-volume-medium"
  else
    volume_icon_name = "audio-volume-high"
  end

  volume_label = string.format("%d%%", audio_percent or 0)
  volume_tooltip = string.format("Volume %d%%", audio_percent or 0)
end
</script>
```

The template should read derived globals such as `volume_icon_name` and
`volume_label`. The script reads proxy fields directly during `render(self)`
and derives presentation state locally.

For pointer-driven handlers like `onclick`, the callback also receives an
event table with:

```lua
event.pointer.x
event.pointer.y
event.current_target.bounds.left
event.current_target.bounds.bottom
event.current_target.position.margin_left
event.current_target.position.margin_top
```

That makes "open this popover at the trigger position" fully explicit in the
frontend script.

---

## Style block

Write the supported MESH practical shell CSS subset. See
[`docs/css-coverage.md`](../css-coverage.md) for the complete property and
unsupported-feature contract. Use `var(--...)` to reference theme design
values and local custom properties in supported declarations. Root theme values
use names such as `var(--color-surface)`. Module-scoped theme values use the
module's semantic variable names, for example `var(--weather-color-sunny)`:

```css
<style>
.nav-shell {
    --surface: var(--color-surface);
    background: var(--surface);
    color: var(--color-on-surface);
    padding: var(--spacing-md);
    border: 1px solid var(--color-outline);
    display: flex;
    flex: 1 1 auto;
    overflow: hidden;
    transition: background-color var(--animation-duration-short) var(--animation-curves-bezier-standard);
    animation: pulse var(--animation-duration-fast) var(--animation-curves-bezier-standard);
}

.chip {
    border-radius: var(--radius-full);
    background: var(--color-surface-container);
    font-size: var(--typography-size-sm);
}
</style>
```

Container queries are supported:

```css
@container (max-width: 760px) {
  .label {
    display: none;
  }
}
```

Transitions currently interpolate `background-color`, `border-color`,
`border-radius`, `border-width`, `color`, `opacity`, `width`, `height`,
`padding`, `margin`, and `transform`. `transition-timing-function` accepts the
standard easing keywords plus `cubic-bezier(...)`. `transform` parses
`translate(...)`, `scale(...)`, and `rotate(...)`, but only translation is
painted and hit-tested today.

Theme-driven component defaults resolve before local stylesheet rules. The
effective order is documented in [`../theming/themes.md`](../theming/themes.md).

Keyframes are percentage-only:

```css
@keyframes pulse {
  0% { opacity: 0; }
  100% { opacity: 1; }
}
```

`from` and `to` aliases are rejected in this first release, and keyframe stop
values must stay literal rather than using `var(...)`. Theme variables belong
on the animation shorthand itself, for example
`animation: pulse var(--animation-duration-fast) var(--animation-curves-bezier-standard)`.

---

## Complete example

```xml
<template>
  <row class="nav-shell">
    <box class="meta">
      <text class="meta-label">Current</text>
      <box class="meta-pill">
        <text class="meta-pill-text">{active}</text>
      </box>
      <button
        class="volume-widget"
        onclick={onVolumeClick}
        title="{volume_tooltip}"
        aria-label="Open audio controls"
      >
        <text class="volume-glyph" aria-hidden="true">{volume_icon_name}</text>
        <box class="volume-copy">
          <text class="volume-label">Volume</text>
          <text class="volume-value">{volume_label}</text>
        </box>
      </button>
    </box>
  </row>
</template>

<script lang="luau">
active = "Dashboard"
volume_icon_name = "audio-volume-muted"
volume_label = "0%"
volume_tooltip = "Volume unavailable"

local audio = require("mesh.audio@>=1.0")

function render(self)
    local audio_percent = audio.percent or 0
    local audio_muted = audio.muted or false
    if audio_muted or audio_percent == 0 then
        volume_icon_name = "audio-volume-muted"
    elseif audio_percent < 34 then
        volume_icon_name = "audio-volume-low"
    elseif audio_percent < 67 then
        volume_icon_name = "audio-volume-medium"
    else
        volume_icon_name = "audio-volume-high"
    end

    volume_label = string.format("%d%%", audio_percent or 0)
    volume_tooltip = string.format("Volume %d%%", audio_percent or 0)
end

function onVolumeClick()
  mesh.events.publish("shell.toggle-surface", { surface_id = "@mesh/quick-settings" })
end
</script>

<style>
.nav-shell {
    width: 100%;
    height: 100%;
    justify-content: space-between;
    align-items: center;
    padding-inline: var(--spacing-lg);
    background: var(--color-surface);
    color: var(--color-on-surface);
}

.meta {
    align-items: center;
    gap: var(--spacing-xs);
}

.meta-pill {
    padding-block: var(--spacing-xs);
    padding-inline: var(--spacing-sm);
    border-radius: var(--radius-full);
    background: var(--color-tertiary-container);
}

.meta-pill-text {
    color: var(--color-on-tertiary-container);
    font-size: var(--typography-size-sm);
    font-weight: 700;
}

.volume-widget {
    align-items: center;
    gap: var(--spacing-xs);
    padding-block: var(--spacing-xs);
    padding-inline: var(--spacing-sm);
    border-radius: var(--radius-full);
    background: var(--color-surface-container);
}
</style>
```

---

## Quick reference

| Goal                | Syntax                    |
| ------------------ | ------------------------ |
| Static text         | `<text>Hello</text>`      |
| Dynamic text        | `<text>{variable}</text>` |
| Dynamic attribute   | `title="{expr}"`          |
| Two-way bind        | `bind:value="variable"`   |
| Event handler       | `onclick={handler}`       |
| Selectable text     | `selectable="true"`       |
| Focus order hint    | `tabindex="0"`            |
| Theme token         | `var(--color-surface)`    |
| Translation key     | `{t("key")}`              |
| Tooltip             | `title="..."`             |
| Screen reader label | `aria-label="..."`        |
| Hide from AT        | `aria-hidden="true"`      |
