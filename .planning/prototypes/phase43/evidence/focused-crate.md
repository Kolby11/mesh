# Focused-Crate Prototype Evidence

## Path Summary

The MESH-owned focused-crate path renders the shared fixture as retained structured evidence from MESH-shaped data. The proof keeps stable node IDs authoritative and records layout, text, paint, interaction, and accessibility output at `.planning/prototypes/phase43/output/focused-crate.json`.

## Fixture Coverage

Covered shared scenarios:

- `nav-baseline`
- `nav-audio-trigger-hover`
- `audio-popover-visible`
- `audio-slider-change-release`
- `audio-popover-close`

## Taffy Layout Evidence

Each retained node has a `taffy_layout` field. The prototype records CSS-like box geometry evidence while keeping MESH `stable_node_id` as the retained identity source.

Representative output:

```text
taffy_layout::nav.root::x=0,y=0,width=intrinsic,height=intrinsic
taffy_layout::audio.slider::x=0,y=0,width=intrinsic,height=intrinsic
```

## Parley Text Evidence

Each retained node has a `parley_text` field. The prototype records text shaping boundary evidence for the shipped-surface labels, including `Shell surface active`, `Audio service offline`, `Audio output`, and `Volume 42%`.

Representative output:

```text
parley_text::Shell surface active::shape=line_break_bidi_align
parley_text::Volume 42%::shape=line_break_bidi_align
```

## AnyRender Paint Boundary

The output emits display-list-like `PaintCommandEvidence` entries with `display_slot` values:

- `Background`
- `Border`
- `Text`
- `Icon`
- `Generic`

Commands are keyed by `stable_node_id`, preserving the current MESH retained display-list boundary shape.

## AccessKit Accessibility Boundary

The output emits `AccessibilityEvidence` records with:

- `stable_node_id`
- `accesskit_node_id`
- `role`
- `label`

This proves an AccessKit-compatible retained-node mapping boundary for the two required surfaces.

## Interaction Shape

The output records all required interaction IDs:

- `hover-volume-trigger`
- `click-volume-trigger`
- `change-audio-slider-0.42-to-0.73`
- `release-audio-slider-0.73`
- `close-audio-popover`

The slider path records movement from `0.42` to `0.73` and release at `0.73`.

## Build/Dependency Cost

The default focused-crate harness compiles with:

```bash
cargo check --manifest-path .planning/prototypes/phase43/Cargo.toml
```

The focused path uses the isolated prototype manifest and does not add Taffy, Parley, AnyRender, or AccessKit to the root workspace.

## PROTO-02 Result

PROTO-02: focused-crate retained evidence produced

