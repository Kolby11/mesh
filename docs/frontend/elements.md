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

For Phase 87, diagnostics intentionally keep the scope narrow: unsupported complex grid syntax is rejected, progress range/value fields must be numeric, boolean progress fields must be boolean-like, structure elements reject control value state, and tooltip ownership must point at a non-empty owner id.

## Deferred Element Behavior

The following families remain defined by the native element taxonomy but are not expanded by the Phase 87 behavior slice: action controls beyond existing button compatibility, text and numeric input variants, choice/menu controls, container/collection controls, full gallery proof, and distinct `meter` runtime behavior.

## Relationship To HTML Qt And Flutter

MESH is MESH-native, not HTML-compatible, Qt-compatible, or Flutter-compatible. HTML, Qt Widgets/layouts, and Flutter provide coverage references so the element library is broad enough for shell surfaces. MESH does not implement browser form submission, native platform widget embedding, Flutter widget semantics, or one-for-one toolkit behavior.
