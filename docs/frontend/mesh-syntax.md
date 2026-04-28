# `.mesh` Component Syntax

A `.mesh` file is a single-file component. It combines markup, logic, styles, schema, translations, and metadata in one place.

The syntax described here is the current MESH UI authoring model. Historical
HTML compatibility tags have been removed in favor of shell-specific
primitives.

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

<schema>
  <!-- typed settings schema -->
</schema>

<i18n>
  <!-- translation keys -->
</i18n>

<meta>
  <!-- accessibility and plugin metadata -->
</meta>
```

Only `<template>` is required. All other blocks are optional.

---

## Markup

### Tags

Use MESH UI tags. These are shell primitives rather than browser DOM elements.

| Tag                | Purpose                         |
| ------------------ | ------------------------------- |
| `box`              | Generic container               |
| `row`              | Horizontal layout container     |
| `column`           | Vertical layout container       |
| `text`             | Text content                    |
| `scroll`           | Scrollable region               |
| `button`           | Clickable action                |
| `input`            | Text input                      |
| `slider`           | Range input                     |
| `label`            | Input label                     |
| `icon`             | Icon or image asset             |
| `separator`        | Divider                         |
| `spacer`           | Flexible spacing node           |

HTML compatibility tags are intentionally not part of the component vocabulary.
Use classes, metadata, accessibility attributes, and component boundaries for
semantics.

Frontend composition can also use custom component tags exported by dependent
plugins via `plugin.json.exports.component.tag`.

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

Common event attributes:

| Attribute      | Fires when                 |
| -------------- | -------------------------- |
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

Logic lives in the `<script lang="luau">` block. Variables declared here are reactive — the template re-renders when they change.

```xml
<script lang="luau">
mesh.state.set("active", "Dashboard")
mesh.state.set("volume", 42)

function onVolumeClick(event)
  mesh.events.publish("shell.position-surface", {
    surface_id = "@mesh/quick-settings",
    margin_top = event.current_target.position.margin_top,
    margin_left = event.current_target.position.margin_left,
  })
  mesh.events.publish("shell.toggle-surface", { surface_id = "@mesh/quick-settings" })
end

function onSearch(event)
  -- event.value holds the current input value
end
</script>
```

### Receiving service data

Service data is produced by backend plugins, routed by the core, and exposed
to frontend scripts as raw payload. Frontends should derive display state
locally in their own script code.

```xml
<script lang="luau">
mesh.state.set("volume_icon_name", "audio-volume-muted")
mesh.state.set("volume_label", "0%")
mesh.state.set("volume_tooltip", "Volume unavailable")

mesh.service.bind("audio.muted", "audio_muted")
mesh.service.bind("audio.percent", "audio_percent")
mesh.service.on("audio", "sync_audio_state")

function sync_audio_state()
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

The template can read the raw service object as `{audio.*}` after updates
arrive. The script can opt into explicit local names like `audio_muted` and
`audio_percent` through `mesh.service.bind("audio.field", "local_name")`, and
it subscribes to updates explicitly with `mesh.service.on("audio", "handler")`.

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

Write standard CSS. Use `token()` to reference theme design tokens:

```css
<style>
.nav-shell {
    background: token(color.surface);
    color: token(color.on-surface);
    padding-inline: token(spacing.lg);
}

.chip {
    border-radius: token(radius.full);
    background: token(color.surface-container);
    font-size: token(typography.size.sm);
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
mesh.state.set("active", "Dashboard")
mesh.state.set("volume_icon_name", "audio-volume-muted")
mesh.state.set("volume_label", "0%")
mesh.state.set("volume_tooltip", "Volume unavailable")

mesh.service.bind("audio.muted", "audio_muted")
mesh.service.bind("audio.percent", "audio_percent")
mesh.service.on("audio", "sync_audio_state")

function sync_audio_state()
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
    padding-inline: token(spacing.lg);
    background: token(color.surface);
    color: token(color.on-surface);
}

.meta {
    align-items: center;
    gap: token(spacing.xs);
}

.meta-pill {
    padding-block: token(spacing.xs);
    padding-inline: token(spacing.sm);
    border-radius: token(radius.full);
    background: token(color.tertiary-container);
}

.meta-pill-text {
    color: token(color.on-tertiary-container);
    font-size: token(typography.size.sm);
    font-weight: 700;
}

.volume-widget {
    align-items: center;
    gap: token(spacing.xs);
    padding-block: token(spacing.xs);
    padding-inline: token(spacing.sm);
    border-radius: token(radius.full);
    background: token(color.surface-container);
}
</style>
```

---

## Quick reference

| Goal                | Syntax                    |
| ------------------- | ------------------------- |
| Static text         | `<text>Hello</text>`      |
| Dynamic text        | `<text>{variable}</text>` |
| Dynamic attribute   | `title="{expr}"`          |
| Two-way bind        | `bind:value="variable"`   |
| Event handler       | `onclick={handler}`       |
| Theme token         | `token(color.surface)`    |
| Translation key     | `{t("key")}`              |
| Tooltip             | `title="..."`             |
| Screen reader label | `aria-label="..."`        |
| Hide from AT        | `aria-hidden="true"`      |
