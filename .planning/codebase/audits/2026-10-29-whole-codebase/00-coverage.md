# Coverage inventory — continuation

The [2026-10-27 inventory](../2026-10-27-whole-codebase/00-coverage.md) assigns the original workspace files to sections and records prior review status. Offline Cargo metadata on 2026-10-29 reports 31 workspace packages; all their manifests appear in these inventories. Section 08 has received the additional review described in its [continuation report](sections/08-ui-element-core.md), with explicit limits on external callers, embedded tests and independent performance rereads. Sections 10–16 remain open, and cross-section review is pending. The inventory must be refreshed again before finalizing because the working tree is changing.

An `rg --files` comparison identified the following additions to the original inventory. Assignment is by current ownership; assignment does not imply inspection.

| Section | Added paths |
| --- | --- |
| 01 | `crates/core/foundation/config/src/keybinds.rs` |
| 02 | `modules/compositions/halo/module.json`, `modules/frontend/glass-controls/module.json`, `modules/frontend/halo-orb/module.json`, `modules/frontend/keybind-help/module.json`, `modules/frontend/radial-launcher/module.json`, `modules/themes/liquid-glass/module.json` |
| 03 | `crates/core/extension/service/src/descriptor.rs`, `crates/core/extension/service/src/generator/rust.rs`, `crates/core/extension/service/tests/fixtures/audio.contract.json`, `crates/core/extension/service/tests/generated/audio.rs`, `crates/core/extension/service/tests/rust_bindings.rs` |
| 04 | `modules/themes/liquid-glass/theme.css` |
| 05 | `modules/frontend/keybind-help/config/i18n/en.json`, `modules/frontend/radial-launcher/config/i18n/en.json` |
| 06 | `crates/core/foundation/resources/src/blob.rs` |
| 08 | `crates/core/ui/elements/src/control_parts.rs`, `crates/core/ui/elements/src/shape.rs` |
| 10 | `crates/core/frontend/compiler/tests/radial_launcher.rs`, `crates/core/frontend/runtime/src/catalog.rs`, `crates/core/frontend/runtime/src/catalog_imports.rs`, `crates/core/frontend/runtime/src/fingerprint.rs`, `crates/core/frontend/runtime/src/generation.rs`, `crates/core/frontend/runtime/src/locale.rs`, `crates/core/frontend/runtime/src/service.rs`, `modules/frontend/glass-controls/components/glass-button.mesh`, `modules/frontend/halo-orb/src/main.mesh`, `modules/frontend/keybind-help/src/main.mesh`, `modules/frontend/radial-launcher/src/main.mesh`, `modules/frontend/settings/src/components/keyboard-page.mesh` |
| 12 | `crates/core/frontend/render/src/surface/painter/tests/skia/vector_shape.rs` |
| 15 | `config/profiles/halo.json`, `crates/core/shell/src/shell/component/tests/integration/real_surfaces/keybind_help.rs`, `crates/core/shell/src/shell/component/tests/integration/real_surfaces/radial_launcher.rs`, `crates/core/shell/src/shell/component/tests/interaction/key_scopes.rs` |

The original inventory lists four paths no longer present: `modules/themes/solarized-dark/module.json`, `modules/themes/solarized-dark/theme.css`, `modules/themes/violet-glass/module.json` and `modules/themes/violet-glass/theme.css`. These are absent from the current checkout, not reviewed current files. Other changes to previously inventoried files also require revalidation where relevant.

A later comparison found additional in-flight paths: section 02 owns `modules/backend/hyprland-keybinds/module.json`, `modules/frontend/cheatsheet/module.json`, `modules/interfaces/wm-keybinds/module.json` and `modules/themes/vitrum/module.json`; section 04 owns `modules/themes/vitrum/dark.css` and `modules/themes/vitrum/light.css`; section 05 owns `modules/frontend/cheatsheet/config/i18n/en.json`; section 10 owns `modules/frontend/cheatsheet/src/main.mesh`; section 11 owns `modules/backend/hyprland-keybinds/src/main.luau`; section 15 owns `crates/core/shell/src/shell/component/tests/integration/real_surfaces/cheatsheet.rs`. These assignments do not imply review.
