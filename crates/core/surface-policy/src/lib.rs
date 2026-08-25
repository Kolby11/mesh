//! Shared metadata for fields whose meaning depends on a surface role.
//!
//! Manifest diagnostics, settings validation/ejection, and presentation
//! lowering all consume the same table. The crate intentionally contains no
//! manifest, settings, or Wayland types so those boundary crates can depend on
//! it without forming a cycle.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceRoleKind {
    Layer,
    Window,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceFieldScope {
    Layer,
    Window,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceRoleField {
    Title,
    AppId,
    Resizable,
    Decorations,
    Anchor,
    Layer,
    ExclusiveZone,
    KeyboardMode,
    Margins,
    Blur,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceRoleFieldMetadata {
    pub field: SurfaceRoleField,
    pub scope: SurfaceFieldScope,
    /// Canonical author-facing key in `mesh.surface`.
    pub manifest_key: &'static str,
    /// Canonical sparse settings keys that represent this field.
    pub settings_keys: &'static [&'static str],
}

impl SurfaceRoleFieldMetadata {
    pub const fn applies_to(self, role: SurfaceRoleKind, promotable: bool) -> bool {
        promotable
            || matches!(
                (self.scope, role),
                (SurfaceFieldScope::Layer, SurfaceRoleKind::Layer)
                    | (SurfaceFieldScope::Window, SurfaceRoleKind::Window)
            )
    }
}

const TITLE_SETTINGS_KEYS: &[&str] = &["title"];
const APP_ID_SETTINGS_KEYS: &[&str] = &["app_id"];
const RESIZABLE_SETTINGS_KEYS: &[&str] = &["resizable"];
const DECORATIONS_SETTINGS_KEYS: &[&str] = &["decorations"];
const ANCHOR_SETTINGS_KEYS: &[&str] = &["anchor"];
const LAYER_SETTINGS_KEYS: &[&str] = &["layer"];
const EXCLUSIVE_ZONE_SETTINGS_KEYS: &[&str] = &["exclusive_zone"];
const KEYBOARD_MODE_SETTINGS_KEYS: &[&str] = &["keyboard_mode"];
const MARGIN_SETTINGS_KEYS: &[&str] =
    &["margin_top", "margin_right", "margin_bottom", "margin_left"];
const BLUR_SETTINGS_KEYS: &[&str] = &["blur"];

/// The one role-field vocabulary shared by authoring, settings, ejection, and
/// presentation. Keep entries ordered by the surface schema so diagnostics
/// and ejected settings remain deterministic.
pub const SURFACE_ROLE_FIELD_METADATA: &[SurfaceRoleFieldMetadata] = &[
    SurfaceRoleFieldMetadata {
        field: SurfaceRoleField::Title,
        scope: SurfaceFieldScope::Window,
        manifest_key: "title",
        settings_keys: TITLE_SETTINGS_KEYS,
    },
    SurfaceRoleFieldMetadata {
        field: SurfaceRoleField::AppId,
        scope: SurfaceFieldScope::Window,
        manifest_key: "appId",
        settings_keys: APP_ID_SETTINGS_KEYS,
    },
    SurfaceRoleFieldMetadata {
        field: SurfaceRoleField::Resizable,
        scope: SurfaceFieldScope::Window,
        manifest_key: "resizable",
        settings_keys: RESIZABLE_SETTINGS_KEYS,
    },
    SurfaceRoleFieldMetadata {
        field: SurfaceRoleField::Decorations,
        scope: SurfaceFieldScope::Window,
        manifest_key: "decorations",
        settings_keys: DECORATIONS_SETTINGS_KEYS,
    },
    SurfaceRoleFieldMetadata {
        field: SurfaceRoleField::Anchor,
        scope: SurfaceFieldScope::Layer,
        manifest_key: "anchor",
        settings_keys: ANCHOR_SETTINGS_KEYS,
    },
    SurfaceRoleFieldMetadata {
        field: SurfaceRoleField::Layer,
        scope: SurfaceFieldScope::Layer,
        manifest_key: "layer",
        settings_keys: LAYER_SETTINGS_KEYS,
    },
    SurfaceRoleFieldMetadata {
        field: SurfaceRoleField::ExclusiveZone,
        scope: SurfaceFieldScope::Layer,
        manifest_key: "exclusiveZone",
        settings_keys: EXCLUSIVE_ZONE_SETTINGS_KEYS,
    },
    SurfaceRoleFieldMetadata {
        field: SurfaceRoleField::KeyboardMode,
        scope: SurfaceFieldScope::Layer,
        manifest_key: "keyboardMode",
        settings_keys: KEYBOARD_MODE_SETTINGS_KEYS,
    },
    SurfaceRoleFieldMetadata {
        field: SurfaceRoleField::Margins,
        scope: SurfaceFieldScope::Layer,
        manifest_key: "margins",
        settings_keys: MARGIN_SETTINGS_KEYS,
    },
    SurfaceRoleFieldMetadata {
        field: SurfaceRoleField::Blur,
        scope: SurfaceFieldScope::Layer,
        manifest_key: "blur",
        settings_keys: BLUR_SETTINGS_KEYS,
    },
];

pub fn surface_role_field_metadata(field: SurfaceRoleField) -> &'static SurfaceRoleFieldMetadata {
    SURFACE_ROLE_FIELD_METADATA
        .iter()
        .find(|metadata| metadata.field == field)
        .expect("every SurfaceRoleField has metadata")
}

pub fn role_field_applies(
    field: SurfaceRoleField,
    role: SurfaceRoleKind,
    promotable: bool,
) -> bool {
    surface_role_field_metadata(field).applies_to(role, promotable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_field_metadata_covers_every_role_specific_field() {
        assert_eq!(SURFACE_ROLE_FIELD_METADATA.len(), 10);
        for metadata in SURFACE_ROLE_FIELD_METADATA {
            assert!(!metadata.manifest_key.is_empty());
            assert!(!metadata.settings_keys.is_empty());
            assert!(metadata.applies_to(SurfaceRoleKind::Layer, true));
            assert!(metadata.applies_to(SurfaceRoleKind::Window, true));
        }
    }

    #[test]
    fn role_field_metadata_distinguishes_layer_and_window_fields() {
        assert!(role_field_applies(
            SurfaceRoleField::Margins,
            SurfaceRoleKind::Layer,
            false
        ));
        assert!(!role_field_applies(
            SurfaceRoleField::Margins,
            SurfaceRoleKind::Window,
            false
        ));
        assert!(role_field_applies(
            SurfaceRoleField::Decorations,
            SurfaceRoleKind::Window,
            false
        ));
        assert!(!role_field_applies(
            SurfaceRoleField::Decorations,
            SurfaceRoleKind::Layer,
            false
        ));
    }
}
