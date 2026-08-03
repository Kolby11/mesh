use super::*;

pub(super) struct RuntimeTreeHasher(u64);

impl Default for RuntimeTreeHasher {
    fn default() -> Self {
        Self(FNV_OFFSET)
    }
}

impl Hasher for RuntimeTreeHasher {
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

    fn write_u128(&mut self, value: u128) {
        self.write_mix(value as u64);
        self.write_mix((value >> 64) as u64);
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

    fn write_i128(&mut self, value: i128) {
        self.write_u128(value as u128);
    }

    fn write_isize(&mut self, value: isize) {
        self.write_mix(value as usize as u64);
    }
}

impl RuntimeTreeHasher {
    #[inline]
    pub(super) fn write_mix(&mut self, value: u64) {
        self.0 ^= value;
        self.0 = self.0.wrapping_mul(FNV_PRIME);
        self.0 ^= self.0 >> 32;
    }
}

pub(super) fn retained_snapshot(node: &WidgetNode) -> RetainedNodeSnapshot {
    retained_snapshot_with_render(node, RenderObjectFingerprint::for_node(node, None))
}

pub(super) fn retained_snapshot_with_render(
    node: &WidgetNode,
    render: RenderObjectFingerprint,
) -> RetainedNodeSnapshot {
    RetainedNodeSnapshot {
        layout: layout_fingerprint(node),
        style_hash: style_fingerprint(&node.computed_style),
        attributes_hash: attributes_fingerprint(node),
        child_ids: node.children.iter().map(|child| child.id).collect(),
        state: node.state,
        render,
        last_seen_epoch: 0,
    }
}

pub(super) fn layout_fingerprint(node: &WidgetNode) -> LayoutFingerprint {
    let layout = node.layout;
    let scroll = node.resolved_scroll_metrics();
    (
        layout.x.to_bits(),
        layout.y.to_bits(),
        layout.width.to_bits(),
        layout.height.to_bits(),
        scroll.x.to_bits(),
        scroll.y.to_bits(),
        scroll.max_x.to_bits(),
        scroll.max_y.to_bits(),
        scroll.content_width.to_bits(),
        scroll.content_height.to_bits(),
    )
}

pub(super) fn style_fingerprint(style: &ComputedStyle) -> u64 {
    let mut hasher = RuntimeTreeHasher::default();
    hash_style_fields(style, &mut hasher);
    hasher.finish()
}

pub(super) fn hash_style_fields(style: &ComputedStyle, hasher: &mut impl Hasher) {
    hash_dimension(style.width, hasher);
    hash_dimension(style.height, hasher);
    hash_dimension(style.min_width, hasher);
    hash_dimension(style.max_width, hasher);
    hash_dimension(style.min_height, hasher);
    hash_dimension(style.max_height, hasher);
    hash_edges(style.padding, hasher);
    hash_edges(style.margin, hasher);
    hash_edges(style.border_width, hasher);
    hash_color(style.background_color, hasher);
    match &style.background_paint {
        BackgroundPaint::None => 0_u8.hash(hasher),
        BackgroundPaint::Image(source) => {
            1_u8.hash(hasher);
            source.path.hash(hasher);
        }
        BackgroundPaint::LinearGradient(gradient) => {
            2_u8.hash(hasher);
            hash_color(gradient.from, hasher);
            hash_color(gradient.to, hasher);
        }
    }
    hash_color(style.border_color, hasher);
    hash_corners(style.border_radius, hasher);
    style.opacity.to_bits().hash(hasher);
    hash_transform(style.transform, hasher);
    style.transitions.hash(hasher);
    style.animations.hash(hasher);
    style.overflow_x.hash(hasher);
    style.overflow_y.hash(hasher);
    style.font_family.hash(hasher);
    style.font_size.to_bits().hash(hasher);
    style.font_weight.hash(hasher);
    hash_color(style.color, hasher);
    style.text_align.hash(hasher);
    style.line_height.to_bits().hash(hasher);
    style.font_style.hash(hasher);
    style.letter_spacing.to_bits().hash(hasher);
    style.text_overflow.hash(hasher);
    style.text_direction.hash(hasher);
    style.display.hash(hasher);
    style.direction.hash(hasher);
    style.justify_content.hash(hasher);
    style.align_items.hash(hasher);
    style.align_content.hash(hasher);
    style.gap.to_bits().hash(hasher);
    style.flex_grow.to_bits().hash(hasher);
    style.flex_shrink.to_bits().hash(hasher);
    hash_dimension(style.flex_basis, hasher);
    style.flex_wrap.hash(hasher);
    style.align_self.hash(hasher);
    style.position.hash(hasher);
    style.mix_blend_mode.hash(hasher);
    style.z_index.hash(hasher);
    style.box_shadow.offset_x.to_bits().hash(hasher);
    style.box_shadow.offset_y.to_bits().hash(hasher);
    style.box_shadow.blur_radius.to_bits().hash(hasher);
    style.box_shadow.spread_radius.to_bits().hash(hasher);
    hash_color(style.box_shadow.color, hasher);
    style.box_shadow.inset.hash(hasher);
    style.filter.blur_radius.to_bits().hash(hasher);
    style.backdrop_filter.blur_radius.to_bits().hash(hasher);
    hash_option_f32(style.inset_top, hasher);
    hash_option_f32(style.inset_right, hasher);
    hash_option_f32(style.inset_bottom, hasher);
    hash_option_f32(style.inset_left, hasher);
    hash_option_f32(style.icon_fill, hasher);
    hash_option_f32(style.icon_weight, hasher);
    hash_option_f32(style.icon_grade, hasher);
    hash_option_f32(style.icon_optical_size, hasher);
}

pub(super) fn attributes_fingerprint(node: &WidgetNode) -> u64 {
    let mut hasher = RuntimeTreeHasher::default();
    node.tag.hash(&mut hasher);
    node.module_id().hash(&mut hasher);
    for (key, value) in &node.attributes {
        if is_typed_runtime_annotation_attribute(key) {
            continue;
        }
        if key == "content" && !node.children.is_empty() {
            continue;
        }
        key.hash(&mut hasher);
        value.hash(&mut hasher);
    }
    for (event, handler) in &node.event_handlers {
        event.hash(&mut hasher);
        handler.hash(&mut hasher);
    }
    for (event, call) in &node.event_handler_calls {
        event.hash(&mut hasher);
        call.handler.hash(&mut hasher);
        for arg in &call.args {
            hash_json_value(arg, &mut hasher);
        }
    }
    hash_accessibility_role(&node.accessibility.role, &mut hasher);
    node.accessibility.label.hash(&mut hasher);
    node.accessibility.focusable.hash(&mut hasher);
    node.accessibility.focused.hash(&mut hasher);
    hasher.finish()
}

pub(super) fn hash_accessibility_role(role: &AccessibilityRole, hasher: &mut impl Hasher) {
    match role {
        AccessibilityRole::Button => 0_u8.hash(hasher),
        AccessibilityRole::Slider => 1_u8.hash(hasher),
        AccessibilityRole::Label => 2_u8.hash(hasher),
        AccessibilityRole::TextInput => 3_u8.hash(hasher),
        AccessibilityRole::Checkbox => 4_u8.hash(hasher),
        AccessibilityRole::Switch => 5_u8.hash(hasher),
        AccessibilityRole::Region => 6_u8.hash(hasher),
        AccessibilityRole::List => 7_u8.hash(hasher),
        AccessibilityRole::ListItem => 8_u8.hash(hasher),
        AccessibilityRole::Image => 9_u8.hash(hasher),
        AccessibilityRole::Toolbar => 10_u8.hash(hasher),
        AccessibilityRole::Menu => 11_u8.hash(hasher),
        AccessibilityRole::MenuItem => 12_u8.hash(hasher),
        AccessibilityRole::Dialog => 13_u8.hash(hasher),
        AccessibilityRole::Alert => 14_u8.hash(hasher),
        AccessibilityRole::Status => 15_u8.hash(hasher),
        AccessibilityRole::ProgressBar => 16_u8.hash(hasher),
        AccessibilityRole::Tab => 17_u8.hash(hasher),
        AccessibilityRole::TabPanel => 18_u8.hash(hasher),
        AccessibilityRole::Separator => 19_u8.hash(hasher),
        AccessibilityRole::Custom(value) => {
            20_u8.hash(hasher);
            value.hash(hasher);
        }
    }
}

pub(super) fn is_typed_runtime_annotation_attribute(key: &str) -> bool {
    matches!(
        key,
        "_mesh_key"
            | "_mesh_focused"
            | "_mesh_scroll_x"
            | "_mesh_scroll_y"
            | "_mesh_scroll_max_x"
            | "_mesh_scroll_max_y"
            | "_mesh_content_width"
            | "_mesh_content_height"
    )
}

pub(super) fn hash_json_value(value: &serde_json::Value, hasher: &mut impl Hasher) {
    match value {
        serde_json::Value::Null => 0u8.hash(hasher),
        serde_json::Value::Bool(value) => {
            1u8.hash(hasher);
            value.hash(hasher);
        }
        serde_json::Value::Number(value) => {
            2u8.hash(hasher);
            if let Some(value) = value.as_i64() {
                0u8.hash(hasher);
                value.hash(hasher);
            } else if let Some(value) = value.as_u64() {
                1u8.hash(hasher);
                value.hash(hasher);
            } else if let Some(value) = value.as_f64() {
                2u8.hash(hasher);
                value.to_bits().hash(hasher);
            } else {
                3u8.hash(hasher);
                value.to_string().hash(hasher);
            }
        }
        serde_json::Value::String(value) => {
            3u8.hash(hasher);
            value.hash(hasher);
        }
        serde_json::Value::Array(values) => {
            4u8.hash(hasher);
            values.len().hash(hasher);
            for value in values {
                hash_json_value(value, hasher);
            }
        }
        serde_json::Value::Object(values) => {
            5u8.hash(hasher);
            values.len().hash(hasher);
            for (key, value) in values {
                key.hash(hasher);
                hash_json_value(value, hasher);
            }
        }
    }
}

/// Converts ElementState to a u32 bitmask using stable bit positions.
/// Bit positions mirror the style resolver's STATE_HOVERED, STATE_FOCUSED, etc. constants
/// and are kept self-contained here to avoid a cross-crate dependency on private constants.
pub(super) fn state_bitmask(state: ElementState) -> u32 {
    let mut mask = 0u32;
    if state.hovered {
        mask |= 1 << 0;
    }
    if state.focused {
        mask |= 1 << 1;
    }
    if state.active {
        mask |= 1 << 2;
    }
    if state.disabled {
        mask |= 1 << 3;
    }
    if state.read_only {
        mask |= 1 << 4;
    }
    if state.required {
        mask |= 1 << 5;
    }
    if state.selected {
        mask |= 1 << 6;
    }
    if state.checked {
        mask |= 1 << 7;
    }
    if state.expanded {
        mask |= 1 << 8;
    }
    if state.pressed {
        mask |= 1 << 9;
    }
    if state.invalid {
        mask |= 1 << 10;
    }
    if state.value {
        mask |= 1 << 11;
    }
    if state.focus_visible {
        mask |= 1 << 12;
    }
    mask
}

pub(super) fn hash_dimension(value: Dimension, hasher: &mut impl Hasher) {
    match value {
        Dimension::Auto => 0u8.hash(hasher),
        Dimension::Px(px) => {
            1u8.hash(hasher);
            px.to_bits().hash(hasher);
        }
        Dimension::Percent(percent) => {
            2u8.hash(hasher);
            percent.to_bits().hash(hasher);
        }
        Dimension::Content => 3u8.hash(hasher),
        Dimension::Fit => 4u8.hash(hasher),
    }
}

pub(super) fn hash_edges(value: Edges, hasher: &mut impl Hasher) {
    value.top.to_bits().hash(hasher);
    value.right.to_bits().hash(hasher);
    value.bottom.to_bits().hash(hasher);
    value.left.to_bits().hash(hasher);
}

pub(super) fn hash_corners(value: Corners, hasher: &mut impl Hasher) {
    value.top_left.to_bits().hash(hasher);
    value.top_right.to_bits().hash(hasher);
    value.bottom_right.to_bits().hash(hasher);
    value.bottom_left.to_bits().hash(hasher);
}

pub(super) fn hash_color(value: Color, hasher: &mut impl Hasher) {
    value.r.hash(hasher);
    value.g.hash(hasher);
    value.b.hash(hasher);
    value.a.hash(hasher);
}

pub(super) fn hash_transform(value: Transform2D, hasher: &mut impl Hasher) {
    value.translate_x.to_bits().hash(hasher);
    value.translate_y.to_bits().hash(hasher);
    value.scale_x.to_bits().hash(hasher);
    value.scale_y.to_bits().hash(hasher);
    value.rotation.to_bits().hash(hasher);
}

pub(super) fn hash_option_f32(value: Option<f32>, hasher: &mut impl Hasher) {
    match value {
        Some(value) => {
            true.hash(hasher);
            value.to_bits().hash(hasher);
        }
        None => false.hash(hasher),
    }
}
