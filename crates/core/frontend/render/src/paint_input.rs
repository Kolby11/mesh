use mesh_core_elements::Prop;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use mesh_core_elements::WidgetNode;
use mesh_core_elements::style::{BackgroundPaint, ComputedStyle, Visibility};
use mesh_core_resources::resource_revision;

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// The paint inputs shared by retained-object invalidation and display-list
/// entry signatures. Keeping the categories here makes every paint-affecting
/// value visible to both contracts instead of maintaining two field lists.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct PaintInput {
    pub(crate) resource_revision: u64,
    pub(crate) material: u64,
    pub(crate) primitive: u64,
    pub(crate) text: TextPaintInput,
    pub(crate) icon: u64,
    pub(crate) variables: u64,
    pub(crate) opacity: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct TextPaintInput {
    pub(crate) content: Option<Arc<str>>,
    pub(crate) signature: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum PaintPrimitiveSlot {
    Background,
    Border,
    Text,
    Icon,
    Generic,
}

impl PaintInput {
    pub(crate) fn for_node(node: &WidgetNode, previous: Option<&Self>) -> Self {
        let previous_text = previous.map(|input| &input.text);
        Self {
            resource_revision: resource_revision(),
            material: hash_material(&node.computed_style),
            primitive: hash_primitive(node),
            text: text_input(node, previous_text),
            icon: hash_icon(node),
            variables: paint_variables_fingerprint(&node.computed_style),
            opacity: node.computed_style.opacity.to_bits(),
        }
    }

    pub(crate) fn signature_for_slot(&self, slot: PaintPrimitiveSlot) -> u64 {
        let mut hasher = PaintInputHasher::default();
        slot.hash(&mut hasher);
        self.resource_revision.hash(&mut hasher);
        self.material.hash(&mut hasher);
        self.primitive.hash(&mut hasher);
        self.text.signature.hash(&mut hasher);
        self.icon.hash(&mut hasher);
        self.variables.hash(&mut hasher);
        self.opacity.hash(&mut hasher);
        hasher.finish()
    }
}

/// The custom-property map's fingerprint, computed once when the map was
/// built and shared by every node that inherits it unchanged.
pub fn paint_variables_fingerprint(style: &ComputedStyle) -> u64 {
    style.custom_properties.fingerprint()
}

fn hash_material(style: &ComputedStyle) -> u64 {
    let mut hasher = PaintInputHasher::default();
    hash_color(style.background_color, &mut hasher);
    hash_background_paint(&style.background_paint, &mut hasher);
    hash_color(style.border_color, &mut hasher);
    hash_edges(style.border_width, &mut hasher);
    hash_edges(style.padding, &mut hasher);
    hash_corners(style.border_radius, &mut hasher);
    style.corner_shape.hash(&mut hasher);
    style.display.hash(&mut hasher);
    hash_visibility(style.visibility, &mut hasher);
    style.overflow_x.hash(&mut hasher);
    style.overflow_y.hash(&mut hasher);
    style.mix_blend_mode.hash(&mut hasher);
    style.z_index.hash(&mut hasher);
    hash_box_shadow(style.box_shadow, &mut hasher);
    if !style.additional_box_shadows.is_empty() {
        style.additional_box_shadows.len().hash(&mut hasher);
        for shadow in &style.additional_box_shadows {
            hash_box_shadow(*shadow, &mut hasher);
        }
    }
    style.filter.blur_radius.to_bits().hash(&mut hasher);
    let backdrop = style.backdrop_filter;
    backdrop.blur_radius.to_bits().hash(&mut hasher);
    backdrop.refract.hash(&mut hasher);
    backdrop.refract_depth.to_bits().hash(&mut hasher);
    backdrop.refract_edge.to_bits().hash(&mut hasher);
    backdrop.dispersion.to_bits().hash(&mut hasher);
    hasher.finish()
}

fn hash_primitive(node: &WidgetNode) -> u64 {
    let mut hasher = PaintInputHasher::default();
    node.tag.hash(&mut hasher);
    match node.tag.as_str() {
        "input" => {
            hash_attributes(
                node,
                &[Prop::Value, Prop::Placeholder, Prop::Type],
                &mut hasher,
            );
            node.state
                .contains(mesh_core_elements::NodeState::FOCUSED)
                .hash(&mut hasher);
            node.input_preedit.hash(&mut hasher);
            hash_color(node.computed_style.color, &mut hasher);
        }
        "slider" => {
            hash_attributes(
                node,
                &[Prop::Min, Prop::Max, Prop::Value, Prop::Orient],
                &mut hasher,
            );
            hash_color(node.computed_style.color, &mut hasher);
        }
        "checkbox" | "radio" => {
            hash_attributes(node, &[Prop::Checked], &mut hasher);
            hash_color(node.computed_style.color, &mut hasher);
        }
        _ => {}
    }
    hasher.finish()
}

fn text_input(node: &WidgetNode, previous: Option<&TextPaintInput>) -> TextPaintInput {
    let content = if node.tag == "text" {
        retained_arc_str(
            node.attributes
                .get(Prop::Text)
                .or_else(|| node.attributes.get(Prop::Content))
                .map(String::as_str),
            previous.and_then(|input| input.content.as_ref()),
        )
    } else {
        None
    };
    let mut hasher = PaintInputHasher::default();
    if node.tag == "text" {
        hash_attributes(node, &[Prop::Content, Prop::Text], &mut hasher);
        node.text_selection.hash(&mut hasher);
    }
    hash_attributes(node, &[Prop::Lang, Prop::FontFeatures], &mut hasher);
    hash_color(node.computed_style.color, &mut hasher);
    node.computed_style.font_family.hash(&mut hasher);
    node.computed_style.font_size.to_bits().hash(&mut hasher);
    node.computed_style.font_weight.hash(&mut hasher);
    node.computed_style.line_height.to_bits().hash(&mut hasher);
    node.computed_style.font_style.hash(&mut hasher);
    node.computed_style.text_align.hash(&mut hasher);
    node.computed_style.text_overflow.hash(&mut hasher);
    node.computed_style.white_space.hash(&mut hasher);
    node.computed_style.text_direction.hash(&mut hasher);
    node.computed_style
        .letter_spacing
        .to_bits()
        .hash(&mut hasher);
    TextPaintInput {
        content,
        signature: hasher.finish(),
    }
}

fn hash_icon(node: &WidgetNode) -> u64 {
    let mut hasher = PaintInputHasher::default();
    if matches!(node.tag.as_str(), "icon" | "image") {
        hash_attributes(node, &[Prop::Src, Prop::Name, Prop::Size], &mut hasher);
    }
    hash_color(node.computed_style.color, &mut hasher);
    node.computed_style.font_family.hash(&mut hasher);
    node.computed_style.font_size.to_bits().hash(&mut hasher);
    node.computed_style.font_weight.hash(&mut hasher);
    node.computed_style.font_style.hash(&mut hasher);
    node.computed_style
        .icon_fill
        .map(f32::to_bits)
        .hash(&mut hasher);
    node.computed_style
        .icon_weight
        .map(f32::to_bits)
        .hash(&mut hasher);
    node.computed_style
        .icon_grade
        .map(f32::to_bits)
        .hash(&mut hasher);
    node.computed_style
        .icon_optical_size
        .map(f32::to_bits)
        .hash(&mut hasher);
    hasher.finish()
}

fn hash_attributes(node: &WidgetNode, keys: &[Prop], hasher: &mut impl Hasher) {
    for key in keys {
        key.hash(hasher);
        node.attributes.get(*key).hash(hasher);
    }
}

fn hash_color(color: mesh_core_elements::style::Color, hasher: &mut impl Hasher) {
    color.r.hash(hasher);
    color.g.hash(hasher);
    color.b.hash(hasher);
    color.a.hash(hasher);
}

fn hash_edges(edges: mesh_core_elements::style::Edges, hasher: &mut impl Hasher) {
    edges.top.to_bits().hash(hasher);
    edges.right.to_bits().hash(hasher);
    edges.bottom.to_bits().hash(hasher);
    edges.left.to_bits().hash(hasher);
}

fn hash_corners(corners: mesh_core_elements::style::Corners, hasher: &mut impl Hasher) {
    corners.top_left.to_bits().hash(hasher);
    corners.top_right.to_bits().hash(hasher);
    corners.bottom_right.to_bits().hash(hasher);
    corners.bottom_left.to_bits().hash(hasher);
}

fn hash_box_shadow(shadow: mesh_core_elements::BoxShadow, hasher: &mut impl Hasher) {
    shadow.offset_x.to_bits().hash(hasher);
    shadow.offset_y.to_bits().hash(hasher);
    shadow.blur_radius.to_bits().hash(hasher);
    shadow.spread_radius.to_bits().hash(hasher);
    hash_color(shadow.color, hasher);
    shadow.inset.hash(hasher);
}

fn hash_background_paint(paint: &BackgroundPaint, hasher: &mut impl Hasher) {
    match paint {
        BackgroundPaint::None => 0_u8.hash(hasher),
        BackgroundPaint::Image(source) => {
            1_u8.hash(hasher);
            source.path.hash(hasher);
        }
        BackgroundPaint::MultiStopLinearGradient(colors) => {
            3_u8.hash(hasher);
            colors.len().hash(hasher);
            for color in colors {
                color.r.hash(hasher);
                color.g.hash(hasher);
                color.b.hash(hasher);
                color.a.hash(hasher);
            }
        }
        BackgroundPaint::LinearGradient(gradient) => {
            2_u8.hash(hasher);
            hash_color(gradient.from, hasher);
            hash_color(gradient.to, hasher);
        }
    }
}

fn hash_visibility(value: Visibility, hasher: &mut impl Hasher) {
    std::mem::discriminant(&value).hash(hasher);
}

pub(crate) fn retained_arc_str(
    value: Option<&str>,
    previous: Option<&Arc<str>>,
) -> Option<Arc<str>> {
    value.map(|value| match previous {
        Some(previous) if previous.as_ref() == value => Arc::clone(previous),
        _ => Arc::from(value),
    })
}

#[derive(Debug, Clone, Copy)]
struct PaintInputHasher(u64);

impl Default for PaintInputHasher {
    fn default() -> Self {
        Self(FNV_OFFSET)
    }
}

impl Hasher for PaintInputHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(FNV_PRIME);
        }
    }

    fn write_u8(&mut self, value: u8) {
        self.write_mix(u64::from(value));
    }

    fn write_u16(&mut self, value: u16) {
        self.write_mix(u64::from(value));
    }

    fn write_u32(&mut self, value: u32) {
        self.write_mix(u64::from(value));
    }

    fn write_u64(&mut self, value: u64) {
        self.write_mix(value);
    }

    fn write_usize(&mut self, value: usize) {
        self.write_mix(value as u64);
    }

    fn write_i8(&mut self, value: i8) {
        self.write_mix(value as u8 as u64);
    }

    fn write_i16(&mut self, value: i16) {
        self.write_mix(value as u16 as u64);
    }

    fn write_i32(&mut self, value: i32) {
        self.write_mix(value as u32 as u64);
    }

    fn write_i64(&mut self, value: i64) {
        self.write_mix(value as u64);
    }
}

impl PaintInputHasher {
    fn write_mix(&mut self, value: u64) {
        self.0 ^= value;
        self.0 = self.0.wrapping_mul(FNV_PRIME);
        self.0 ^= self.0 >> 32;
    }
}

#[test]
fn css_profile_material_hash_includes_later_stops_and_shadows() {
    use mesh_core_elements::{BoxShadow, style::Color};
    let mut style = ComputedStyle::default();
    style.background_paint =
        BackgroundPaint::MultiStopLinearGradient(vec![Color::BLACK, Color::WHITE, Color::BLACK]);
    let before = hash_material(&style);
    if let BackgroundPaint::MultiStopLinearGradient(colors) = &mut style.background_paint {
        colors[1] = Color::TRANSPARENT;
    }
    assert_ne!(before, hash_material(&style));
    style.additional_box_shadows.push(BoxShadow {
        offset_x: 10.0,
        color: Color::BLACK,
        ..BoxShadow::NONE
    });
    let before = hash_material(&style);
    style.additional_box_shadows[0].color = Color::WHITE;
    assert_ne!(before, hash_material(&style));
}
