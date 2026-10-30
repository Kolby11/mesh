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
    JustifyContent, Position, TransitionProperties, TransitionStyle,
};
use crate::tree::{NodeState, WidgetNode};

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
    /// `progress::track` and `meter::track`: the bar the value fills.
    ProgressTrack,
    /// `progress::fill` and `meter::fill`: the track up to the value.
    ProgressFill,
    /// `checkbox::indicator`: the box that shows a check mark when checked.
    CheckboxIndicator,
    /// `radio::indicator`: the ring that shows a dot when checked.
    RadioIndicator,
    /// `switch::track`: the bar the thumb slides along.
    SwitchTrack,
    /// `switch::thumb`: the knob, at the end of the track when checked.
    SwitchThumb,
    /// Spacers before and after the thumb whose shares of the track follow
    /// the checked state; they take the thumb's `margin` transition, so the
    /// thumb glides.
    SwitchLead,
    SwitchTail,
}

impl ControlPart {
    /// The name `host::name` selectors use, or `None` for structure.
    pub fn name(self) -> Option<&'static str> {
        match self {
            Self::SliderTrack | Self::ProgressTrack | Self::SwitchTrack => Some("track"),
            Self::SliderFill | Self::ProgressFill => Some("fill"),
            Self::SliderThumb | Self::SwitchThumb => Some("thumb"),
            Self::CheckboxIndicator | Self::RadioIndicator => Some("indicator"),
            Self::SliderRail
            | Self::SliderLead
            | Self::SliderAnchor
            | Self::SwitchLead
            | Self::SwitchTail => None,
        }
    }
}

/// Part names `tag` exposes to `tag::name` selectors.
pub fn control_part_names(tag: &str) -> &'static [&'static str] {
    match crate::element_runtime_tag_for_tag(tag) {
        Some("slider") => &["track", "fill", "thumb"],
        Some("progress" | "meter") => &["track", "fill"],
        Some("checkbox" | "radio") => &["indicator"],
        Some("switch") => &["track", "thumb"],
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
        Some("progress" | "meter") => vec![part_node(ControlPart::ProgressTrack, vec![
            part_node(ControlPart::ProgressFill, Vec::new()),
        ])],
        Some("checkbox") => vec![part_node(ControlPart::CheckboxIndicator, Vec::new())],
        Some("radio") => vec![part_node(ControlPart::RadioIndicator, Vec::new())],
        Some("switch") => vec![part_node(ControlPart::SwitchTrack, vec![
            part_node(ControlPart::SwitchLead, Vec::new()),
            part_node(ControlPart::SwitchThumb, Vec::new()),
            part_node(ControlPart::SwitchTail, Vec::new()),
        ])],
        _ => Vec::new(),
    }
}

fn part_node(part: ControlPart, children: Vec<WidgetNode>) -> WidgetNode {
    let mut node = WidgetNode::new("box");
    node.set_control_part(part);
    node.children = children.into();
    node
}

/// Share of the track an indeterminate `progress` fills; authors move it
/// with keyframes on `progress::fill`.
const INDETERMINATE_FRACTION: f32 = 0.3;

/// Value-derived geometry shared by every part of one control.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ControlGeometry {
    /// Position of the value between `min` and `max`, in `0..=1`.
    pub fraction: f32,
    pub vertical: bool,
    /// Whether a checkable host is checked.
    pub checked: bool,
}

impl ControlGeometry {
    pub(crate) fn of(host: &WidgetNode) -> Self {
        let number = |prop: Prop, default: f32| {
            host.attributes
                .get(prop)
                .and_then(|value| value.trim().parse::<f32>().ok())
                .filter(|value| value.is_finite())
                .unwrap_or(default)
        };
        let slider = host.tag == "slider";
        let indeterminate = !slider
            && host
                .attributes
                .get(Prop::Indeterminate)
                .is_some_and(|value| matches!(value.as_str(), "" | "true" | "1"));
        let (min, max) = (number(Prop::Min, 0.0), number(Prop::Max, 100.0));
        let value = number(Prop::Value, if slider { 50.0 } else { 0.0 });
        let fraction = if indeterminate {
            INDETERMINATE_FRACTION
        } else if max > min {
            ((value - min) / (max - min)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        Self {
            fraction,
            checked: host.state.contains(NodeState::CHECKED)
                || host
                    .attributes
                    .get(Prop::Checked)
                    .is_some_and(|value| matches!(value.as_str(), "" | "true" | "1" | "checked")),
            vertical: host
                .attributes
                .get(Prop::Orient)
                .is_some_and(|orient| orient == "vertical"),
        }
    }
}

/// Lay the host out around its parts: its main axis follows the
/// orientation.
pub(crate) fn apply_host_layout(host: &mut ComputedStyle, geometry: ControlGeometry) {
    host.direction = if geometry.vertical {
        FlexDirection::Column
    } else {
        FlexDirection::Row
    };
    host.align_items = AlignItems::Center;
}

/// The style a part starts from before `host::part` rules apply. Together
/// these reproduce the platform's unstyled controls, tinted with the host's
/// `color`. Values are written for a horizontal control.
pub(crate) fn initial_part_style(
    part: ControlPart,
    host: &ComputedStyle,
    geometry: ControlGeometry,
) -> ComputedStyle {
    let mut style = ComputedStyle::default();
    let checked = geometry.checked;
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
        ControlPart::ProgressTrack => {
            style.flex_grow = 1.0;
            style.height = Dimension::Px(4.0);
            style.border_radius = Corners::all(2.0);
            style.background_color = dim(host.color, 0.35);
        }
        ControlPart::ProgressFill => {
            style.border_radius = Corners::all(2.0);
            style.background_color = host.color;
        }
        ControlPart::CheckboxIndicator | ControlPart::RadioIndicator => {
            let radius = if part == ControlPart::RadioIndicator { 9.0 } else { 4.0 };
            style.width = Dimension::Px(18.0);
            style.height = Dimension::Px(18.0);
            style.border_radius = Corners::all(radius);
            style.border_width = Edges::all(2.0);
            style.border_color = host.color;
            // The mark is drawn in the indicator's `color`.
            style.color = host.color;
            if checked && part == ControlPart::CheckboxIndicator {
                style.background_color = host.color;
                style.color = contrasting(host.color);
            }
        }
        ControlPart::SwitchTrack => {
            style.width = Dimension::Px(36.0);
            style.height = Dimension::Px(20.0);
            style.padding = Edges::all(2.0);
            style.border_radius = Corners::all(10.0);
            style.background_color = if checked {
                host.color
            } else {
                dim(host.color, 0.35)
            };
        }
        ControlPart::SwitchThumb => {
            style.width = Dimension::Px(16.0);
            style.height = Dimension::Px(16.0);
            style.border_radius = Corners::all(8.0);
            style.background_color = if checked {
                contrasting(host.color)
            } else {
                host.color
            };
        }
        ControlPart::SliderRail
        | ControlPart::SliderLead
        | ControlPart::SliderAnchor
        | ControlPart::SwitchLead
        | ControlPart::SwitchTail => {}
    }
    style
}

/// The glide of a switch's spacers: each of the thumb's transitions that
/// covers `margin` (or `all`) applies to their widths.
pub(crate) fn glide_transitions(thumb: &[TransitionStyle]) -> Vec<TransitionStyle> {
    thumb
        .iter()
        .filter(|transition| transition.properties.all || transition.properties.margin)
        .map(|transition| TransitionStyle {
            properties: TransitionProperties {
                width: true,
                ..TransitionProperties::none()
            },
            ..transition.clone()
        })
        .collect()
}

/// Swap the axes of a style authored for a horizontal control. The start of a
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
    geometry: ControlGeometry,
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
        ControlPart::SliderTrack | ControlPart::ProgressTrack => {
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
        ControlPart::SliderFill | ControlPart::ProgressFill => {
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
        ControlPart::SliderThumb
        | ControlPart::SwitchThumb
        | ControlPart::CheckboxIndicator
        | ControlPart::RadioIndicator => {
            style.flex_shrink = 0.0;
        }
        ControlPart::SwitchTrack => {
            style.flex_shrink = 0.0;
            style.direction = FlexDirection::Row;
            style.justify_content = JustifyContent::Start;
            style.align_items = AlignItems::Center;
        }
        // Both spacers shrink by the thumb's width in proportion to their
        // shares, so the thumb sits at `fraction` of the free track and moves
        // linearly while their widths animate.
        ControlPart::SwitchLead | ControlPart::SwitchTail => {
            let lead = if geometry.checked { 100.0 } else { 0.0 };
            let share = if part == ControlPart::SwitchLead {
                lead
            } else {
                100.0 - lead
            };
            style.width = Dimension::Percent(share);
            style.flex_shrink = 1.0;
            style.min_width = Dimension::Px(0.0);
        }
    }
}

/// Black or white, whichever reads on `color`.
fn contrasting(color: Color) -> Color {
    let luminance =
        0.2126 * f32::from(color.r) + 0.7152 * f32::from(color.g) + 0.0722 * f32::from(color.b);
    if luminance > 140.0 {
        Color {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        }
    } else {
        Color {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
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
    fn authored_children_before_the_parts_keep_part_styling() {
        let mut node = slider("25");
        let mut children = vec![WidgetNode::new("text")];
        children.extend(control_part_nodes("slider"));
        node.children = children.into();
        let node = styled(node);
        let thumb = part_or_none(&node, ControlPart::SliderThumb).unwrap();
        assert_eq!((thumb.layout.width, thumb.layout.height), (36.0, 22.0));
    }

    fn progress(tag: &str, attributes: &[(Prop, &str)]) -> WidgetNode {
        let mut node = WidgetNode::new(tag);
        for (prop, value) in attributes {
            node.attributes.insert(*prop, (*value).into());
        }
        node.children = control_part_nodes(tag).into();
        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);
        let rules = vec![
            rule(tag, &[("width", "200px"), ("height", "20px"), ("padding", "0px")]),
            rule(&format!("{tag}::track"), &[("height", "8px")]),
        ];
        resolver.restyle_subtree_cached(&mut node, &rules, StyleContext::default(), &mut None);
        LayoutEngine::compute(&mut node, 400.0, 100.0);
        node
    }

    #[test]
    fn progress_and_meter_fill_their_track_to_the_value() {
        for tag in ["progress", "meter"] {
            assert_eq!(control_part_names(tag), &["track", "fill"]);
            let node = progress(tag, &[(Prop::Value, "3"), (Prop::Max, "4")]);
            let track = part_or_none(&node, ControlPart::ProgressTrack).unwrap();
            let fill = part_or_none(&node, ControlPart::ProgressFill).unwrap();
            assert_eq!((track.layout.width, track.layout.height), (200.0, 8.0));
            assert_eq!((fill.layout.width, fill.layout.height), (150.0, 8.0));
        }
        let empty = progress("progress", &[]);
        let fill = part_or_none(&empty, ControlPart::ProgressFill).unwrap();
        assert_eq!(fill.layout.width, 0.0, "a progress without a value starts empty");
        let busy = progress("progress", &[(Prop::Indeterminate, "true"), (Prop::Value, "90")]);
        let fill = part_or_none(&busy, ControlPart::ProgressFill).unwrap();
        assert_eq!(fill.layout.width, 60.0);
    }

    fn styled_with(tag: &str, checked: bool, rules: &[StyleRule]) -> WidgetNode {
        let mut node = WidgetNode::new(tag);
        node.state.set(NodeState::CHECKED, checked);
        node.children = control_part_nodes(tag).into();
        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);
        resolver.restyle_subtree_cached(&mut node, rules, StyleContext::default(), &mut None);
        LayoutEngine::compute(&mut node, 400.0, 100.0);
        node
    }

    #[test]
    fn indicators_mirror_the_checked_host_and_take_checked_rules() {
        let rules = [
            rule("checkbox::indicator", &[("width", "22px"), ("height", "22px")]),
            rule("checkbox:checked::indicator", &[("background-color", "#00ff00")]),
        ];
        let unchecked = styled_with("checkbox", false, &rules);
        let indicator = part_or_none(&unchecked, ControlPart::CheckboxIndicator).unwrap();
        assert_eq!((indicator.layout.width, indicator.layout.height), (22.0, 22.0));
        assert!(!indicator.state.contains(NodeState::CHECKED));
        let checked = styled_with("checkbox", true, &rules);
        let indicator = part_or_none(&checked, ControlPart::CheckboxIndicator).unwrap();
        assert!(indicator.state.contains(NodeState::CHECKED));
        assert_eq!(
            indicator.computed_style.background_color,
            Color::from_hex("#00ff00").unwrap()
        );
        let radio = styled_with("radio", true, &[]);
        let indicator = part_or_none(&radio, ControlPart::RadioIndicator).unwrap();
        assert_eq!((indicator.layout.width, indicator.layout.height), (18.0, 18.0));
        assert_eq!(indicator.computed_style.border_radius, Corners::all(9.0));
    }

    #[test]
    fn switch_thumb_sits_at_the_checked_end_and_glides_with_its_margin_transition() {
        let rules = [
            rule("switch::track", &[("width", "50px"), ("padding", "3px")]),
            rule("switch::thumb", &[("width", "20px"), ("transition", "margin 120ms ease")]),
        ];
        let thumb_x = |node: &WidgetNode| {
            let track = part_or_none(node, ControlPart::SwitchTrack).unwrap();
            part_or_none(node, ControlPart::SwitchThumb).unwrap().layout.x - track.layout.x
        };
        let off = styled_with("switch", false, &rules);
        assert_eq!(thumb_x(&off), 3.0);
        let on = styled_with("switch", true, &rules);
        assert_eq!(thumb_x(&on), 50.0 - 3.0 - 20.0);
        let lead = part_or_none(&on, ControlPart::SwitchLead).unwrap();
        assert_eq!(lead.computed_style.transitions.len(), 1);
        assert_eq!(lead.computed_style.transitions[0].duration_ms, 120);
        assert!(lead.computed_style.transitions[0].properties.width);
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
