# `.mesh` Component Syntax

A `.mesh` file is a single-file component. It combines markup, logic, styles, schema, translations, and metadata in one place.

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

Use standard HTML tags. MESH renders these as shell UI primitives.

| Tag                | Purpose                         |
| ------------------ | ------------------------------- |
| `div`              | Generic block container         |
| `span`             | Generic inline container        |
| `p`                | Paragraph text                  |
| `nav`              | Navigation region               |
| `header`           | Surface header region           |
| `aside`            | Sidebar or supplementary region |
| `section`          | Logical content section         |
| `article`          | Self-contained content block    |
| `main`             | Primary content region          |
| `ul` / `ol` / `li` | Lists                           |
| `button`           | Clickable action                |
| `input`            | Text or range input             |
| `label`            | Input label                     |
| `img`              | Image                           |
| `hr`               | Divider                         |

Do not use tags that imply compositor-level layout (`footer`, `dialog`, `frame`) unless the surface role explicitly warrants it. Prefer semantic tags over generic `div` wherever meaning is clear.

### Text interpolation

Embed dynamic values directly in element content using `{}`:

```xml
<span class="label">{active}</span>
<span class="value">{volume_label}</span>
<p>{t("greeting", { name = userName })}</p>
```

The runtime tracks the referenced variable and re-renders the text node when it changes. Do not use `:content=` — that syntax is not valid.

### Static attributes

Write static attributes exactly as in HTML:

```xml
<button class="chip" title="Toggle Wi-Fi" aria-label="Toggle Wi-Fi">Wi-Fi</button>
<img src="logo.png" alt="MESH logo" />
<input type="range" min="0" max="100" />
```

### Dynamic attribute binding

Use `{}` to bind an expression to any attribute value:

```xml
<button title="{volume_tooltip}" aria-label="{volume_aria_label}">
  <span>{volume_icon_name}</span>
</button>

<div class="chip {active ? 'chip--on' : 'chip--off'}">{label}</div>
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

Use standard HTML event attribute names with a Luau function reference in `{}`:

```xml
<button onclick={onVolumeClick}>Volume</button>
<input type="text" oninput={onSearch} />
<div onmouseenter={onHover} onmouseleave={onBlur}>...</div>
```

Handlers receive an event object. For click handlers, that includes trigger
geometry under `event.current_target`, so a callback can position a surface
explicitly before showing it.

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
  <span aria-hidden="true">{volume_icon_name}</span>
  <span class="volume-value">{volume_label}</span>
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
  <nav class="nav-shell">
    <div class="meta">
      <span class="meta-label">Current</span>
      <div class="meta-pill">
        <span class="meta-pill-text">{active}</span>
      </div>
      <button
        class="volume-widget"
        onclick={onVolumeClick}
        title="{volume_tooltip}"
        aria-label="Open audio controls"
      >
        <span class="volume-glyph" aria-hidden="true">{volume_icon_name}</span>
        <div class="volume-copy">
          <span class="volume-label">Volume</span>
          <span class="volume-value">{volume_label}</span>
        </div>
      </button>
    </div>
  </nav>
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
| Static text         | `<span>Hello</span>`      |
| Dynamic text        | `<span>{variable}</span>` |
| Dynamic attribute   | `title="{expr}"`          |
| Two-way bind        | `bind:value="variable"`   |
| Event handler       | `onclick={handler}`       |
| Theme token         | `token(color.surface)`    |
| Translation key     | `{t("key")}`              |
| Tooltip             | `title="..."`             |
| Screen reader label | `aria-label="..."`        |
| Hide from AT        | `aria-hidden="true"`      |
