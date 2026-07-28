# Phase 61: Localized Resolution And Override Safety - Patterns

## Closest Existing Analogs

| Target Work | Existing Analog | Pattern To Reuse |
|-------------|-----------------|------------------|
| Resolution ordering | `resolve_surface_shortcut_declaration` in `input/keyboard.rs` | Keep precedence explicit: override, locale candidates, generic trigger. |
| Locale fallback | `keybind_locale_candidates` in `input/keyboard.rs` | Exact locale before parent locale; normalize underscores to hyphens. |
| Manifest-first declaration source | `surface_shortcut_declarations` in `input/keyboard.rs` | Manifest action ids suppress same-id legacy settings declarations. |
| Resolver tests | `keybind_locale_*` tests in `navigation.rs` | Synthetic component manifests and `resolved_surface_shortcuts` assertions. |
| Override tests | `keyboard_shortcuts_manifest_keybind_subscriber_resolves_user_override_by_id` | Direct `KeyboardSettings` construction with `surface_shortcuts`. |

## File Roles

- `crates/core/shell/src/shell/component/input/keyboard.rs`: effective resolution, declaration precedence, source metadata, and modifier matching.
- `crates/core/shell/src/shell/component/tests/interaction/navigation.rs`: focused resolver and dispatch regression tests.
- `crates/core/foundation/config/src/lib.rs`: user override schema.
- `crates/core/extension/module/src/manifest/model.rs`: action/trigger kind model.

## Data Flow

Manifest and legacy settings declarations -> optional user override lookup by `surface_id/action_id` -> localized access-key candidate resolution -> generic trigger fallback -> `ResolvedSurfaceShortcut` -> dispatch/accessibility consumers.

## Landmines

- Do not let user overrides create actions that are absent from declarations.
- Do not let localized defaults override shortcut actions; KRES-03 limits localized defaults to access keys unless user override exists.
- Do not remove legacy settings fallback for missing manifest actions.
- Do not change Phase 60 dispatch precedence or focused input protection.
- Do not add diagnostics in Phase 61; malformed/duplicate/unsafe diagnostics belong to Phase 62.
