# Phase 43 Pattern Map

## Scope

Phase 43 should create isolated prototype artifacts and comparison documents. It should not modify production renderer, presentation, frontend module, theme, or shell code.

## Planned Files and Closest Analogs

| Planned file | Role | Closest existing analog | Pattern to preserve |
|--------------|------|-------------------------|---------------------|
| `.planning/prototypes/phase43/Cargo.toml` | Standalone prototype manifest | Root `Cargo.toml` | Keep prototype dependencies out of the root workspace; use an empty `[workspace]` table in the nested manifest. |
| `.planning/prototypes/phase43/fixtures/phase43-scenarios.json` | Shared scenario fixture | `modules/frontend/navigation-bar/src/main.mesh`, `modules/frontend/audio-popover/src/main.mesh` | Preserve surface names, labels, controls, slider values, and open-close state from shipped surfaces. |
| `.planning/prototypes/phase43/src/lib.rs` | Fixture/evidence schema | `crates/core/frontend/render/src/display_list.rs`, `crates/core/frontend/render/src/render_object.rs` | Preserve stable node identity, primitive slots, dirty categories, and display-list-like output. |
| `.planning/prototypes/phase43/src/bin/blitz_reference.rs` | Blitz reference harness | `.planning/phases/42-renderer-architecture-decision-matrix/42-PHASE43-HANDOFF.md` | Use HTML/CSS-equivalent fixture and document exact blocker if Blitz cannot render in throwaway scope. |
| `.planning/prototypes/phase43/src/bin/focused_crate.rs` | Focused-crate harness | `crates/core/frontend/render/src/display_list.rs` | Emit retained layout, text, paint, interaction, and accessibility evidence from MESH-shaped fixture data. |
| `.planning/prototypes/phase43/evidence/*.md` | Per-path evidence | Phase 42 matrix/handoff docs | Record comparable dimensions and reproduction commands. |
| `.planning/phases/43-comparable-renderer-prototype-proofs/43-PROTOTYPE-COMPARISON.md` | Final comparison | `42-DECISION-MATRIX.md` | Use common headings for both paths and make the Phase 44 recommendation explicit. |
| `.planning/phases/43-comparable-renderer-prototype-proofs/43-PHASE44-HANDOFF.md` | Next-phase handoff | `42-PHASE43-HANDOFF.md` | Name selected path, integration boundary, preserved MESH contracts, and remaining risks. |

## Concrete Source Details to Carry Forward

- Navigation baseline must include `Shell surface active`, `Audio service offline`, `control-cluster`, `VolumeButton`, `ThemeButton`, and `SettingsButton`.
- Audio popover must include `Audio output`, `Volume 42%` or equivalent percent label, `audio-slider`, mute button, volume down/up buttons, and close/exiting state.
- Retained evidence must include dirty categories: transform, clip, opacity, geometry, material, text, and accessibility.
- Display evidence must include primitive slots: Background, Border, Text, Icon, and Generic or exact prototype equivalents.
- Accessibility evidence must keep node identity stable enough to map MESH node IDs to AccessKit node IDs.

