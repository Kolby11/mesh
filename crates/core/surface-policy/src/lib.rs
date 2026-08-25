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

/// The normalized, compositor-independent values that participate in a live
/// surface policy decision.
///
/// This is deliberately owned by the policy crate rather than by either the
/// settings resolver or a Wayland backend. Callers lower their local surface
/// types into this snapshot once, and every consumer then uses the same typed
/// diff. In particular, a new field cannot be added to the shell's reload
/// comparison without also being considered by presentation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfacePolicySnapshot {
    /// Monotonic generation assigned by [`SurfacePolicyGenerator`]. The
    /// revision is metadata and is intentionally ignored when comparing the
    /// semantic values below.
    pub revision: u64,
    pub role: SurfaceRoleKind,
    pub promotable: bool,
    pub visible: bool,
    /// The base compositor namespace. `blur` is kept separately so the
    /// snapshot records the author intent even before a backend lowers it.
    pub namespace: String,
    pub blur: bool,
    pub window_title: Option<String>,
    pub window_app_id: Option<String>,
    pub window_resizable: bool,
    pub window_decorations: SurfacePolicyDecorations,
    pub edge: Option<SurfacePolicyEdge>,
    pub layer: SurfacePolicyLayer,
    pub size_policy: SurfacePolicySizePolicy,
    pub content_size: Option<(u32, u32)>,
    pub surface_size: Option<(u32, u32)>,
    pub width_spans_output: bool,
    pub height_spans_output: bool,
    pub exclusive_zone: i32,
    pub keyboard_mode: SurfacePolicyKeyboardMode,
    pub margins: [i32; 4],
    pub padding: [u32; 4],
}

impl Default for SurfacePolicySnapshot {
    fn default() -> Self {
        Self {
            revision: 0,
            role: SurfaceRoleKind::Layer,
            promotable: false,
            visible: false,
            namespace: String::new(),
            blur: false,
            window_title: None,
            window_app_id: None,
            window_resizable: true,
            window_decorations: SurfacePolicyDecorations::Client,
            edge: Some(SurfacePolicyEdge::Top),
            layer: SurfacePolicyLayer::Top,
            size_policy: SurfacePolicySizePolicy::Fixed,
            content_size: None,
            surface_size: None,
            width_spans_output: false,
            height_spans_output: false,
            exclusive_zone: 0,
            keyboard_mode: SurfacePolicyKeyboardMode::None,
            margins: [0; 4],
            padding: [0; 4],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfacePolicyDecorations {
    Client,
    Server,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfacePolicyEdge {
    Top,
    Bottom,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfacePolicyLayer {
    Background,
    Bottom,
    Top,
    Overlay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfacePolicySizePolicy {
    Fixed,
    Flexible,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfacePolicyKeyboardMode {
    None,
    Exclusive,
    OnDemand,
}

/// The semantic work required to move from one accepted surface policy to the
/// next. The values are ordered from the most destructive transition to the
/// least destructive one by [`SurfacePolicySnapshot::diff`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfacePolicyChange {
    Noop,
    InputRegionOnly,
    MeasureAgain,
    LayerConfigure,
    LayerRecreate,
    WindowLive,
    WindowRecreate,
    RoleTransition,
    VisibilityOnly,
}

/// A revisioned semantic policy diff shared by shell reload and presentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfacePolicyDiff {
    pub change: SurfacePolicyChange,
    pub from_revision: Option<u64>,
    pub to_revision: u64,
}

impl SurfacePolicyDiff {
    pub const fn is_noop(self) -> bool {
        matches!(self.change, SurfacePolicyChange::Noop)
    }

    pub const fn requires_recreation(self) -> bool {
        matches!(
            self.change,
            SurfacePolicyChange::LayerRecreate
                | SurfacePolicyChange::WindowRecreate
                | SurfacePolicyChange::RoleTransition
        )
    }

    pub const fn requires_fresh_configure(self) -> bool {
        matches!(
            self.change,
            SurfacePolicyChange::LayerConfigure
                | SurfacePolicyChange::LayerRecreate
                | SurfacePolicyChange::WindowRecreate
                | SurfacePolicyChange::RoleTransition
        )
    }
}

impl SurfacePolicySnapshot {
    /// Compare semantic policy values while carrying the revisions through the
    /// result for stale-generation diagnostics and accepted-cache assertions.
    pub fn diff(&self, next: &Self) -> SurfacePolicyDiff {
        let change = if self.role != next.role {
            SurfacePolicyChange::RoleTransition
        } else if self.role == SurfaceRoleKind::Layer {
            if self.namespace != next.namespace || self.blur != next.blur {
                SurfacePolicyChange::LayerRecreate
            } else if self.layer_geometry_changed(next) {
                SurfacePolicyChange::LayerConfigure
            } else if self.padding != next.padding || self.keyboard_mode != next.keyboard_mode {
                SurfacePolicyChange::InputRegionOnly
            } else if self.measurement_changed(next) {
                SurfacePolicyChange::MeasureAgain
            } else if self.visible != next.visible {
                SurfacePolicyChange::VisibilityOnly
            } else {
                SurfacePolicyChange::Noop
            }
        } else if self.window_decorations != next.window_decorations {
            SurfacePolicyChange::WindowRecreate
        } else if self.window_live_state_changed(next) {
            SurfacePolicyChange::WindowLive
        } else if self.padding != next.padding {
            SurfacePolicyChange::InputRegionOnly
        } else if self.measurement_changed(next) {
            SurfacePolicyChange::MeasureAgain
        } else if self.visible != next.visible {
            SurfacePolicyChange::VisibilityOnly
        } else {
            SurfacePolicyChange::Noop
        };

        SurfacePolicyDiff {
            change,
            from_revision: Some(self.revision),
            to_revision: next.revision,
        }
    }

    fn layer_geometry_changed(&self, next: &Self) -> bool {
        self.edge != next.edge
            || self.layer != next.layer
            || self.surface_size != next.surface_size
            || self.width_spans_output != next.width_spans_output
            || self.height_spans_output != next.height_spans_output
            || self.exclusive_zone != next.exclusive_zone
            || self.margins != next.margins
    }

    fn window_live_state_changed(&self, next: &Self) -> bool {
        self.window_title != next.window_title
            || self.window_app_id != next.window_app_id
            || self.window_resizable != next.window_resizable
            || self.content_size != next.content_size
            || self.surface_size != next.surface_size
    }

    fn measurement_changed(&self, next: &Self) -> bool {
        self.content_size.is_none() != next.content_size.is_none()
            || self.surface_size.is_none() != next.surface_size.is_none()
    }
}

/// Assigns monotonically increasing revisions to accepted policy snapshots.
///
/// Candidates are not visible until [`Self::update`] returns; a failed
/// configure can therefore be retried with the same revision instead of
/// advancing the accepted generation speculatively.
#[derive(Debug, Clone, Default)]
pub struct SurfacePolicyGenerator {
    current: Option<SurfacePolicySnapshot>,
    next_revision: u64,
}

#[derive(Debug, Clone)]
pub struct SurfacePolicyUpdate {
    pub previous: Option<SurfacePolicySnapshot>,
    pub current: SurfacePolicySnapshot,
    pub diff: SurfacePolicyDiff,
}

impl SurfacePolicyGenerator {
    pub fn current(&self) -> Option<&SurfacePolicySnapshot> {
        self.current.as_ref()
    }

    pub fn update(&mut self, mut candidate: SurfacePolicySnapshot) -> SurfacePolicyUpdate {
        let previous = self.current.clone();
        let candidate_change = previous
            .as_ref()
            .map(|previous| previous.diff(&candidate).change)
            .unwrap_or(SurfacePolicyChange::MeasureAgain);
        let revision = if let Some(previous) = previous.as_ref()
            && matches!(candidate_change, SurfacePolicyChange::Noop)
        {
            previous.revision
        } else {
            self.next_revision = self.next_revision.saturating_add(1).max(1);
            self.next_revision
        };
        candidate.revision = revision;
        let diff = previous.as_ref().map_or(
            SurfacePolicyDiff {
                change: SurfacePolicyChange::MeasureAgain,
                from_revision: None,
                to_revision: revision,
            },
            |previous| previous.diff(&candidate),
        );
        self.current = Some(candidate.clone());
        SurfacePolicyUpdate {
            previous,
            current: candidate,
            diff,
        }
    }
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

    #[test]
    fn semantic_diff_classifies_creation_geometry_and_input_changes() {
        let previous = SurfacePolicySnapshot {
            revision: 4,
            namespace: "panel".into(),
            content_size: Some((800, 40)),
            surface_size: Some((800, 40)),
            ..SurfacePolicySnapshot::default()
        };

        let mut input = previous.clone();
        input.revision = 5;
        input.padding = [0, 0, 0, 8];
        assert_eq!(
            previous.diff(&input).change,
            SurfacePolicyChange::InputRegionOnly
        );

        let mut geometry = previous.clone();
        geometry.revision = 5;
        geometry.margins = [1, 0, 0, 0];
        assert_eq!(
            previous.diff(&geometry).change,
            SurfacePolicyChange::LayerConfigure
        );

        let mut creation = previous.clone();
        creation.revision = 5;
        creation.blur = true;
        assert_eq!(
            previous.diff(&creation).change,
            SurfacePolicyChange::LayerRecreate
        );
    }

    #[test]
    fn generator_reuses_noop_revision_and_advances_only_on_change() {
        let mut generator = SurfacePolicyGenerator::default();
        let first = generator.update(SurfacePolicySnapshot::default());
        assert_eq!(first.current.revision, 1);
        assert_eq!(first.diff.change, SurfacePolicyChange::MeasureAgain);
        assert_eq!(first.diff.from_revision, None);
        assert_eq!(first.diff.to_revision, 1);

        let same = generator.update(SurfacePolicySnapshot::default());
        assert_eq!(same.current.revision, 1);
        assert_eq!(same.diff.change, SurfacePolicyChange::Noop);
        assert_eq!(same.diff.from_revision, Some(1));
        assert_eq!(same.diff.to_revision, 1);

        let mut changed = SurfacePolicySnapshot::default();
        changed.padding = [0, 0, 0, 4];
        let update = generator.update(changed);
        assert_eq!(update.current.revision, 2);
        assert_eq!(update.diff.change, SurfacePolicyChange::InputRegionOnly);
        assert_eq!(update.diff.from_revision, Some(1));
        assert_eq!(update.diff.to_revision, 2);
    }
}
