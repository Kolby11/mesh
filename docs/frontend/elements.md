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

Common attributes include `id`, `class`, `ref`, `label`, `aria-label`, `disabled`, `readonly`, `required`, `value`, `checked`, `selected`, `expanded`, and `invalid`. Element-specific phases may add narrower attributes, but shared attributes keep diagnostics and Luau state handling consistent across families.

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

## Relationship To HTML Qt And Flutter

MESH is MESH-native, not HTML-compatible, Qt-compatible, or Flutter-compatible. HTML, Qt Widgets/layouts, and Flutter provide coverage references so the element library is broad enough for shell surfaces. MESH does not implement browser form submission, native platform widget embedding, Flutter widget semantics, or one-for-one toolkit behavior.
