//! Named parts of native controls, styled with `host::part` rules while the
//! host element keeps its behaviour (spec 04 §6.2).
//!
//! The compiler appends a host's part subtree as ordinary child nodes, so
//! layout, paint, transitions and backdrop filters apply to parts unchanged.
//! Parts are styled while their host is resolved and are never matched by
//! ordinary rules, hit tested or exposed to accessibility.

use crate::Prop;
use crate::style::{
    AlignItems, AlignSelf, Color, ComputedStyle, Corners, Dimension, Edges, FlexDirection,
    JustifyContent, Position,
};
use crate::tree::WidgetNode;

/// One node of a control's part subtree. Named parts are the public styling
/// API; the unnamed ones are structure that places them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ControlPart {
    /// `slider::track`: the bar the value runs along.
    SliderTrack,
    /// `slider::fill`: the part of the track up to the value.
    SliderFill,
    /// `slider::thumb`: the handle centred on the value.
    SliderThumb,
    /// Covers the track and carries the thumb independently of the fill.
    SliderRail,
    /// Spans the rail from its start to the value.
    SliderLead,
    /// Two-pixel slot centred on the value; centres the thumb on it at any
    /// size. It keeps an area because boxes without one are not painted, and
    /// whole pixels because layout rounds to them.
    SliderAnchor,
}

impl ControlPart {
    /// The name `host::name` selectors use, or `None` for structure.
    pub fn name(self) -> Option<&'static str> {
        match self {
            Self::SliderTrack => Some("track"),
            Self::SliderFill => Some("fill"),
            Self::SliderThumb => Some("thumb"),
            Self::SliderRail | Self::SliderLead | Self::SliderAnchor => None,
        }
    }
}

/// Part names `tag` exposes to `tag::name` selectors.
pub fn control_part_names(tag: &str) -> &'static [&'static str] {
    match crate::element_runtime_tag_for_tag(tag) {
        Some("slider") => &["track", "fill", "thumb"],
        _ => &[],
    }
}

/// The part subtree a `tag` element carries, in paint order.
pub fn control_part_nodes(tag: &str) -> Vec<WidgetNode> {
    match crate::element_runtime_tag_for_tag(tag) {
        Some("slider") => {
            let anchor = part_node(ControlPart::SliderAnchor, vec![part_node(
                ControlPart::SliderThumb,
                Vec::new(),
            )]);
            let rail = part_node(ControlPart::SliderRail, vec![
                part_node(ControlPart::SliderLead, Vec::new()),
                anchor,
            ]);
            vec![part_node(ControlPart::SliderTrack, vec![
                part_node(ControlPart::SliderFill, Vec::new()),
                rail,
            ])]
        }
        _ => Vec::new(),
    }
}

fn part_node(part: ControlPart, children: Vec<WidgetNode>) -> WidgetNode {
    let mut node = WidgetNode::new("box");
    node.set_control_part(part);
    node.children = children.into();
    node
}

/// Value-derived geometry shared by every part of one slider.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SliderGeometry {
    /// Position of the value between `min` and `max`, in `0..=1`.
    pub fraction: f32,
    pub vertical: bool,
}

impl SliderGeometry {
    pub(crate) fn of(host: &WidgetNode) -> Self {
        let number = |prop: Prop, default: f32| {
            host.attributes
                .get(prop)
                .and_then(|value| value.trim().parse::<f32>().ok())
                .filter(|value| value.is_finite())
                .unwrap_or(default)
        };
        let (min, max) = (number(Prop::Min, 0.0), number(Prop::Max, 100.0));
        let value = number(Prop::Value, 50.0);
        let fraction = if max > min {
            ((value - min) / (max - min)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        Self {
            fraction,
            vertical: host
                .attributes
                .get(Prop::Orient)
                .is_some_and(|orient| orient == "vertical"),
        }
    }
}

/// Lay the host out around its parts: a slider's only children are its
/// parts, so its main axis follows the orientation.
pub(crate) fn apply_host_layout(host: &mut ComputedStyle, geometry: SliderGeometry) {
    host.direction = if geometry.vertical {
        FlexDirection::Column
    } else {
        FlexDirection::Row
    };
    host.align_items = AlignItems::Center;
}

/// The style a part starts from before `host::part` rules apply. Together
/// these reproduce the platform's unstyled slider, tinted with the host's
/// `color`. Values are written for a horizontal slider.
pub(crate) fn initial_part_style(part: ControlPart, host: &ComputedStyle) -> ComputedStyle {
    let mut style = ComputedStyle::default();
    match part {
        ControlPart::SliderTrack => {
            style.flex_grow = 1.0;
            style.height = Dimension::Px(4.0);
            style.margin = Edges {
                left: 8.0,
                right: 8.0,
                ..Edges::all(0.0)
            };
            style.border_radius = Corners::all(2.0);
            style.background_color = dim(host.color, 0.35);
        }
        ControlPart::SliderFill => {
            style.border_radius = Corners::all(2.0);
            style.background_color = host.color;
        }
        ControlPart::SliderThumb => {
            style.width = Dimension::Px(16.0);
            style.height = Dimension::Px(16.0);
            style.border_radius = Corners::all(8.0);
            style.background_color = host.color;
        }
        ControlPart::SliderRail | ControlPart::SliderLead | ControlPart::SliderAnchor => {}
    }
    style
}

/// Swap the axes of a style authored for a horizontal slider. The start of a
/// horizontal track (left) is the bottom of a vertical one.
pub(crate) fn transpose_for_vertical(style: &mut ComputedStyle) {
    std::mem::swap(&mut style.width, &mut style.height);
    std::mem::swap(&mut style.min_width, &mut style.min_height);
    std::mem::swap(&mut style.max_width, &mut style.max_height);
    let rotate = |edges: Edges| Edges {
        bottom: edges.left,
        top: edges.right,
        left: edges.top,
        right: edges.bottom,
    };
    style.margin = rotate(style.margin);
    style.padding = rotate(style.padding);
}

/// The geometry core owns: where the fill ends and where the thumb sits.
/// Applied after the cascade, so author rules cannot move the value.
pub(crate) fn apply_part_geometry(
    part: ControlPart,
    style: &mut ComputedStyle,
    geometry: SliderGeometry,
) {
    let percent = geometry.fraction * 100.0;
    let main = if geometry.vertical {
        FlexDirection::Column
    } else {
        FlexDirection::Row
    };
    let set_main = |style: &mut ComputedStyle, value: Dimension| {
        if geometry.vertical {
            style.height = value;
        } else {
            style.width = value;
        }
    };
    match part {
        ControlPart::SliderTrack => {
            style.position = Position::Relative;
            style.direction = main;
            // A vertical value grows from the bottom.
            style.justify_content = if geometry.vertical {
                JustifyContent::End
            } else {
                JustifyContent::Start
            };
            style.align_items = AlignItems::Stretch;
        }
        ControlPart::SliderFill => {
            set_main(style, Dimension::Percent(percent));
            style.flex_shrink = 0.0;
            style.align_self = AlignSelf::Stretch;
        }
        ControlPart::SliderRail => {
            style.position = Position::Absolute;
            style.inset_top = Some(0.0);
            style.inset_right = Some(0.0);
            style.inset_bottom = Some(0.0);
            style.inset_left = Some(0.0);
            style.direction = main;
            style.justify_content = JustifyContent::Start;
            style.align_items = AlignItems::Stretch;
        }
        ControlPart::SliderLead => {
            // The rail runs top-down, so a vertical lead spans what is left
            // above the value.
            let lead = if geometry.vertical {
                100.0 - percent
            } else {
                percent
            };
            set_main(style, Dimension::Percent(lead));
            style.flex_shrink = 0.0;
        }
        ControlPart::SliderAnchor => {
            set_main(style, Dimension::Px(2.0));
            if geometry.vertical {
                style.margin.top = -1.0;
            } else {
                style.margin.left = -1.0;
            }
            style.flex_shrink = 0.0;
            style.align_self = AlignSelf::Stretch;
            style.direction = main;
            style.justify_content = JustifyContent::Center;
            style.align_items = AlignItems::Center;
        }
        ControlPart::SliderThumb => {
            style.flex_shrink = 0.0;
        }
    }
}

fn dim(color: Color, factor: f32) -> Color {
    let scale = |channel: u8| ((channel as f32) * factor).round().clamp(0.0, 255.0) as u8;
    Color {
        r: scale(color.r),
        g: scale(color.g),
        b: scale(color.b),
        a: color.a,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::LayoutEngine;
    use crate::style::{StyleContext, StyleResolver};
    use crate::tree::NodeState;
    use mesh_core_component::style::{Declaration, StyleRule, StyleValue};

    fn rule(selector: &str, declarations: &[(&str, &str)]) -> StyleRule {
        StyleRule {
            selector: mesh_core_theme::css::parse_selector(selector).unwrap(),
            declarations: declarations
                .iter()
                .map(|(property, value)| Declaration {
                    property: (*property).into(),
                    value: StyleValue::Literal((*value).into()),
                })
                .collect(),
            container_query: None,
        }
    }

    fn slider(value: &str) -> WidgetNode {
        let mut slider = WidgetNode::new("slider");
        slider.attributes.insert(Prop::Class, "glassy".into());
        slider.attributes.insert(Prop::Value, value.into());
        slider.children = control_part_nodes("slider").into();
        slider
    }

    fn part_or_none(node: &WidgetNode, wanted: ControlPart) -> Option<&WidgetNode> {
        if node.control_part() == Some(wanted) {
            return Some(node);
        }
        node.children
            .iter()
            .find_map(|child| part_or_none(child, wanted))
    }

    fn rules() -> Vec<StyleRule> {
        vec![
            rule("slider", &[("width", "200px"), ("height", "40px"), ("padding", "0px")]),
            // An ordinary rule never reaches a part.
            rule("box", &[("background-color", "#ff0000")]),
            rule("slider.glassy::track", &[("height", "6px"), ("margin-left", "0px"), ("margin-right", "0px")]),
            rule("slider::thumb", &[("width", "36px"), ("height", "22px")]),
            rule("slider:active::thumb", &[("width", "48px")]),
        ]
    }

    fn styled(mut node: WidgetNode) -> WidgetNode {
        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);
        resolver.restyle_subtree_cached(&mut node, &rules(), StyleContext::default(), &mut None);
        LayoutEngine::compute(&mut node, 400.0, 100.0);
        node
    }

    #[test]
    fn slider_parts_take_part_rules_and_core_geometry() {
        let node = styled(slider("25"));
        let track = part_or_none(&node, ControlPart::SliderTrack).unwrap();
        let fill = part_or_none(&node, ControlPart::SliderFill).unwrap();
        let thumb = part_or_none(&node, ControlPart::SliderThumb).unwrap();
        assert_ne!(
            track.computed_style.background_color,
            Color::from_hex("#ff0000").unwrap(),
            "ordinary rules do not match parts"
        );
        assert_eq!(track.layout.height, 6.0);
        assert_eq!(track.layout.width, 200.0);
        assert_eq!(fill.computed_style.width, Dimension::Percent(25.0));
        assert_eq!(fill.layout.width, 50.0);
        assert_eq!((thumb.layout.width, thumb.layout.height), (36.0, 22.0));
        // The anchor sits at the value and the thumb is centred on it.
        let anchor = part_or_none(&node, ControlPart::SliderAnchor).unwrap();
        assert_eq!(anchor.layout.x + anchor.layout.width / 2.0, 50.0);
        assert_eq!(thumb.layout.x, 50.0 - 18.0);
    }

    #[test]
    fn host_state_selects_part_rules_and_the_thumb_stays_centred() {
        let mut node = slider("50");
        node.state.set(NodeState::ACTIVE, true);
        let node = styled(node);
        let anchor = part_or_none(&node, ControlPart::SliderAnchor).unwrap();
        let thumb = part_or_none(&node, ControlPart::SliderThumb).unwrap();
        assert_eq!(thumb.layout.width, 48.0);
        assert_eq!(anchor.layout.x + anchor.layout.width / 2.0, 100.0);
        assert_eq!(thumb.layout.x, 100.0 - 24.0);
    }

    #[test]
    fn vertical_sliders_transpose_part_styles_and_grow_from_the_bottom() {
        let mut node = slider("25");
        node.attributes.insert(Prop::Orient, "vertical".into());
        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);
        let mut rules = rules();
        rules[0] = rule("slider", &[("width", "40px"), ("height", "200px"), ("padding", "0px")]);
        resolver.restyle_subtree_cached(&mut node, &rules, StyleContext::default(), &mut None);
        LayoutEngine::compute(&mut node, 100.0, 400.0);
        let track = part_or_none(&node, ControlPart::SliderTrack).unwrap();
        let fill = part_or_none(&node, ControlPart::SliderFill).unwrap();
        let thumb = part_or_none(&node, ControlPart::SliderThumb).unwrap();
        assert_eq!((track.layout.width, track.layout.height), (6.0, 200.0));
        assert_eq!((thumb.layout.width, thumb.layout.height), (22.0, 36.0));
        assert_eq!(fill.layout.height, 50.0);
        // The fill grows from the bottom of the 200px track.
        assert!((fill.layout.y - 150.0).abs() < 0.5, "fill y {}", fill.layout.y);
        let anchor = part_or_none(&node, ControlPart::SliderAnchor).unwrap();
        let anchor_centre = anchor.layout.y + anchor.layout.height / 2.0;
        assert!((anchor_centre - 150.0).abs() < 0.01, "anchor centre {anchor_centre}");
        assert_eq!(thumb.layout.y + thumb.layout.height / 2.0, 150.0);
    }
}
