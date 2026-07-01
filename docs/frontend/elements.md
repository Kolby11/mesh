# MESH Native Elements

MESH elements are shell-native retained UI primitives. They are inspired by familiar coverage categories from HTML, Qt Widgets/layouts, and Flutter, but they are not compatibility targets. MESH owns parsing, retained rendering, Luau event delivery, accessibility metadata, diagnostics, styling hooks, and shell interaction rules.

## Native Element Model

Lowercase tags are built-in MESH elements. PascalCase tags are imported components. Native elements have source-level semantics, shared metadata, runtime state, style hooks, Luau event handlers, and accessibility defaults. Later behavior phases add richer rendering and interaction against this contract rather than redefining element names.

## Element Families

| Family | Representative Tags | Purpose |
|--------|---------------------|---------|
| Layout | `box`, `row`, `column`, `grid`, `stack`, `spacer`, `divider`, `scroll-area` | Compose retained shell layouts. |
| Structure | `section`, `header`, `footer`, `group`, `form-row` | Add semantic grouping for styling and accessibility. |
| Display | `text`, `icon`, `image`, `badge`, `progress`, `meter`, `tooltip`, `avatar`, `shortcut` | Present read-only content and status. |
| Action | `button`, `icon-button`, `toggle-button`, `command-button`, `link-button` | Trigger Luau actions. |
| Text and numeric input | `input`, `textarea`, `search`, `password`, `number-input`, `stepper` | Capture text or numeric values. |
| Choice and menu | `select`, `option`, `checkbox`, `switch`, `radio`, `radio-group`, `segmented-control`, `menu`, `menu-item`, `command-item`, `separator`, `preference-row` | Present choices, menus, and command rows. |
| Container | `panel`, `popover`, `dialog`, `sheet`, `tabs`, `tab`, `accordion`, `details` | Own open/closed, grouped, or surface-like UI regions. |
| Collection | `list`, `list-item`, `table`, `cell`, `tree`, `empty-state` | Present repeated or structured shell data. |
| Shell | `slot`, `surface`, `widget` | Connect component composition and shell-owned surfaces. |

## Common Attributes

Common attributes include `id`, `class`, `style`, `ref`, `label`, `aria-label`, `role`, `aria-role`, `title`, `disabled`, `readonly`, `required`, `value`, `checked`, `selected`, `expanded`, and `invalid`. Element-specific phases may add narrower attributes, but shared attributes keep diagnostics and Luau state handling consistent across families.

## Layout And Display Elements

Phase 87 makes the layout, structure, and display subset usable for shell composition. These layout and display elements are the first behavior slice built on the native taxonomy.

Layout primitives:

| Tag | Behavior |
|-----|----------|
| `box` | Generic retained container. |
| `row` | Horizontal flex layout. |
| `column` | Vertical flex layout. |
| `grid` | Conservative source-level grid metadata. `columns` and `rows` accept fixed pixel tracks such as `120px` and `auto`; broader browser grid syntax is diagnosed. |
| `stack` | Overlay composition using existing absolute positioning and z-order behavior. |
| `spacer` | Flexible empty space, typically with flex growth or explicit size. |
| `divider` / `separator` | Visual separator elements. |
| `scroll-area` | Canonical semantic scroll region. Runtime behavior remains compatible with existing `scroll` and `scroll-view` tags. |

Structure primitives:

`section`, `header`, `footer`, `group`, and `form-row` lower to compatible runtime containers while preserving source semantics through metadata, accessibility roles, labels, and style hooks. Use these tags when the grouping matters to styling or assistive technology, not to request browser-like document behavior.

Display primitives:

| Tag | Behavior |
|-----|----------|
| `text` | Text content. |
| `icon` | Icon theme or source-backed icon. |
| `image` | Image source with accessible alternate text. |
| `badge` | Compact status text. |
| `progress` | Progress indicator metadata with `value`, `min`, `max`, and `indeterminate`. |
| `tooltip` | Tooltip content or ownership metadata tied into the existing title/tooltip lookup path. |
| `avatar` | Image/icon-backed avatar metadata. |
| `shortcut` | Keyboard shortcut label metadata. |

`meter` remains part of the taxonomy for future coverage, but Phase 87 does not create separate runtime behavior for it. Use `progress` for current progress-like display.

Example:

```xml
<scroll-area class="panel-scroll" overflow-y="auto">
  <grid columns="120px auto" gap="8" label="Download status">
    <text>Package</text>
    <badge>Ready</badge>
    <text>Progress</text>
    <progress value="{download_percent}" min="0" max="100" label="Download progress" />
  </grid>
</scroll-area>
```

## Action And Text Input Controls

Phase 88 keeps action controls intentionally small: `button` is the only native action behavior. Use attributes to configure state and intent, and use child markup for visual content.

```xml
<button class="toolbar-action" onclick={toggle_audio} pressed="{audio_open}" keybind="audio.toggle">
  <icon name="audio-volume-high" />
  <text>Audio</text>
</button>
```

Do not put icon shortcut attributes on `button`. `icon`, `name`, and `src` belong on a dedicated child `icon` or `image` element. The compatibility tags `icon-button`, `toggle-button`, `command-button`, and `link-button` are still accepted, but they lower to the same runtime `button` path and should not be treated as separate native behavior.

Button attributes include `variant`, `pressed`, `disabled`, `busy`, `default`, `destructive`, `keybind`, `command`, and `href`. `command` and `href` are intent metadata; Luau handlers still own the actual command or navigation behavior.

Text and numeric controls use one native runtime input path. Source tags configure semantics:

| Tag | Runtime behavior |
|-----|------------------|
| `input` | Generic single-line input. |
| `textarea` | Input with multiline source metadata. Full multiline editing remains conservative. |
| `search` | Input with search type metadata. |
| `password` | Input with masked source metadata. |
| `number-input` | Input with numeric `min`, `max`, `step`, and `value` diagnostics. |
| `stepper` | Numeric input with stepper source semantics and default step metadata. |

Text inputs support `value`, `placeholder`, `disabled`, `readonly`, `required`, `invalid`, `oninput`, and `onchange`. Runtime edits dispatch `oninput` for immediate value updates and keep `onchange` compatibility for committed/current value handlers.

```xml
<search value="{query}" placeholder="Search modules" oninput={on_query_input} />
<number-input value="{limit}" min="1" max="20" step="1" onchange={on_limit_change} />
```

## Choice Controls And Menus

Phase 89 adds native behavior for the choice/menu controls that need distinct value or focus semantics: `select`, `checkbox`, `switch`, `radio-group`/`radio`, and `menu`. `segmented-control`, `menu-item`, `command-item`, `separator`, and `preference-row` remain configured source elements over existing runtime primitives until they need separate rendering or value behavior.

Use static child `option` elements for selects:

```xml
<select value="{language}" onchange={on_language_change} aria-label="Language">
  <option value="en">English</option>
  <option value="sk">Slovak</option>
</select>
```

Selecting an option dispatches `onchange` on the parent `select` with the option `value`. `option` supports `value`, `selected`, and `disabled`.

`checkbox` and `switch` dispatch `onchange` with a boolean checked value. `radio` dispatches its string `value`, and nested radios are exclusive within their parent `radio-group`.

Menus use roving focus for command rows. Use child `icon`, `text`, and `shortcut` elements for menu content; menu items activate through `onclick` or `onactivate`.

```xml
<menu aria-label="Audio menu">
  <menu-item onactivate={toggle_mute} keybind="audio.toggle">
    <icon name="audio-volume-muted" />
    <text>Mute</text>
    <shortcut>Ctrl+M</shortcut>
  </menu-item>
</menu>
```

## Containers And Collections

Phase 90 adds native source semantics for the container and collection elements needed by shipped shell surfaces.

Containers include `popover`, `dialog`, `tabs`, `tab`, `accordion`, and `details`. `panel` and `sheet` remain configured containers for now. Popover focus and escape behavior continue to use the existing shell cross-surface popover system; Phase 90 does not add a full in-tree modal trap or backdrop model.

Inline `<popover>` nodes are promoted to compositor popups when open, which lets a short parent surface such as a panel host content that paints outside its own buffer. Author placement with `anchor-ref`, `anchor`, `gravity`, `offset-x`, `offset-y`, and `constrain`; avoid manifest surface geometry for embeddable popovers.

Use `grab="hover"` or omit `grab` for hover-open menus. Hover popovers do not take a compositor grab because an `xdg_popup` grab requires a recent click serial; the shell hover bridge handles dismissal while the pointer crosses from trigger to popup. Use `grab="click"` only for click-open popovers that should use compositor outside-click dismissal and keyboard focus ownership.

Popover promotion depends on the compositor protocols MESH already targets: `wlr-layer-shell-v1` plus `xdg_popup` support via layer-shell `get_popup`. This is expected on wlroots-family compositors, KDE, and Hyprland. GNOME does not expose layer-shell as a stable target, so GNOME support remains outside the current shell compatibility boundary rather than a separate popover fallback requirement.

Tabs are an activatable group:

```xml
<tabs label="Debug views">
  <tab selected="{view == 'overview'}" onactivate={show_overview}>Overview</tab>
  <tab selected="{view == 'surfaces'}" onactivate={show_surfaces}>Surfaces</tab>
</tabs>
```

Collections start with `list` and `list-item`. List items can expose `selected`, `active`, `disabled`, `onclick`, and `onactivate`. `table`, `cell`, `tree`, and `empty-state` carry semantic metadata and style hooks, but rich table/tree keyboard models are deferred.

```xml
<list label="Surfaces">
  <list-item selected="{is_current}" onactivate={open_surface}>
    <text>{surface_id}</text>
  </list-item>
  <empty-state hidden="{has_rows}">No recent surface activity</empty-state>
</list>
```

## Shared State

Shared state names are `disabled`, `read-only`, `required`, `focused`, `selected`, `checked`, `expanded`, `pressed`, `invalid`, `active`, and `value`. Not every element uses every state. Element metadata defines which states apply, and the runtime exposes applicable state through retained nodes, style hooks, accessibility metadata, and Luau event payloads.

## Events

Elements use Luau handler attributes such as `onclick`, `oninput`, `onchange`, `onselect`, `onactivate`, and `onopenchange`. Handlers are normalized to event names such as `click`, `input`, `change`, `select`, `activate`, and `openchange`. Controls that own a value should use shared value/change plumbing rather than bespoke per-control state workarounds.

## Style Hooks

Style hooks are based on stable element tags, classes, ids, and state pseudo-classes. Common pseudo-state hooks include `:disabled`, `:readonly`, `:required`, `:focus`, `:focus-visible`, `:selected`, `:checked`, `:expanded`, `:pressed`, `:invalid`, and `:active`.

## Accessibility

Native elements carry accessibility roles, focusability, labels, descriptions, value metadata, checked/selected/expanded state, and keyboard shortcut metadata where applicable. Interactive controls should have an accessible name through visible text, `label`, or `aria-label`.

## Diagnostics

Unsupported attributes, unsupported event handlers, invalid values, invalid nesting, missing assets, and missing accessible names should produce actionable diagnostics. Diagnostics should identify the tag, the attribute or event, why it is unsupported, and what the author can do instead.

For Phase 87, diagnostics intentionally keep the layout/display scope narrow: unsupported complex grid syntax is rejected, progress range/value fields must be numeric, boolean progress fields must be boolean-like, structure elements reject control value state, and tooltip ownership must point at a non-empty owner id.

For Phase 88, diagnostics reject button icon shortcut attributes, unsupported browser form/navigation behavior, invalid numeric input values, non-positive numeric steps, and invalid boolean state values.

For Phase 89, diagnostics validate choice/menu state attributes, require non-empty `option` and `radio` values when authored statically, and reject invalid boolean state values.

For Phase 90, diagnostics validate container and collection boolean state attributes and require non-empty labels on interactive popover/dialog containers when authored statically.

## Deferred Element Behavior

The following families remain defined by the native element taxonomy but are not expanded yet: rich data-driven option APIs, nested menu popups, full modal focus traps/backdrops, rich table/tree behavior, full gallery proof, distinct `meter` runtime behavior, distinct action button runtimes, and full multiline text editing.

## Relationship To HTML Qt And Flutter

MESH is MESH-native, not HTML-compatible, Qt-compatible, or Flutter-compatible. HTML, Qt Widgets/layouts, and Flutter provide coverage references so the element library is broad enough for shell surfaces. MESH does not implement browser form submission, native platform widget embedding, Flutter widget semantics, or one-for-one toolkit behavior.

## Shipped Surface Proof

The v1.16 element library is proven on shipped shell surfaces rather than a separate gallery. Navigation uses native action and choice controls. Audio popover uses popover semantics, slider, buttons, and icon content. Debug inspector uses dialog, tabs, tab, list, list-item, and empty-state semantics. The text-selection proof surface covers selectable text and clipboard behavior.

These proofs intentionally keep behavior shell-native. Browser form submission, full modal backdrop/trap behavior, rich table/tree models, and data-driven select APIs remain explicit future work.
