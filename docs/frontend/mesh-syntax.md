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
<props>
  <!-- typed, defaulted, localized component configuration goes here -->
</props>

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

## `<props>`

One static Luau table with one entry per configurable value. Each entry auto-projects to three places at
once: a typed `prop(name)` reference usable anywhere in `<style>`, a reactive
`props.name` field in `<script>`, and a generated settings-UI row.

```xml
<props>
{
    width = { type = "size",  default = "fit-content", label = t("var.width") },
    density = { type = "enum",  options = {"compact", "cozy"}, default = "cozy" },
    show_pct = { type = "bool",  default = true, label = t("var.show_percent") },
    accent = { type = "token", default = "color-primary" },
    icon = { type = "icon",  default = "audio-volume-high" },
    anim_ms = { type = "duration", default = 120, min = 0, max = 1000 },
}
</props>
```

Use ordinary Luau table syntax (`=`, comma/semicolon separators, `{ ... }`
arrays and `--` comments). Values are static literals; `t("key", "fallback")`
is translation metadata and is never executed. Computed expressions and calls
are rejected. The legacy colon declarations and square-bracket arrays must be
migrated to the table form above.

`type` is required; `default`, `label`/`description` (localized), `options`
(enum), `min`/`max`/`step`/`unit`, and `expose` (default `true`; `false` keeps
the prop CSS/script-only, with no settings row) are optional. Supported
types: `size`, `number`/`int`, `bool`, `enum`, `string`, `color`, `token`,
`icon`, `duration` — see [component spec §3](../spec/03-components.md#3-the-props-block)
for the full type table and CSS/Lua/control mapping per type. A `token` value
such as `color-primary` or `color.primary` projects to `var(--color-primary)`.

`prop()` is not `var(--…)`: prop names are component-scoped (two components
can both declare `width` with no collision), and it is a typed reference
checked at the use site — `width: prop(density)` is a compile error.
`prop(name)` is usable anywhere `var()` is, including inside `calc()`.

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
<button onclick={toggle_audio} pressed={audio_open}>
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
<select value={language} onchange={onLanguageChange} aria-label="Language">
  <option value="en">English</option>
  <option value="sk">Slovak</option>
</select>
```

`onchange` receives the selected option value. `checkbox` and `switch` receive a
boolean checked value. `radio` values are exclusive inside `radio-group`.
Menus use `menu-item` or `command-item` children and activate through `onclick`
or `onactivate`; put icons and shortcut labels inside the item markup.

### Customizable slots

A component author can expose a bounded region to visual composition without
making the component's complete element tree editable:

```xml
<row class="start">
  <slot
    name="start"
    extension-point="mesh.navigation.item"
    mode="customizable"
  />
</row>
```

`name`, `extension-point`, and `mode` are static. A customizable slot requires
a locally unique `name`, and the module must declare the extension point in
`mesh.hosts`. Its host record can supply ordered `slots.<name>.defaults` using
public contribution references such as `@mesh/navigation-bar:launcher`.

An ordinary `<slot extension-point="…" />` remains automatic and renders the
resolved contributions in their normal order. Visual editors can only arrange
public component contributions and literal public prop values within a
customizable slot. Fixed elements, Luau, handlers, styles, and surface policy
remain source-level `.mesh` customization.

Container and collection elements preserve shell-native source semantics. Use
`tabs` with activatable `tab` children, and `list` with activatable
`list-item` children:

```xml
<tabs label="Debug views">
  <tab selected={current_view == 'overview'} onactivate={showOverview}>Overview</tab>
  <tab selected={current_view == 'surfaces'} onactivate={showSurfaces}>Surfaces</tab>
</tabs>

<list label="Surfaces">
  <list-item selected={is_active} onactivate={openSurface}>
    <text>{surface_id}</text>
  </list-item>
  <empty-state hidden={has_rows}>No rows</empty-state>
</list>
```

Custom component tags must be PascalCase and must be imported in the script
block before they can be used, using Luau `require(...)`:

```luau
local BatteryWidget = require("./components/battery-widget.mesh")
local VolumeBar = require("@mesh/volume-bar")
```

Local component imports resolve relative to the importing file, `@src/...`
resolves from the module's `src/` directory, module component imports resolve
through declared module dependencies. Script source is preserved unchanged.
The legacy `import Alias from "..."` syntax is unsupported: replace it with
`local Alias = require("...")`. It does not register an import and is invalid
Luau when executed.

Imported components are definitions, not mounted instances. Markup creates the
instance:

```xml
<VolumeBar device_id={active_device} bind:this={volume_bar} />
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

### Public style parts

Expose stable styling targets on native elements with static names:

```xml
<button part="volume-button"><icon part="glyph volume-glyph" name="audio-volume-high" /></button>
```

Parts belong to the module authoring the element. Users customize them through
profile CSS such as `@mesh/navigation-bar::part(glyph) { opacity: 0.8; }` in
`settings.shell.theme.stylesheet`. Names are space-separated identifiers;
`part` accepts static strings on native elements. Renaming a public part is a
compatibility change. See [the public styling contract](../spec/04-styling.md#44-public-style-parts-and-user-stylesheets).

### Dynamic attribute binding

Use `{}` to bind an expression to runtime attribute values (`part` stays static):

```xml
<button title={volume_tooltip} aria-label={volume_aria_label}>
  <text>{volume_icon_name}</text>
</button>

<box class={active and "chip chip--on" or "chip chip--off"}>{label}</box>
```

Template expressions are Luau expressions evaluated in the same component
instance environment as the `<script lang="luau">` block. They can read public
and private script locals, call functions, use standard Luau operators, and
access loop-local bindings. The braces select a value-producing expression;
they do not create a separate DSL or restricted scripting scope.

Runtime rendering compiles template expressions as closures in the component
script chunk, so they capture script-local values and functions. Loop bindings
are supplied through a temporary locals-first function environment. Tooling and
VM-less preview paths may use static approximation, but that approximation is
not the language specification.

### Class and style directives

`class:name={condition}` adds the class `name` while the condition is truthy.
`style:property={value}` sets one inline declaration, and leaves it out
while the value is `nil` or `false`. Both combine with a static or bound
`class` / `style` on the same element.

```xml
<button
  class="chip"
  class:chip--on={active}
  class:chip--busy={pending ~= nil}
  style:background-color={swatch}
>{label}</button>
```

Directives are shorthand for one `class` / `style` binding expression, so they
are tracked and diffed like any other binding. Class names may contain
letters, digits, `-` and `_`; style properties are lowercase with `-`.

### Conditional and list blocks

`{#if}` renders its children while a condition is truthy, with optional
`{:else if}` and `{:else}` branches. `{#for}` repeats its children for each
item of a list; give it a `key` whenever items can be reordered, inserted or
removed, so each row keeps its identity and state.

```xml
{#if devices and #devices > 0}
  {#for device in devices key={device.id}}
    <list-item>{device.name}</list-item>
  {/for}
{:else if scanning}
  <text>Searching…</text>
{:else}
  <empty-state>No devices</empty-state>
{/if}
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
change: text inputs assign strings, sliders assign numbers, and checkboxes or
switches assign booleans. Native `<slider bind:value="volume" />`,
`<checkbox bind:checked="enabled" />`, and `<switch bind:checked="enabled" />`
use the same contract. The `input` range and checkbox forms above lower to
those native controls.

Initialize the variable in `<script>`; use a public reactive assignment such
as `searchQuery = ""` so other template readers and effects observe edits.
The target must be a variable name, not an expression, table path, or loop
item. Unsupported element/attribute pairs are compile errors. Write-back runs
before an explicit `onchange` handler, which receives the same typed value;
that handler is resolved at change time. Ordinary `value={searchQuery}`
bindings remain one-way.

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
<WifiItem network_id={network.id} network_name={network.name} />
```

Inside `WifiItem`, those fields are normal public script members:

```luau
network_id = nil
network_name = ""
display_name = ""

effect(function()
  display_name = network_name
end)
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

  A subscriber runs in the parent's own scope, so assigning a public member
  from inside it (`audio_label` above) re-renders the parent's template in the
  same tick — the parent does not have to poll the child from an effect.

Only the child's **public members** and `self.<Event>` channels cross the
boundary. Host internals (`self`, `module`, `mesh`, `require`, `__mesh_*`) and
lifecycle hooks (`init`, `render`, `mount`, `unmount`) stay private to the child.

Cross-*surface* references (e.g. panel ↔ launcher) are a separate trust boundary
and remain a marshalled event bus — `bind:this` liveness is within one surface.

### JSON input

`mesh.json.decode(source)` is available in component and backend scripts without
host capability grants. It accepts at most 1 MiB of JSON text, returns ordinary
Luau values, and raises an error on malformed input or excessive nesting.
JSON null is the `mesh.json.null` sentinel, preserving object fields and array
positions. Use `pcall` when malformed external input should degrade locally.
The decoder performs no I/O; domain providers normalize decoded records into
their declared service contract.

### Event handlers

Use `on...` event attribute names with a Luau function reference in `{}`:

```xml
<button onclick={onVolumeClick}>Volume</button>
<input type="text" oninput={onSearch} />
<box onmouseenter={onHover} onmouseleave={onBlur}>...</box>
```

Handler expressions resolve actual Luau functions in the component's lexical
scope. Private functions, table members and inline closures work directly:

```xml
<button onclick={function(event) select({ id = item.id }, event) end}>Pick</button>
<button onclick={actions:select({ id = item.id })}>Pick</button>
<Child onChoose={select} />
```

An outer call such as `actions:select({ id = item.id })` is event-time sugar:
its arguments are evaluated when the event fires, then event arguments are
appended. Method calls preserve `self`. Inline closures capture the rendered
loop iteration; component state stays live. A handler expression must produce
a function or `nil` to disable an optional handler. Other values, malformed
expressions and runtime failures report local errors.

Callback attributes on imported components become real function-valued public
inputs. The child may invoke them directly from Luau or forward them to another
component. They retain the defining component's environment and host grants;
they are separate from JSON props and persisted preferences. Removing a callback
input restores the child's authored binding. Reloading or unmounting its owner
expires existing callbacks. Each instance admits at most 4096 live bound
callbacks; inactive cache entries are reclaimed at the limit.

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
    refs.active_result:scroll_into_view()  -- smoothly reveal the row
end

function onResetList()
    refs.result_list:scroll_to(0)          -- jump the scroll container to the top
end

function onSmoothReveal()
    refs.result_list:scroll_to(0, { smooth = true, duration = 300 })
end
```

| Method                  | Effect                                                          |
| ---------------------- | ------------------------------------------------------------- |
| `:focus()`              | Routes through the canonical focus path (fires `onfocus`).     |
| `:blur()`               | Clears focus if this element currently holds it (fires `onblur`). |
| `:click()`              | Synthesizes a click on the node through the real dispatch path (fires `onclick`, or activation handlers for menu/list items). |
| `:scroll_into_view()`   | Smoothly scrolls each scrollable ancestor just enough to reveal the element (CSS "nearest" alignment; handles nested scroll regions). |
| `:scroll_to(top[,left])`| Sets this element's own scroll offset (DOM `element.scrollTop`), clamped to its scrollable range; omitted axes stay put. |
| `:set_value(text)`      | Sets an input's text (DOM `input.value = ...`); does not fire `oninput`/`onchange`. Equivalent to `refs.x.value = text`. |

Both scroll methods accept a trailing **options table** `{ smooth = true,
duration = <ms> }` (DOM `behavior: "smooth"`). `scroll_into_view` is smooth by
default; pass `{ smooth = false }` for an immediate reveal. `scroll_to` remains
immediate by default and opts into easing with `{ smooth = true }`. A later
instant scroll on the same container cancels the animation.

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
| `onscroll`     | wheel/scroll over element  |
| `ontwofingerscroll` | two-finger trackpad pan |
| `onswipe`      | trackpad swipe lifecycle   |
| `onpinch`      | trackpad pinch lifecycle   |
| `onhold`       | trackpad hold lifecycle    |
| `ontouchstart` | touchscreen contact begins |
| `ontouchmove`  | touchscreen contact moves  |
| `ontouchend`   | touchscreen contact ends   |
| `ontouchcancel`| compositor cancels contacts |
| `ontap`        | short single-touch tap     |
| `ondoubletap`  | second nearby tap          |
| `onlongpress`  | touch held for 500 ms      |

Do not use `@click=` or `:on*=` — those are not valid MESH syntax.

#### Gesture and touch payloads

Gesture handlers use the same `event.pointer`, `event.surface`, `event.current`,
and `event.current_target` geometry as pointer handlers. Trackpad gestures keep
the node hit at gesture start as their target until the gesture ends.

```luau
function onSwipe(event)
  if event.phase == "end" and event.fingers == 3 then
    mesh.log.info(event.direction .. " at " .. event.velocity.x)
  end
end

function onPinch(event)
  if event.phase == "move" then
    zoom = event.scale       -- relative to gesture start; 1.0 is unchanged
    angle = event.rotation   -- degrees
  end
end
```

| Handler | Additional payload fields |
| ------- | ------------------------- |
| `ontwofingerscroll` | `type`, `delta = { x, y }` |
| `onswipe` | `phase` (`start`, `move`, `end`), `fingers`, `delta`, `total_delta`, and on end `direction`, `velocity`, `duration`, `cancelled` |
| `onpinch` | `phase`, `fingers`, `delta`, `total_delta`, `scale`, `rotation`, `cancelled` |
| `onhold` | `phase`, `fingers`, `duration`, `cancelled` |

Raw touchscreen handlers capture the deepest opted-in node at touch-down and
keep that target for the contact lifetime. `event.touch` is the changed
`{ id, x, y }` point; `event.touches` is the sorted array of contacts still
active after the event; `event.changed_touches` contains changed points; and
`event.cancelled` distinguishes cancellation. Coordinates are logical surface
pixels.

```xml
<button
  onclick={openItem}
  ontap={showTouchFeedback}
  ondoubletap={openDetails}
  onlongpress={openContextMenu}>
  Open
</button>
```

`ontap`, `ondoubletap`, and `onlongpress` add `duration` (seconds),
`tap_count`, `synthesized = true`, and `touch = { id, x, y }`. A tap is one
contact released before the 500 ms long-press threshold without moving more
than 12 logical pixels. Two
taps on the same target within 350 ms and 12 pixels also fire `ondoubletap` on
the second tap. A stationary contact fires `onlongpress` after 500 ms and does
not subsequently fire a tap.

Every recognized tap also synthesizes the normal `onclick`/activation path
with `event.synthesized_from == "touch"`. Existing buttons therefore work on
touchscreens without adding `ontap`; declaring `ontap` is only needed for
touch-specific behavior. If no gesture handler is registered, two-finger input
keeps native scrolling and other gesture events are ignored.

### Keyboard focus and traversal

- `Tab` and `Shift+Tab` move through the final rendered visual order by default.
- `tabindex` is supported as an override path for keyboard traversal.
- `tabindex="0"` keeps an element in normal traversal, positive values sort before visual-order defaults, and `tabindex="-1"` keeps the element pointer/script focusable but removes it from normal `Tab` traversal.
- `autofocus` marks where focus starts when the enclosing surface, dialog, sheet or popover opens. Without it, focus starts at the first editable text field, else the first `Tab` stop; `autofocus="false"` on a dialog, sheet or popover keeps focus where it was ([10 §3](../spec/10-keyboard.md#3-surface-keyboard-policy-keyboard_mode)).
- Keyboard focus is visible by default: the platform draws a `:focus-visible` outline beneath all styles. Restyle it with `outline`, or remove it with `outline: none` ([04](../spec/04-styling.md)).
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
here are public reactive component members; their template bindings update when
they change. Element attributes and text patch in place when all readers of a
changed member are recorded bindings. Direct service-field reads and function
calls in element bindings also patch in place. Keyed loop bodies preserve
iteration locals for patches. Fixed-shape native keyed lists patch changed
items after checking that the ordered unique string/numeric keys are unchanged;
key, order or length changes reconcile the template. Binding targets are indexed
until the template rebuilds. Plain item-field keys reuse validation when those
fields are unchanged; collection snapshots still require item comparisons.
Event handlers in the body
are re-bound to the patched item, so `onclick={pick(item.id)}` acts on its
current value. Nested control flow, child components and two-way bindings
still rebuild. Writes made by input handlers patch the same way once the event
has been dispatched. Structural conditions, unkeyed loops and component props
use template evaluation and retained diffing. Calls
conservatively invalidate on public writes because a helper can capture state.
`local` variables and functions stay private to the script.

```xml
<script lang="luau">
active = "Dashboard"
volume = 42
local audio = require("mesh.audio@>=1.0")

function onVolumeClick(event)
  audio.toggle_mute()
end

effect(function(self)
  self.storage.last_volume = volume
end)
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
local audio = require("mesh.audio@>=1.0")

local function volumeIcon()
  local audio_percent = audio.percent or 0
  local audio_muted = audio.muted or false
  if audio_muted or audio_percent == 0 then
    return "audio-volume-muted"
  elseif audio_percent < 34 then
    return "audio-volume-low"
  elseif audio_percent < 67 then
    return "audio-volume-medium"
  else
    return "audio-volume-high"
  end
end
</script>
<template>
  <row title={string.format("Volume %d%%", audio.percent or 0)}>
    <icon name={volumeIcon()} />
    <text>{string.format("%d%%", audio.percent or 0)}</text>
  </row>
</template>
```

Expressions and lexical helpers derive presentation directly from service
fields. The runtime records those field reads and patches the element bindings
when they change. The script body runs once as setup. Use template expressions
and lexical helpers for derived presentation; `render(self)` is no longer
scheduled and produces a migration diagnostic.

Use `effect(fn)` for reactive side effects. Effects run after setup and when
observed public scalar inputs or service fields change. Compound public values,
props, and settings use conservative invalidation. Return a cleanup function to
release resources before the next run and on reload or unmount. The returned
handle supports `handle:cancel()`, which runs cleanup once. Effects run in
registration order with bounded settling and a callback budget; one failing
effect does not stop independent effects. Timed work uses `task.spawn` and
`task.sleep` to write public state explicitly.

For pointer-driven handlers like `onclick`, the callback also receives an
event table with:

```lua
event.pointer.x
event.pointer.y
event.pointer.button
event.current_target.bounds.left
event.current_target.bounds.bottom
event.current_target.position.margin_left
event.current_target.position.margin_top
```

Pointer coordinates are logical surface pixels. `event.pointer.button` is the
Linux input-event code (`0x110` is the primary/left button); MESH activation
remains primary-button-only.

Inline `<popover>` nodes should normally be authored inside the trigger's
component and anchored declaratively:

```xml
<button ref="theme-button" onpointerenter={openTheme}>Theme</button>
<popover open={theme_open} anchor-ref="theme-button" anchor="bottom" gravity="bottom">
  <menu aria-label="Theme choices">...</menu>
</popover>
```

Open hover menus with the default `grab="hover"` behavior. Hover popovers cannot
take an `xdg_popup` grab because the compositor requires a recent click serial;
the shell hover bridge owns dismissal as the pointer crosses from trigger to
popup. Use `grab="click"` only for click-open popovers that should dismiss on
outside click through the compositor grab.

Popover promotion assumes the same compositor boundary as the rest of the MESH
Wayland shell: `wlr-layer-shell-v1` plus `xdg_popup` support through
layer-shell `get_popup`. wlroots-family compositors, KDE, and Hyprland are in
scope; GNOME's lack of stable layer-shell support is a non-goal for this path.

### Shell requests

`mesh.shell` asks the shell to act on surfaces. Every function needs the
module's `shell.surface` capability; arguments and the capability are checked
at the call, which raises an error on failure.

| Call | Effect |
| --- | --- |
| `mesh.shell.show_surface(id)` / `hide_surface(id)` / `toggle_surface(id)` | Show, hide or toggle a surface. |
| `mesh.shell.activate_popover(id, event?, { focus = bool }?)` | Open a popover surface anchored to the element in `event`. Same as `mesh.popover.activate`. |
| `mesh.shell.hide_popover(id, { defer = bool }?)` | Hide a popover surface; `defer` waits out the shell hover bridge. Same as `mesh.popover.hide`. |
| `mesh.shell.set_surface_role(id, "layer" \| "window")` / `toggle_surface_role(id)` | Move a promotable surface between shell chrome and a window. |
| `mesh.shell.promote_widget(id, node_key)` / `demote_widget(id, node_key)` / `set_widget_role(id, node_key, role)` | Promote an embedded widget to its own window, or dock it back. |
| `mesh.shell.position_surface(id, margin_top, margin_left)` | Move a surface; margins are whole pixels. |

`mesh.events.publish` is for broadcasting to other modules. Publishing a
retired `shell.*` channel such as `shell.show-surface` raises an error that
names the `mesh.shell` replacement, and publishing a service method such as
`mesh.theme.set_theme` raises an error that names the proxy call
(`require("mesh.theme").set_theme(...)`). Service calls go through the proxy
only.

### Timers and tasks

The `task` library schedules work on the component instance. Delays are in
milliseconds.

| Call | Effect |
| --- | --- |
| `task.delay(ms, fn)` | Run `fn` once, `ms` from now. Returns a handle. |
| `task.spawn(fn, ...)` | Run `fn(...)` as a task now, until it finishes, sleeps or awaits. Returns a handle while it is suspended, otherwise `nil`. |
| `task.sleep(ms)` | Suspend the current task. Only valid inside `task.spawn`. |
| `ticket:await()` | Inside `task.spawn`, suspend the task until the service call completes, then return its snapshot (`done`, `status`, `result`). Elsewhere it returns the current snapshot without waiting, like `ticket:poll()`. |
| `handle:cancel()` / `task.cancel(handle)` | Drop a pending task. Returns whether it was still pending. |

```lua
local refresh = nil

function onQueryChanged()
    if refresh then refresh:cancel() end
    refresh = task.delay(150, function()
        refresh = nil
        run_search()
    end)
end
```

Tasks run like event handlers: with the same execution budget, and with state
changes and published events applied afterwards. Scheduling and cancelling stay
inside the script, so neither reaches the shell. Pending tasks belong to the
instance and are dropped when it unmounts or reloads. A delay must be a
finite number between 0 and 24 hours, and one instance may have at most 256
pending tasks; anything else raises an error at the call. An error inside a
task is reported as a handler error and does not stop other due tasks.
The removed `shell.schedule-handler` and `shell.cancel-handler` channels now
raise an error that points to `task.delay`.

```lua
local audio = require("mesh.audio")

function onReset()
    task.spawn(function()
        local done = audio:set_volume("default", 50):await()
        if done.status ~= "ok" then
            reset_error = done.result and done.result.error or done.status
        end
    end)
end
```

A task awaiting a call has no deadline, so it costs no wakeups; the call's
completion makes it due. One call can be awaited by one task at a time.

### Hover intent

Hover-opened popovers do not need a timer in script. Give the `<popover>`
`open-on="hover"` and the core disclosure controller opens it when the pointer
enters its `anchor-ref` trigger and closes it 400 ms after the pointer has left
both trigger and popup. See
[Popover disclosure](elements.md#popover-disclosure).

---

## Style block

Write the supported MESH practical shell CSS subset. See
[`docs/css-coverage.md`](../css-coverage.md) for the complete property and
unsupported-feature contract. Use `var(--...)` to reference theme design
values and local custom properties in supported declarations. Root theme values
use names such as `var(--color-surface)`. Module-scoped theme values use the
module's semantic variable names, for example `var(--weather-color-sunny)`.
Doctor and the LSP warn about literal colours in `<style>` declarations. For
a deliberately fixed colour, append `/* mesh-allow-literal */` after its
semicolon on the same line; modules that deliberately ignore themes declare
`mesh.theme.independent: true`. `transparent` and `currentColor` are exempt.
See [theme coherence](../spec/04-styling.md#7-theme-coherence-enforcement).

For example:

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

Component `<style>` selectors share one cascade across the surface's component
tree, including local/imported components and extension-point contributions.
They can match sibling and parent nodes. Prefix internal classes with a component
or module name (for example `.glass-status-battery`) instead of reusing generic
names such as `.pill`. Adding selector atoms does not increase priority: MESH
applies matching rules in source order without CSS specificity. A popup promoted
from this tree keeps the same stylesheet rules. See the
[stylesheet boundary](../spec/04-styling.md#61-selector-support--cascade-order).

Theme-driven component defaults resolve before local stylesheet rules. The
effective order is documented in [`../spec/04-styling.md`](../spec/04-styling.md).

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
Each percentage stop may also declare `animation-timing-function`; the
validated timing function is used for interpolation until the next stop.

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
        title={volume_tooltip}
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

effect(function(self)
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
end)

function onVolumeClick()
  mesh.shell.toggle_surface("@mesh/quick-settings")
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
| Dynamic attribute   | `title={expr}`            |
| Two-way bind        | `bind:value="variable"`   |
| Event handler       | `onclick={handler}`       |
| Selectable text     | `selectable="true"`       |
| Focus order hint    | `tabindex="0"`            |
| Theme token         | `var(--color-surface)`    |
| Translation key     | `{t("key")}`              |
| Tooltip             | `title="..."`             |
| Screen reader label | `aria-label="..."`        |
| Hide from AT        | `aria-hidden="true"`      |
