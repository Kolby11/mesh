use std::collections::HashMap;
use std::sync::Arc;

use mesh_core_component::style::StyleValue;
use mesh_core_theme::TokenValue;

use super::TransitionEasing;

/// Author-facing style diagnostic emitted while resolving supported shell CSS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyleDiagnostic {
    pub property: String,
    pub selector: Option<String>,
    pub message: String,
}

/// The inherited text properties whose authored state must survive a style
/// resolution and any later targeted restyle.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StylePropertyMask {
    pub color: bool,
    pub font_family: bool,
    pub font_size: bool,
    pub font_weight: bool,
    pub line_height: bool,
}

impl StylePropertyMask {
    /// Mark every inherited field affected by a supported CSS declaration.
    pub fn mark_property(&mut self, property: &str) {
        match property {
            "color" => self.color = true,
            "font" => {
                self.font_family = true;
                self.font_size = true;
                self.font_weight = true;
                self.line_height = true;
            }
            "font-family" => self.font_family = true,
            "font-size" => self.font_size = true,
            "font-weight" => self.font_weight = true,
            "line-height" => self.line_height = true,
            _ => {}
        }
    }

    /// True when every field set by this mask is already set by `accumulated`.
    pub fn adds_nothing_to(self, accumulated: Self) -> bool {
        (!self.color || accumulated.color)
            && (!self.font_family || accumulated.font_family)
            && (!self.font_size || accumulated.font_size)
            && (!self.font_weight || accumulated.font_weight)
            && (!self.line_height || accumulated.line_height)
    }
}

impl std::ops::BitOrAssign for StylePropertyMask {
    fn bitor_assign(&mut self, rhs: Self) {
        self.color |= rhs.color;
        self.font_family |= rhs.font_family;
        self.font_size |= rhs.font_size;
        self.font_weight |= rhs.font_weight;
        self.line_height |= rhs.line_height;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyleProfileStatus {
    Implemented,
    DiagnosticOnly,
    Deferred,
    OutOfScope,
}

impl StyleProfileStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Implemented => "implemented",
            Self::DiagnosticOnly => "diagnostic-only",
            Self::Deferred => "deferred",
            Self::OutOfScope => "out-of-scope",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StyleProfileProperty {
    pub property: &'static str,
    pub category: &'static str,
    pub status: StyleProfileStatus,
}

const STYLE_PROFILE_PROPERTIES: &[StyleProfileProperty] = &[
    StyleProfileProperty {
        property: "background",
        category: "color",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "background-color",
        category: "color",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "color",
        category: "color",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "border",
        category: "border",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "border-color",
        category: "border",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "border-width",
        category: "border",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "border-top-width",
        category: "border",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "border-right-width",
        category: "border",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "border-bottom-width",
        category: "border",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "border-left-width",
        category: "border",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "border-radius",
        category: "radius",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "corner-shape",
        category: "radius",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "corner-top-left-shape",
        category: "radius",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "corner-top-right-shape",
        category: "radius",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "corner-bottom-right-shape",
        category: "radius",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "corner-bottom-left-shape",
        category: "radius",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "border-top-left-radius",
        category: "radius",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "border-top-right-radius",
        category: "radius",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "border-bottom-right-radius",
        category: "radius",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "border-bottom-left-radius",
        category: "radius",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "aspect-ratio",
        category: "layout",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "display",
        category: "layout",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "visibility",
        category: "layout",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "opacity",
        category: "opacity",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "overflow",
        category: "layout",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "overflow-x",
        category: "layout",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "overflow-y",
        category: "layout",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "width",
        category: "size",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "height",
        category: "size",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "min-width",
        category: "size",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "max-width",
        category: "size",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "min-height",
        category: "size",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "max-height",
        category: "size",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "padding",
        category: "spacing",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "padding-top",
        category: "spacing",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "padding-right",
        category: "spacing",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "padding-bottom",
        category: "spacing",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "padding-left",
        category: "spacing",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "padding-x",
        category: "spacing",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "padding-y",
        category: "spacing",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "padding-inline",
        category: "spacing",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "padding-block",
        category: "spacing",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "margin",
        category: "spacing",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "margin-top",
        category: "spacing",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "margin-right",
        category: "spacing",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "margin-bottom",
        category: "spacing",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "margin-left",
        category: "spacing",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "margin-x",
        category: "spacing",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "margin-y",
        category: "spacing",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "margin-inline",
        category: "spacing",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "margin-block",
        category: "spacing",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "exclusive-zone",
        category: "layout",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "font",
        category: "font",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "font-family",
        category: "font",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "font-size",
        category: "font",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "font-weight",
        category: "font",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "font-style",
        category: "font",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "line-height",
        category: "font",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "letter-spacing",
        category: "font",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "text-align",
        category: "font",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "text-overflow",
        category: "font",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "direction",
        category: "font",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "flex",
        category: "layout",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "flex-direction",
        category: "layout",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "flex-wrap",
        category: "layout",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "flex-grow",
        category: "layout",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "flex-shrink",
        category: "layout",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "flex-basis",
        category: "layout",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "justify-content",
        category: "layout",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "align-items",
        category: "layout",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "align-self",
        category: "layout",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "align-content",
        category: "layout",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "gap",
        category: "layout",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "row-gap",
        category: "layout",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "column-gap",
        category: "layout",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "gap-x",
        category: "layout",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "position",
        category: "layout",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "z-index",
        category: "layout",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "inset",
        category: "layout",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "top",
        category: "layout",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "right",
        category: "layout",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "bottom",
        category: "layout",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "left",
        category: "layout",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "transition",
        category: "transition",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "transition-property",
        category: "transition",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "transition-duration",
        category: "transition",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "transition-delay",
        category: "transition",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "transition-timing-function",
        category: "transition",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "animation",
        category: "animation",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "animation-name",
        category: "animation",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "animation-duration",
        category: "animation",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "animation-delay",
        category: "animation",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "animation-timing-function",
        category: "animation",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "animation-iteration-count",
        category: "animation",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "animation-direction",
        category: "animation",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "animation-fill-mode",
        category: "animation",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "animation-play-state",
        category: "animation",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "transform",
        category: "transform",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "transform-origin",
        category: "transform",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "box-shadow",
        category: "shadow",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "outline",
        category: "outline",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "outline-width",
        category: "outline",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "outline-color",
        category: "outline",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "outline-offset",
        category: "outline",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "outline-style",
        category: "outline",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "filter",
        category: "filter",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "backdrop-filter",
        category: "filter",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "mix-blend-mode",
        category: "compositing",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "border-style",
        category: "border",
        status: StyleProfileStatus::DiagnosticOnly,
    },
    StyleProfileProperty {
        property: "background-image",
        category: "image",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "linear-gradient",
        category: "gradient",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "radial-gradient",
        category: "gradient",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "grid-template-columns",
        category: "layout",
        status: StyleProfileStatus::OutOfScope,
    },
    StyleProfileProperty {
        property: "float",
        category: "layout",
        status: StyleProfileStatus::OutOfScope,
    },
    StyleProfileProperty {
        property: "white-space",
        category: "font",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "container-type",
        category: "layout",
        status: StyleProfileStatus::OutOfScope,
    },
    StyleProfileProperty {
        property: "text-wrap",
        category: "font",
        status: StyleProfileStatus::OutOfScope,
    },
    StyleProfileProperty {
        property: "tooltip-anchor",
        category: "tooltip",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "tooltip-offset",
        category: "tooltip",
        status: StyleProfileStatus::Implemented,
    },
];

pub fn supported_css_properties() -> &'static [&'static str] {
    super::resolve::lowered_css_properties()
}

pub fn is_supported_css_property(property: &str) -> bool {
    property.starts_with("--") || supported_css_properties().contains(&property)
}

pub fn style_profile_properties() -> &'static [StyleProfileProperty] {
    STYLE_PROFILE_PROPERTIES
}

pub fn style_profile_status(property: &str) -> Option<StyleProfileStatus> {
    if property.starts_with("--") {
        return Some(StyleProfileStatus::Implemented);
    }

    STYLE_PROFILE_PROPERTIES
        .iter()
        .find(|entry| entry.property == property)
        .map(|entry| entry.status)
}

pub fn is_transition_safe_keyframe_property(property: &str) -> bool {
    mesh_core_component::style::is_transition_safe_keyframe_property(property)
}

/// Where a tooltip should appear relative to the hovered element.
///
/// Set via the `tooltip-anchor` CSS property. Elements use this to declare
/// their preferred tooltip placement. The shell applies screen-edge avoidance
/// automatically — if the preferred placement would overflow, it flips to the
/// opposite side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TooltipAnchor {
    /// Use the shell's global default positioning strategy.
    #[default]
    Auto,
    /// Centered below the element (preferred), flipping above on overflow.
    Bottom,
    /// Centered above the element (preferred), flipping below on overflow.
    Top,
    /// Left of the element (preferred), flipping right on overflow.
    Left,
    /// Right of the element (preferred), flipping left on overflow.
    Right,
    /// Place the tooltip near the cursor.
    Cursor,
}

/// Whether a layer surface reserves the measured root box at its anchored
/// edge. This is intentionally a CSS property on the component root rather
/// than a manifest number: the reservation follows the box as CSS changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SurfaceExclusiveZone {
    /// Reserve the root's border box plus its margin on the anchored edge.
    #[default]
    Auto,
    /// Do not reserve compositor space; the surface is an overlay/HUD.
    None,
    /// Ignore every other surface's exclusive zone and extend to the anchored
    /// edge (layer-shell `-1`): wallpapers and full-output backdrops.
    Ignore,
}

impl SurfaceExclusiveZone {
    pub fn from_css(value: &str) -> Option<Self> {
        match value.trim() {
            "auto" => Some(Self::Auto),
            "none" => Some(Self::None),
            "ignore" => Some(Self::Ignore),
            _ => None,
        }
    }
}

impl TooltipAnchor {
    pub fn from_css(value: &str) -> Option<Self> {
        match value.trim() {
            "auto" => Some(Self::Auto),
            "bottom" => Some(Self::Bottom),
            "top" => Some(Self::Top),
            "left" => Some(Self::Left),
            "right" => Some(Self::Right),
            "cursor" => Some(Self::Cursor),
            _ => None,
        }
    }
}

/// A node's CSS custom properties: immutable, shared between a parent and
/// every descendant that declares none of its own, and fingerprinted once.
///
/// Cloning is a reference-count increment. Two values are equal when they
/// hold the same entries; [`fingerprint`](Self::fingerprint) is independent of
/// map iteration order.
#[derive(Clone)]
pub struct CustomProperties(Arc<CustomPropertiesInner>);

struct CustomPropertiesInner {
    map: HashMap<String, StyleValue>,
    fingerprint: u64,
}

impl CustomProperties {
    pub fn new(map: HashMap<String, StyleValue>) -> Self {
        let fingerprint = custom_properties_fingerprint(&map);
        Self(Arc::new(CustomPropertiesInner { map, fingerprint }))
    }

    /// A stable hash of the entries, computed when the value was built.
    pub fn fingerprint(&self) -> u64 {
        self.0.fingerprint
    }

    pub fn ptr_eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

fn custom_properties_fingerprint(map: &HashMap<String, StyleValue>) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut entries: Vec<_> = map.iter().collect();
    entries.sort_unstable_by(|left, right| left.0.cmp(right.0));
    let mut hasher = std::hash::DefaultHasher::new();
    entries.len().hash(&mut hasher);
    for (key, value) in entries {
        key.hash(&mut hasher);
        value.hash(&mut hasher);
    }
    hasher.finish()
}

impl Default for CustomProperties {
    fn default() -> Self {
        static EMPTY: std::sync::LazyLock<CustomProperties> =
            std::sync::LazyLock::new(|| CustomProperties::new(HashMap::new()));
        EMPTY.clone()
    }
}

impl std::ops::Deref for CustomProperties {
    type Target = HashMap<String, StyleValue>;

    fn deref(&self) -> &Self::Target {
        &self.0.map
    }
}

impl From<HashMap<String, StyleValue>> for CustomProperties {
    fn from(map: HashMap<String, StyleValue>) -> Self {
        Self::new(map)
    }
}

impl PartialEq for CustomProperties {
    fn eq(&self, other: &Self) -> bool {
        self.ptr_eq(other)
            || (self.0.fingerprint == other.0.fingerprint && self.0.map == other.0.map)
    }
}

impl std::fmt::Debug for CustomProperties {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.map.fmt(formatter)
    }
}

/// Fully resolved style for a widget node.
#[derive(Debug, Clone, PartialEq)]
pub struct ComputedStyle {
    /// CSS custom properties after this node's cascade. Keeping them on the
    /// immutable style snapshot lets both initial tree construction and
    /// targeted restyles pass inherited variables without a separate side
    /// channel.
    pub custom_properties: CustomProperties,
    /// Authored inherited properties that won the node's cascade.
    pub explicit_properties: StylePropertyMask,
    /// Inherited properties copied from the parent during this resolution.
    pub inherited_properties: StylePropertyMask,
    pub width: Dimension,
    pub height: Dimension,
    pub min_width: Dimension,
    pub max_width: Dimension,
    pub min_height: Dimension,
    pub max_height: Dimension,
    pub padding: Edges,
    pub margin: Edges,
    /// Layer-surface reservation policy for the component root.
    pub surface_exclusive_zone: SurfaceExclusiveZone,
    pub border_width: Edges,
    pub background_color: Color,
    pub background_paint: BackgroundPaint,
    pub border_color: Color,
    pub border_radius: Corners,
    pub corner_shape: CornerShapes,
    pub opacity: f32,
    pub transform: Transform2D,
    pub transform_origin: TransformOrigin,
    pub box_shadow: BoxShadow,
    /// Shadows after the first, in CSS front-to-back order.
    pub additional_box_shadows: Vec<BoxShadow>,
    pub outline: Outline,
    pub filter: VisualFilter,
    pub backdrop_filter: VisualFilter,
    pub mix_blend_mode: BlendMode,
    pub transitions: Vec<TransitionStyle>,
    /// The independently authored transition longhand lists used to rebuild
    /// `transitions` while declarations are being applied. Keeping the source
    /// lists preserves their values when a later property list changes length.
    pub transition_longhands: TransitionLonghands,
    pub animations: Vec<AnimationStyle>,
    pub overflow_x: Overflow,
    pub overflow_y: Overflow,
    pub font_family: Arc<str>,
    pub font_size: f32,
    pub font_weight: u16,
    pub color: Color,
    pub text_align: TextAlign,
    pub line_height: f32,
    pub font_style: FontStyle,
    pub letter_spacing: f32,
    pub text_overflow: TextOverflow,
    pub white_space: WhiteSpace,
    pub text_direction: TextDirection,
    pub display: Display,
    pub visibility: Visibility,
    pub direction: FlexDirection,
    pub justify_content: JustifyContent,
    pub align_items: AlignItems,
    pub align_content: AlignContent,
    pub gap: f32,
    /// `width / height` the box keeps when only one axis is determined, so a
    /// square control can take its size from the container instead of pinning
    /// both axes. `None` is CSS `aspect-ratio: auto`.
    pub aspect_ratio: Option<f32>,
    pub flex_grow: f32,
    pub flex_shrink: f32,
    pub flex_basis: Dimension,
    pub flex_wrap: FlexWrap,
    pub align_self: AlignSelf,
    pub position: Position,
    pub z_index: i32,
    pub inset_top: Option<f32>,
    pub inset_right: Option<f32>,
    pub inset_bottom: Option<f32>,
    pub inset_left: Option<f32>,
    /// Variable-font axis values for icon font packs (Material Symbols et
    /// al.). Sourced from CSS custom properties `--icon-fill`,
    /// `--icon-weight`, `--icon-grade`, `--icon-optical-size`. `None`
    /// means "use the font's default for this axis"; `Some` overrides it.
    /// Silently ignored when the resolved icon is a file (SVG/PNG) or
    /// when the font pack doesn't expose the axis.
    pub icon_fill: Option<f32>,
    pub icon_weight: Option<f32>,
    pub icon_grade: Option<f32>,
    pub icon_optical_size: Option<f32>,
    /// Per-element tooltip placement override. Set via the `tooltip-anchor`
    /// CSS property. `Auto` defers to the shell's global default.
    pub tooltip_anchor: TooltipAnchor,
    /// Per-element tooltip offset override in CSS pixels (horizontal, vertical).
    /// Set via the `tooltip-offset` CSS property. `None` uses the shell default.
    pub tooltip_offset: Option<(f32, f32)>,
    /// Set when a transition wrote an interpolated sample over this style's
    /// animatable values. Resolution builds a fresh style with it cleared, so
    /// while it is set nothing has restyled the node since that sample and its
    /// animatable values are not the authored ones — a frame that reuses the
    /// retained style must not read them as a new transition target.
    pub animated_sample: bool,
}

impl ComputedStyle {
    /// Whether `other` feeds layout the same inputs as `self`. Fields that
    /// only change painting, animation setup or inheritance bookkeeping are
    /// ignored; any other field counts, so a newly added field is layout
    /// relevant until listed here.
    pub fn same_layout_inputs(&self, other: &Self) -> bool {
        let mut probe = other.clone();
        probe.custom_properties = self.custom_properties.clone();
        probe.explicit_properties = self.explicit_properties;
        probe.inherited_properties = self.inherited_properties;
        probe.background_color = self.background_color;
        probe.background_paint = self.background_paint.clone();
        probe.border_color = self.border_color;
        probe.border_radius = self.border_radius;
        probe.corner_shape = self.corner_shape;
        probe.opacity = self.opacity;
        probe.transform = self.transform;
        probe.transform_origin = self.transform_origin;
        probe.box_shadow = self.box_shadow;
        probe.additional_box_shadows = self.additional_box_shadows.clone();
        probe.outline = self.outline;
        probe.filter = self.filter.clone();
        probe.backdrop_filter = self.backdrop_filter.clone();
        probe.mix_blend_mode = self.mix_blend_mode;
        probe.transitions = self.transitions.clone();
        probe.transition_longhands = self.transition_longhands.clone();
        probe.animations = self.animations.clone();
        probe.color = self.color;
        probe.z_index = self.z_index;
        probe.visibility = self.visibility;
        probe.tooltip_anchor = self.tooltip_anchor;
        probe.tooltip_offset = self.tooltip_offset;
        probe.animated_sample = self.animated_sample;
        self == &probe
    }
}

impl Default for ComputedStyle {
    fn default() -> Self {
        Self {
            custom_properties: CustomProperties::default(),
            explicit_properties: StylePropertyMask::default(),
            inherited_properties: StylePropertyMask::default(),
            width: Dimension::Auto,
            height: Dimension::Auto,
            min_width: Dimension::Auto,
            max_width: Dimension::Auto,
            min_height: Dimension::Auto,
            max_height: Dimension::Auto,
            padding: Edges::zero(),
            margin: Edges::zero(),
            surface_exclusive_zone: SurfaceExclusiveZone::Auto,
            border_width: Edges::zero(),
            background_color: Color::TRANSPARENT,
            background_paint: BackgroundPaint::None,
            border_color: Color::TRANSPARENT,
            border_radius: Corners::zero(),
            corner_shape: CornerShapes::default(),
            opacity: 1.0,
            transform: Transform2D::IDENTITY,
            transform_origin: TransformOrigin::default(),
            box_shadow: BoxShadow::NONE,
            additional_box_shadows: Vec::new(),
            outline: Outline::NONE,
            filter: VisualFilter::NONE,
            backdrop_filter: VisualFilter::NONE,
            mix_blend_mode: BlendMode::Normal,
            transitions: vec![TransitionStyle::default()],
            transition_longhands: TransitionLonghands::default(),
            animations: vec![AnimationStyle::default()],
            overflow_x: Overflow::Visible,
            overflow_y: Overflow::Visible,
            font_family: Arc::from("Inter"),
            font_size: 14.0,
            font_weight: 400,
            color: Color::WHITE,
            text_align: TextAlign::Left,
            line_height: 1.4,
            font_style: FontStyle::Normal,
            letter_spacing: 0.0,
            text_overflow: TextOverflow::Clip,
            white_space: WhiteSpace::Normal,
            text_direction: TextDirection::Ltr,
            display: Display::Flex,
            visibility: Visibility::Visible,
            direction: FlexDirection::Row,
            justify_content: JustifyContent::Start,
            align_items: AlignItems::Stretch,
            align_content: AlignContent::Stretch,
            gap: 0.0,
            aspect_ratio: None,
            flex_grow: 0.0,
            flex_shrink: 1.0,
            flex_basis: Dimension::Auto,
            flex_wrap: FlexWrap::NoWrap,
            align_self: AlignSelf::Auto,
            position: Position::Static,
            z_index: 0,
            inset_top: None,
            inset_right: None,
            inset_bottom: None,
            inset_left: None,
            icon_fill: None,
            icon_weight: None,
            icon_grade: None,
            icon_optical_size: None,
            tooltip_anchor: TooltipAnchor::Auto,
            tooltip_offset: None,
            animated_sample: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Dimension {
    Auto,
    Px(f32),
    Percent(f32),
    Content,
    Fit,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Edges {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl Edges {
    pub fn zero() -> Self {
        Self {
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
            left: 0.0,
        }
    }

    pub fn all(value: f32) -> Self {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }

    pub fn horizontal(&self) -> f32 {
        self.left + self.right
    }

    pub fn vertical(&self) -> f32 {
        self.top + self.bottom
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Corners {
    pub top_left: f32,
    pub top_right: f32,
    pub bottom_right: f32,
    pub bottom_left: f32,
}

impl Corners {
    pub fn zero() -> Self {
        Self {
            top_left: 0.0,
            top_right: 0.0,
            bottom_right: 0.0,
            bottom_left: 0.0,
        }
    }

    pub fn all(value: f32) -> Self {
        Self {
            top_left: value,
            top_right: value,
            bottom_right: value,
            bottom_left: value,
        }
    }
}

/// CSS `corner-shape` keyword for one corner. `Scoop` curves the corner
/// inward: its arc is centred on the box's corner point, so the quarter disc
/// of the `border-radius` there is removed from the box rather than added.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum CornerShape {
    #[default]
    Round,
    Scoop,
}

impl CornerShape {
    /// Unsupported keywords (`bevel`, `notch`, `squircle`, `superellipse()`)
    /// resolve to `round`.
    pub fn from_css(value: &str) -> Self {
        match value.trim() {
            "scoop" => Self::Scoop,
            _ => Self::Round,
        }
    }
}

/// Per-corner [`CornerShape`], in the same order as [`Corners`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct CornerShapes {
    pub top_left: CornerShape,
    pub top_right: CornerShape,
    pub bottom_right: CornerShape,
    pub bottom_left: CornerShape,
}

impl CornerShapes {
    /// The `corner-shape` shorthand: one to four keywords expanding like
    /// `border-radius`.
    pub fn from_css(value: &str) -> Self {
        let shapes: Vec<_> = value.split_whitespace().map(CornerShape::from_css).collect();
        let [top_left, top_right, bottom_right, bottom_left] = match shapes.as_slice() {
            [all] => [*all; 4],
            [a, b] => [*a, *b, *a, *b],
            [a, b, c] => [*a, *b, *c, *b],
            [a, b, c, d, ..] => [*a, *b, *c, *d],
            [] => [CornerShape::Round; 4],
        };
        Self {
            top_left,
            top_right,
            bottom_right,
            bottom_left,
        }
    }

    /// Whether any corner with a radius in `radii` is scooped.
    pub fn scoops(self, radii: Corners) -> bool {
        (self.top_left == CornerShape::Scoop && radii.top_left > 0.0)
            || (self.top_right == CornerShape::Scoop && radii.top_right > 0.0)
            || (self.bottom_right == CornerShape::Scoop && radii.bottom_right > 0.0)
            || (self.bottom_left == CornerShape::Scoop && radii.bottom_left > 0.0)
    }

    /// `radii` with every scooped corner squared off: a scooped box paints as
    /// this shape under a clip that removes the scoops.
    pub fn round_radii(self, radii: Corners) -> Corners {
        let keep = |shape: CornerShape, radius: f32| {
            if shape == CornerShape::Scoop { 0.0 } else { radius }
        };
        Corners {
            top_left: keep(self.top_left, radii.top_left),
            top_right: keep(self.top_right, radii.top_right),
            bottom_right: keep(self.bottom_right, radii.bottom_right),
            bottom_left: keep(self.bottom_left, radii.bottom_left),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Overflow {
    Visible,
    Hidden,
    Auto,
    Scroll,
}

impl Overflow {
    pub fn clips_contents(self) -> bool {
        !matches!(self, Self::Visible)
    }

    /// Whether this axis lets the user scroll to reveal clipped content.
    /// `Hidden` clips like `Auto`/`Scroll` but never scrolls.
    pub fn is_scrollable(self) -> bool {
        matches!(self, Self::Auto | Self::Scroll)
    }

    pub fn shows_scrollbar_when_overflowing(self) -> bool {
        self.is_scrollable()
    }

    pub fn always_shows_scrollbar(self) -> bool {
        matches!(self, Self::Scroll)
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct StyleContext {
    pub container_width: f32,
    pub container_height: f32,
}

/// Names every non-`all` field of [`TransitionProperties`] exactly once, so the
/// set operations below cannot drift from the struct when a property is added.
macro_rules! for_each_transition_property {
    ($callback:ident) => {
        $callback!(
            border_radius,
            border_width,
            opacity,
            background_color,
            border_color,
            color,
            width,
            height,
            min_width,
            max_width,
            min_height,
            max_height,
            padding,
            margin,
            transform,
            box_shadow,
            filter,
            backdrop_filter,
            font_size,
            letter_spacing,
            line_height,
            gap,
            inset_top,
            inset_right,
            inset_bottom,
            inset_left
        )
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct TransitionProperties {
    pub all: bool,
    pub border_radius: bool,
    pub border_width: bool,
    pub opacity: bool,
    pub background_color: bool,
    pub border_color: bool,
    pub color: bool,
    pub width: bool,
    pub height: bool,
    pub min_width: bool,
    pub max_width: bool,
    pub min_height: bool,
    pub max_height: bool,
    pub padding: bool,
    pub margin: bool,
    pub transform: bool,
    pub box_shadow: bool,
    pub filter: bool,
    pub backdrop_filter: bool,
    pub font_size: bool,
    pub letter_spacing: bool,
    pub line_height: bool,
    pub gap: bool,
    pub inset_top: bool,
    pub inset_right: bool,
    pub inset_bottom: bool,
    pub inset_left: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationPropertyBucket {
    None,
    PaintOnly,
    LayerEffect,
    LayoutAffecting,
}

impl TransitionProperties {
    pub fn none() -> Self {
        Self::default()
    }

    pub fn all() -> Self {
        Self {
            all: true,
            border_radius: true,
            border_width: true,
            opacity: true,
            background_color: true,
            border_color: true,
            color: true,
            width: true,
            height: true,
            min_width: true,
            max_width: true,
            min_height: true,
            max_height: true,
            padding: true,
            margin: true,
            transform: true,
            box_shadow: true,
            filter: true,
            backdrop_filter: true,
            font_size: true,
            letter_spacing: true,
            line_height: true,
            gap: true,
            inset_top: true,
            inset_right: true,
            inset_bottom: true,
            inset_left: true,
        }
    }

    /// No property animates under this mask.
    pub fn is_empty(self) -> bool {
        macro_rules! none_set {
            ($($field:ident),*) => { !(self.all $(|| self.$field)*) };
        }
        for_each_transition_property!(none_set)
    }

    /// Replace the `all` shorthand with the individual flags it stands for, so
    /// the mask can take part in set operations without `all` re-enabling a
    /// property that was just removed.
    fn expanded(self) -> Self {
        if self.all {
            Self {
                all: false,
                ..Self::all()
            }
        } else {
            self
        }
    }

    /// Every property named by either mask.
    pub fn union(self, other: Self) -> Self {
        if self.all || other.all {
            return Self::all();
        }
        let other = other.expanded();
        let mut merged = self.expanded();
        macro_rules! merge {
            ($($field:ident),*) => { $(merged.$field |= other.$field;)* };
        }
        for_each_transition_property!(merge);
        merged
    }

    /// The properties both masks name.
    pub fn intersection(self, other: Self) -> Self {
        self.difference(self.difference(other))
    }

    /// The animatable properties a declaration of `property` writes. A
    /// longhand such as `padding-left` names the property it is part of; a
    /// shorthand such as `border` names every property it sets. `all` names
    /// everything, and a property that is not animatable names nothing.
    pub fn of_property(property: &str) -> Self {
        let mut properties = Self::none();
        match property {
            "all" => return Self::all(),
            "opacity" => properties.opacity = true,
            "background" | "background-color" => properties.background_color = true,
            "color" => properties.color = true,
            "width" => properties.width = true,
            "height" => properties.height = true,
            "min-width" => properties.min_width = true,
            "max-width" => properties.max_width = true,
            "min-height" => properties.min_height = true,
            "max-height" => properties.max_height = true,
            "transform" => properties.transform = true,
            "box-shadow" => properties.box_shadow = true,
            "filter" => properties.filter = true,
            "backdrop-filter" => properties.backdrop_filter = true,
            "font" => {
                properties.font_size = true;
                properties.line_height = true;
            }
            "font-size" => properties.font_size = true,
            "letter-spacing" => properties.letter_spacing = true,
            "line-height" => properties.line_height = true,
            "gap" | "column-gap" | "row-gap" | "gap-x" => properties.gap = true,
            "top" => properties.inset_top = true,
            "right" => properties.inset_right = true,
            "bottom" => properties.inset_bottom = true,
            "left" => properties.inset_left = true,
            "inset" => {
                properties.inset_top = true;
                properties.inset_right = true;
                properties.inset_bottom = true;
                properties.inset_left = true;
            }
            _ if property.starts_with("padding") => properties.padding = true,
            _ if property.starts_with("margin") => properties.margin = true,
            _ if property.starts_with("border") => {
                if property.ends_with("radius") {
                    properties.border_radius = true;
                } else if property.ends_with("width") {
                    properties.border_width = true;
                } else if property.ends_with("color") {
                    properties.border_color = true;
                } else if matches!(
                    property,
                    "border" | "border-top" | "border-right" | "border-bottom" | "border-left"
                ) {
                    properties.border_width = true;
                    properties.border_color = true;
                }
            }
            _ => {}
        }
        properties
    }

    /// The properties this mask names that `other` does not.
    pub fn difference(self, other: Self) -> Self {
        if other.all {
            return Self::none();
        }
        let mut remaining = self.expanded();
        macro_rules! subtract {
            ($($field:ident),*) => { $(remaining.$field &= !other.$field;)* };
        }
        for_each_transition_property!(subtract);
        remaining
    }

    pub fn animates_border_radius(self) -> bool {
        self.all || self.border_radius
    }

    pub fn animates_border_width(self) -> bool {
        self.all || self.border_width
    }

    pub fn animates_opacity(self) -> bool {
        self.all || self.opacity
    }

    pub fn animates_background_color(self) -> bool {
        self.all || self.background_color
    }

    pub fn animates_border_color(self) -> bool {
        self.all || self.border_color
    }

    pub fn animates_color(self) -> bool {
        self.all || self.color
    }

    pub fn animates_width(self) -> bool {
        self.all || self.width
    }

    pub fn animates_height(self) -> bool {
        self.all || self.height
    }

    pub fn animates_padding(self) -> bool {
        self.all || self.padding
    }

    pub fn animates_margin(self) -> bool {
        self.all || self.margin
    }

    pub fn animates_transform(self) -> bool {
        self.all || self.transform
    }

    pub fn animates_box_shadow(self) -> bool {
        self.all || self.box_shadow
    }

    pub fn animates_filter(self) -> bool {
        self.all || self.filter
    }

    pub fn animates_backdrop_filter(self) -> bool {
        self.all || self.backdrop_filter
    }

    pub fn animates_min_width(self) -> bool {
        self.all || self.min_width
    }

    pub fn animates_max_width(self) -> bool {
        self.all || self.max_width
    }

    pub fn animates_min_height(self) -> bool {
        self.all || self.min_height
    }

    pub fn animates_max_height(self) -> bool {
        self.all || self.max_height
    }

    pub fn animates_font_size(self) -> bool {
        self.all || self.font_size
    }

    pub fn animates_letter_spacing(self) -> bool {
        self.all || self.letter_spacing
    }

    pub fn animates_line_height(self) -> bool {
        self.all || self.line_height
    }

    pub fn animates_gap(self) -> bool {
        self.all || self.gap
    }

    pub fn animates_inset_top(self) -> bool {
        self.all || self.inset_top
    }

    pub fn animates_inset_right(self) -> bool {
        self.all || self.inset_right
    }

    pub fn animates_inset_bottom(self) -> bool {
        self.all || self.inset_bottom
    }

    pub fn animates_inset_left(self) -> bool {
        self.all || self.inset_left
    }

    pub fn affects_layout(self) -> bool {
        self.all
            || self.border_width
            || self.width
            || self.height
            || self.min_width
            || self.max_width
            || self.min_height
            || self.max_height
            || self.padding
            || self.margin
            || self.font_size
            || self.letter_spacing
            || self.line_height
            || self.gap
            || self.inset_top
            || self.inset_right
            || self.inset_bottom
            || self.inset_left
    }

    pub fn animation_bucket(self) -> AnimationPropertyBucket {
        if self.all
            || self.width
            || self.height
            || self.min_width
            || self.max_width
            || self.min_height
            || self.max_height
            || self.padding
            || self.margin
            || self.font_size
            || self.letter_spacing
            || self.line_height
            || self.gap
            || self.inset_top
            || self.inset_right
            || self.inset_bottom
            || self.inset_left
        {
            return AnimationPropertyBucket::LayoutAffecting;
        }

        if self.box_shadow || self.filter || self.backdrop_filter {
            return AnimationPropertyBucket::LayerEffect;
        }

        if self.border_radius
            || self.border_width
            || self.opacity
            || self.background_color
            || self.border_color
            || self.color
            || self.transform
        {
            return AnimationPropertyBucket::PaintOnly;
        }

        AnimationPropertyBucket::None
    }

    pub fn has_paint_only_animation(self) -> bool {
        self.animation_bucket() == AnimationPropertyBucket::PaintOnly
    }

    pub fn has_layer_effect_animation(self) -> bool {
        self.animation_bucket() == AnimationPropertyBucket::LayerEffect
    }

    pub fn has_layout_affecting_animation(self) -> bool {
        self.animation_bucket() == AnimationPropertyBucket::LayoutAffecting
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform2D {
    pub translate_x: f32,
    pub translate_y: f32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub rotation: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoxShadow {
    pub offset_x: f32,
    pub offset_y: f32,
    pub blur_radius: f32,
    pub spread_radius: f32,
    pub color: Color,
    pub inset: bool,
}

impl BoxShadow {
    pub const NONE: Self = Self {
        offset_x: 0.0,
        offset_y: 0.0,
        blur_radius: 0.0,
        spread_radius: 0.0,
        color: Color::TRANSPARENT,
        inset: false,
    };

    pub fn is_none(self) -> bool {
        self.color.a == 0
            || (self.offset_x == 0.0
                && self.offset_y == 0.0
                && self.blur_radius == 0.0
                && self.spread_radius == 0.0)
    }
}

impl Default for BoxShadow {
    fn default() -> Self {
        Self::NONE
    }
}

/// CSS `outline`: a solid ring `offset` outside the border box. It never
/// affects layout and follows the border radius, grown by the offset.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Outline {
    pub width: f32,
    pub offset: f32,
    pub color: Color,
}

impl Outline {
    pub const NONE: Self = Self {
        width: 0.0,
        offset: 0.0,
        color: Color::TRANSPARENT,
    };

    pub fn is_none(self) -> bool {
        self.width <= 0.0 || self.color.a == 0
    }

    /// How far the ring reaches outside the border box.
    pub fn extent(self) -> f32 {
        if self.is_none() {
            0.0
        } else {
            (self.offset + self.width).max(0.0)
        }
    }
}

impl Default for Outline {
    fn default() -> Self {
        Self::NONE
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub enum BackgroundPaint {
    #[default]
    None,
    Image(StyleImageSource),
    LinearGradient(StyleLinearGradient),
    MultiStopLinearGradient(Vec<Color>),
    /// An angle-directed, positioned or radial gradient. The plain
    /// `to bottom` forms keep the dedicated variants above so existing
    /// themes keep their render path; everything else lowers here.
    Gradient(StyleGradient),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StyleImageSource {
    pub path: String,
}

/// One gradient colour stop with its resolved position as a fraction of the
/// gradient axis in `[0, 1]`. Unpinned stops take the CSS defaults (first 0,
/// last 1, middles evenly between their pinned neighbours), and positions at
/// or below a predecessor pin equal to it (the CSS hard-band rule).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GradientStop {
    pub color: Color,
    pub position: f32,
}

/// The gradient axis: an angle-directed line for linear gradients, or a
/// shape/extent/centre triple for radial ones.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GradientGeometry {
    /// CSS angle: `0deg` points up, increasing clockwise.
    Linear { angle_degrees: f32 },
    Radial {
        shape: RadialShape,
        extent: RadialExtent,
        /// Centre as fractions of the box width/height.
        center: (f32, f32),
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RadialShape {
    #[default]
    Ellipse,
    Circle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RadialExtent {
    ClosestSide,
    ClosestCorner,
    FarthestSide,
    #[default]
    FarthestCorner,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StyleGradient {
    pub geometry: GradientGeometry,
    pub stops: Vec<GradientStop>,
}

impl Default for StyleGradient {
    fn default() -> Self {
        Self {
            geometry: GradientGeometry::Linear {
                angle_degrees: 180.0,
            },
            stops: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StyleLinearGradient {
    pub from: Color,
    pub to: Color,
}

impl Default for StyleLinearGradient {
    fn default() -> Self {
        Self {
            from: Color::TRANSPARENT,
            to: Color::TRANSPARENT,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VisualFilter {
    pub blur_radius: f32,
    /// Set by `refract()`: the element samples the shell backdrop source
    /// instead of asking the compositor for blur (spec 04 §11). Declaring
    /// `refract(0px 0px)` paints the source unrefracted.
    pub refract: bool,
    /// `refract(<depth> <edge>)`: maximum displacement in logical pixels at the
    /// rim, and the width of the rim band it fades over.
    pub refract_depth: f32,
    pub refract_edge: f32,
    /// `dispersion(<px>)`: extra displacement of the red and blue channels at
    /// the rim, for chromatic fringing.
    pub dispersion: f32,
}

impl VisualFilter {
    pub const NONE: Self = Self {
        blur_radius: 0.0,
        refract: false,
        refract_depth: 0.0,
        refract_edge: 0.0,
        dispersion: 0.0,
    };

    pub const fn blur(blur_radius: f32) -> Self {
        Self {
            blur_radius,
            ..Self::NONE
        }
    }

    pub fn is_none(self) -> bool {
        self.blur_radius <= 0.0 && !self.has_refraction()
    }

    /// Whether this backdrop samples the shell backdrop source.
    pub fn has_refraction(self) -> bool {
        self.refract
    }
}

impl Default for VisualFilter {
    fn default() -> Self {
        Self::NONE
    }
}

impl Transform2D {
    pub const IDENTITY: Self = Self {
        translate_x: 0.0,
        translate_y: 0.0,
        scale_x: 1.0,
        scale_y: 1.0,
        rotation: 0.0,
    };

    pub fn is_identity(&self) -> bool {
        *self == Self::IDENTITY
    }
}

impl Default for Transform2D {
    fn default() -> Self {
        Self::IDENTITY
    }
}

/// A single axis value for `transform-origin`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TransformOriginValue {
    Percent(f32),
    Px(f32),
}

impl TransformOriginValue {
    pub fn resolve(&self, size: f32) -> f32 {
        match self {
            Self::Percent(p) => size * p / 100.0,
            Self::Px(v) => *v,
        }
    }
}

/// The CSS `transform-origin` property. Default is 50% 50% (center).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TransformOrigin {
    pub x: TransformOriginValue,
    pub y: TransformOriginValue,
}

impl Default for TransformOrigin {
    fn default() -> Self {
        Self {
            x: TransformOriginValue::Percent(50.0),
            y: TransformOriginValue::Percent(50.0),
        }
    }
}

/// Independently authored CSS transition longhand lists.
///
/// The resolved [`ComputedStyle::transitions`] vector is the runtime form.
/// These source lists are retained separately so each longhand can be applied
/// in cascade order without losing values that were temporarily truncated by a
/// different `transition-property` list.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TransitionLonghands {
    pub properties: Option<Vec<TransitionProperties>>,
    pub durations_ms: Option<Vec<u32>>,
    pub delays_ms: Option<Vec<u32>>,
    pub easings: Option<Vec<TransitionEasing>>,
}

impl TransitionLonghands {
    pub fn from_shorthand(entries: &[TransitionStyle]) -> Self {
        Self {
            properties: Some(entries.iter().map(|entry| entry.properties).collect()),
            durations_ms: Some(entries.iter().map(|entry| entry.duration_ms).collect()),
            delays_ms: Some(entries.iter().map(|entry| entry.delay_ms).collect()),
            easings: Some(entries.iter().map(|entry| entry.easing).collect()),
        }
    }

    /// Resolve the independent lists using CSS's repeat-shorter/truncate-
    /// longer list matching rule. The property list determines the number of
    /// transition entries whenever it is present; otherwise the longest
    /// authored longhand list does.
    pub fn resolve(&self) -> Vec<TransitionStyle> {
        let list_len = |len: usize| len.max(1);
        let property_len = self.properties.as_ref().map_or(0, |values| values.len());
        let duration_len = self.durations_ms.as_ref().map_or(0, |values| values.len());
        let delay_len = self.delays_ms.as_ref().map_or(0, |values| values.len());
        let easing_len = self.easings.as_ref().map_or(0, |values| values.len());
        let entry_count = if self.properties.is_some() {
            list_len(property_len)
        } else {
            list_len(
                property_len
                    .max(duration_len)
                    .max(delay_len)
                    .max(easing_len),
            )
        };

        (0..entry_count)
            .map(|index| TransitionStyle {
                properties: Self::value_at(
                    self.properties.as_ref(),
                    index,
                    TransitionProperties::none(),
                ),
                duration_ms: Self::value_at(self.durations_ms.as_ref(), index, 0),
                delay_ms: Self::value_at(self.delays_ms.as_ref(), index, 0),
                easing: Self::value_at(self.easings.as_ref(), index, TransitionEasing::EaseOut),
            })
            .collect()
    }

    fn value_at<T: Copy>(values: Option<&Vec<T>>, index: usize, default: T) -> T {
        values
            .filter(|values| !values.is_empty())
            .map(|values| values[index % values.len()])
            .unwrap_or(default)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Hash)]
pub struct TransitionStyle {
    pub duration_ms: u32,
    pub delay_ms: u32,
    pub easing: TransitionEasing,
    pub properties: TransitionProperties,
}

#[derive(Debug, Clone, PartialEq, Hash)]
pub struct AnimationStyle {
    pub name: Option<String>,
    pub duration_ms: u32,
    pub delay_ms: u32,
    pub easing: TransitionEasing,
    pub iteration_count: AnimationIterationCount,
    pub direction: AnimationDirection,
    pub fill_mode: AnimationFillMode,
    pub play_state: AnimationPlayState,
}

impl Default for AnimationStyle {
    fn default() -> Self {
        Self {
            name: None,
            duration_ms: 0,
            delay_ms: 0,
            easing: TransitionEasing::EaseOut,
            iteration_count: AnimationIterationCount::Number(1),
            direction: AnimationDirection::Normal,
            fill_mode: AnimationFillMode::None,
            play_state: AnimationPlayState::Running,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnimationIterationCount {
    Number(u32),
    Infinite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnimationDirection {
    Normal,
    Reverse,
    Alternate,
    AlternateReverse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnimationFillMode {
    None,
    Forwards,
    Backwards,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnimationPlayState {
    Running,
    Paused,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const TRANSPARENT: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };
    pub const WHITE: Self = Self {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };
    pub const BLACK: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    };

    pub fn from_hex(hex: &str) -> Option<Self> {
        let hex = hex.strip_prefix('#')?;
        if !hex.is_ascii() {
            return None;
        }
        match hex.len() {
            3 => {
                let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
                let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
                let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
                Some(Self { r, g, b, a: 255 })
            }
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                Some(Self { r, g, b, a: 255 })
            }
            8 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
                Some(Self { r, g, b, a })
            }
            _ => None,
        }
    }

    /// Parse a context-independent theme CSS color into sRGB channels.
    pub fn from_css(value: &str) -> Option<Self> {
        // Preserve channel rounding for existing authored hex/rgb values.
        Self::from_legacy_css(value).or_else(|| {
            let [r, g, b, a] = mesh_core_theme::css::parse_color_rgba(value)?;
            Some(Self { r, g, b, a })
        })
    }

    fn from_legacy_css(value: &str) -> Option<Self> {
        let value = value.trim();
        if let Some(color) = Self::from_hex(value) {
            return Some(color);
        }
        match value {
            "transparent" => return Some(Self::TRANSPARENT),
            "black" => return Some(Self::BLACK),
            "white" => return Some(Self::WHITE),
            _ => {}
        }

        let (body, has_alpha) = if let Some(body) = value
            .strip_prefix("rgba(")
            .and_then(|value| value.strip_suffix(')'))
        {
            (body, true)
        } else if let Some(body) = value
            .strip_prefix("rgb(")
            .and_then(|value| value.strip_suffix(')'))
        {
            (body, false)
        } else {
            return None;
        };
        let parts = body.split(',').map(str::trim).collect::<Vec<_>>();
        if parts.len() != if has_alpha { 4 } else { 3 } {
            return None;
        }
        let channel = |value: &str| -> Option<u8> {
            if let Some(percent) = value.strip_suffix('%') {
                return Some((percent.parse::<f32>().ok()?.clamp(0.0, 100.0) * 2.55).round() as u8);
            }
            Some(value.parse::<f32>().ok()?.clamp(0.0, 255.0).round() as u8)
        };
        let alpha = if has_alpha {
            let value = parts[3];
            let unit = if let Some(percent) = value.strip_suffix('%') {
                percent.parse::<f32>().ok()? / 100.0
            } else {
                value.parse::<f32>().ok()?
            };
            (unit.clamp(0.0, 1.0) * 255.0).round() as u8
        } else {
            255
        };
        Some(Self {
            r: channel(parts[0])?,
            g: channel(parts[1])?,
            b: channel(parts[2])?,
            a: alpha,
        })
    }

    pub fn from_token(value: &TokenValue) -> Option<Self> {
        match value {
            TokenValue::String(s) => Self::from_css(s),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Display {
    Flex,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Visibility {
    #[default]
    Visible,
    Hidden,
    Collapse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FlexDirection {
    Row,
    Column,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JustifyContent {
    Start,
    End,
    Center,
    SpaceBetween,
    SpaceAround,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AlignItems {
    Start,
    End,
    Center,
    Stretch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FontStyle {
    Normal,
    Italic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum TextDirection {
    #[default]
    Ltr,
    Rtl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TextOverflow {
    #[default]
    Clip,
    Ellipsis,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum WhiteSpace {
    #[default]
    Normal,
    Nowrap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FlexWrap {
    NoWrap,
    Wrap,
    WrapReverse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AlignSelf {
    Auto,
    Start,
    End,
    Center,
    Stretch,
    Baseline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum Position {
    #[default]
    Static,
    Relative,
    Absolute,
    Fixed,
}

/// CSS `mix-blend-mode`: how an element's own painting is composited with the
/// content already behind it. Only the modes the painter backend implements are
/// represented; unknown values resolve to `Normal`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum BlendMode {
    #[default]
    Normal,
    Multiply,
    Screen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AlignContent {
    Start,
    End,
    Center,
    SpaceBetween,
    SpaceAround,
    Stretch,
}
