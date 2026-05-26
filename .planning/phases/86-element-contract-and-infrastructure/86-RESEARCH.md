# Phase 86 Research: Element Contract And Infrastructure

## Research Complete

Phase 86 should consolidate the existing MESH element vocabulary into a registry-backed contract rather than replacing the current parser/compiler stack. The repo already has the right ownership boundaries:

- `crates/core/ui/component/src/template.rs` defines source-level `SourceTag` values and parser AST nodes.
- `crates/core/frontend/compiler/src/tags.rs` lowers source tags to runtime `UiTag` primitives.
- `crates/core/ui/elements/src/element.rs` defines `ElementKind`, field definitions, type definitions, and runtime element snapshots.
- `crates/core/ui/elements/src/tree.rs` stores live `WidgetNode` attributes, event handlers, accessibility info, and `ElementState`.
- `crates/core/frontend/compiler/src/render.rs` resolves attributes, event handlers, default input types, and accessibility roles while building widget nodes.
- `docs/frontend/mesh-syntax.md` is the best author-doc entry point for cross-linking the new element model documentation.

## Existing Implementation Shape

### Element Metadata

`mesh-core-elements` already exposes an `ElementKind` enum, `ElementFieldDef`, `ElementTypeDef`, `BASE_ELEMENT_FIELDS`, family-specific field arrays, and `ELEMENT_TYPE_DEFS`. This is the best place to extend into the Phase 86 taxonomy. Current metadata is narrower than v1.16 needs:

- Existing tags include `box`, `row`, `column`, `stack`, `scroll`, `scroll-view`, `spacer`, `separator`, `text`, `label`, `icon`, `image`, `button`, `icon-button`, `input`, `slider`, `switch`, `checkbox`, `list`, `list-item`, `slot`, `surface`, and `widget`.
- Missing planned tags include `grid`, `divider`, `section`, `header`, `footer`, `group`, `form-row`, `badge`, `progress`, `meter`, `tooltip`, `avatar`, `shortcut`, `toggle-button`, `command-button`, `link-button`, `textarea`, `search`, `password`, `number-input`, `stepper`, `select`, `option`, `radio`, `radio-group`, `segmented-control`, `menu`, `menu-item`, `command-item`, `preference-row`, `popover`, `dialog`, `sheet`, `tabs`, `tab`, `accordion`, `details`, `table`, `row`/collection row ambiguity, `cell`, `tree`, and `empty-state`.
- `ElementStateSnapshot` currently exposes only `hovered`, `active`, `focused`, `disabled`, and `checked`. Phase 86 requires a broader shared state model.

### Parser And Source Tags

The `.mesh` parser uses `SourceTag::from_tag_name` and rejects unknown lowercase tags with actionable errors. It also has reserved PascalCase primitive checks to keep component tags unambiguous. Phase 86 should add metadata-backed validation without weakening the custom-component rule:

- Lowercase primitive tags are MESH-native elements.
- PascalCase tags are imported components.
- Reserved PascalCase primitive names should continue to produce lowercase correction messages.
- Unknown lowercase tags should continue to fail author-facing validation, but the accepted vocabulary should come from the shared element contract.

### Compiler Lowering

`lower_source_tag` maps semantic source tags to the small runtime primitive set used by layout, style, and painter. This is useful and should remain:

- New tags can initially lower to existing primitives (`box`, `row`, `column`, `text`, `button`, `input`, etc.) until later behavior phases add specialized runtime/painter logic.
- Existing shipped modules should not change behavior in Phase 86.
- Attribute/event metadata can be used during compiler validation and docs without requiring every planned tag to render uniquely immediately.

### State, Events, And Accessibility

`WidgetNode` has string attributes, event handler mappings, `AccessibilityInfo`, and `ElementState`. `render.rs` already normalizes `on...` event handler attributes and allows `click`, `change`, `release`, `focus`, `blur`, `keydown`, `keyup`, and `keybind`.

The Phase 86 shared state/event work should:

- Extend state contracts at the metadata level first.
- Keep actual input behavior changes scoped for later control-family phases.
- Introduce event definitions such as `click`, `input`, `change`, `toggle`, `open-change`, `select`, and `activate` as metadata with payload shape descriptions.
- Use current Luau handler naming (`onclick`, `onchange`, `oninput`) rather than adding a separate event syntax in this phase.

### Diagnostics

The parser currently produces hard `ParseError::InvalidTemplate` messages for unknown tags, reserved primitive capitalization, invalid component imports, and malformed markup. Style diagnostics already model non-fatal unsupported CSS behavior. For Phase 86, generic element diagnostics should be introduced as data/validation output that can later flow into component diagnostics:

- Unsupported attribute for a known element.
- Unsupported event handler for a known element.
- Invalid value type or enum literal for a metadata-known attribute.
- Missing accessibility label for label-required element families can be documented now and enforced later where behavior needs it.

## Recommended Implementation Strategy

### Plan Slice 1: Contract Model

Extend `mesh-core-elements` with a richer element contract:

- `ElementFamily`
- `ElementAttributeDef`
- `ElementAttributeType`
- `ElementStateFlag`
- `ElementEventDef`
- `ElementAccessibilityDef`
- `ElementCompatibilityRef`
- richer `ElementTypeDef`

Keep existing `ElementFieldDef`/snapshot APIs compatible by either extending in place or adding parallel metadata structs. Prefer additive changes to avoid breaking downstream code.

### Plan Slice 2: Parser And Compiler Consumption

Make parser/compiler consume the contract instead of maintaining unrelated vocabularies:

- `SourceTag::from_tag_name` should classify from known metadata where feasible.
- `ElementKind::from_tag` and `element_type_for_tag` should know the full Phase 86 taxonomy.
- `lower_source_tag` should continue lowering to current runtime primitives for existing and newly registered elements.
- Attribute/event validation should be generic and non-fatal where the component diagnostics path exists; if the current parser has only fatal errors, return actionable fatal errors only for clearly invalid structure and keep non-fatal diagnostic plumbing as a documented follow-up hook in this phase.

### Plan Slice 3: Docs And Tests

Add `docs/frontend/elements.md` with:

- Native MESH element model.
- Taxonomy grouped by family.
- Common attributes, state flags, events, style hooks, accessibility expectations.
- HTML/Qt/Flutter relationship as inspiration only.
- Out-of-scope parity expectations.

Test focus:

- All ELEMCORE requirement families are present in metadata.
- Existing tags still parse and lower compatibly.
- New planned tags are recognized and lower to safe existing primitives.
- Unsupported attributes/events produce actionable diagnostics or validation results.
- Docs include the taxonomy, common state, event model, diagnostics, and compatibility boundary.

## Risks And Constraints

- `row` is already a layout tag and later collection requirements also name `row`; avoid one enum variant doing two incompatible jobs. Prefer metadata family/context or future `table-row` alias if necessary, while keeping `<row>` as layout.
- Parser currently has fatal errors, while the requirement asks for non-fatal diagnostics. Plan should avoid overpromising full diagnostic transport unless it includes a concrete component diagnostic path.
- New tags that lower to existing primitives must not imply complete behavior. Metadata should distinguish `implemented`, `diagnostic-only`, and `planned` behavior status.
- Accessibility role coverage exists but is not complete for planned controls. Phase 86 can define defaults and tests for metadata without implementing every interactive behavior.

## Validation Architecture

Automated validation should combine source tests and docs checks:

- Unit tests in `mesh-core-elements` for taxonomy completeness, tag lookup, common attributes, state flags, event definitions, and compatibility references.
- Parser/compiler tests for recognition and lowering of representative new tags while preserving shipped tag behavior.
- Diagnostics tests for unsupported attributes/events with concrete author-action messages.
- Documentation assertions or review checks that `docs/frontend/elements.md` covers taxonomy, common attributes, state, events, diagnostics, accessibility, and HTML/Qt/Flutter non-parity.

Manual validation is not required for Phase 86 if tests cover metadata, parser/compiler representation, diagnostics, and docs existence. Later phases will need visual and interaction proof.
