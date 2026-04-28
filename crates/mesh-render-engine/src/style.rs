use mesh_ui::style::{Display, FlexDirection};
use mesh_ui::{ComputedStyle, Dimension, StyleContext};

#[derive(Clone, Copy, Default)]
pub(crate) struct InheritedStyleMask {
    color: bool,
    font_family: bool,
    font_size: bool,
    font_weight: bool,
    line_height: bool,
}

pub(crate) fn inherit_text_style(
    style: &mut ComputedStyle,
    parent_style: &ComputedStyle,
    explicit: InheritedStyleMask,
) {
    if !explicit.color {
        style.color = parent_style.color;
    }
    if !explicit.font_family {
        style.font_family = parent_style.font_family.clone();
    }
    if !explicit.font_size {
        style.font_size = parent_style.font_size;
    }
    if !explicit.font_weight {
        style.font_weight = parent_style.font_weight;
    }
    if !explicit.line_height {
        style.line_height = parent_style.line_height;
    }
}

pub(crate) fn inherited_style_mask(
    rules: &[mesh_component::style::StyleRule],
    tag: &str,
    classes: &[String],
    id: Option<&str>,
    context: StyleContext,
) -> InheritedStyleMask {
    let mut mask = InheritedStyleMask::default();

    for rule in rules {
        if !selector_matches(&rule.selector, tag, classes, id)
            || rule.container_query.is_some_and(|query| {
                !query.matches(context.container_width, context.container_height)
            })
        {
            continue;
        }

        for decl in &rule.declarations {
            match decl.property.as_str() {
                "color" => mask.color = true,
                "font-family" => mask.font_family = true,
                "font-size" => mask.font_size = true,
                "font-weight" => mask.font_weight = true,
                "line-height" => mask.line_height = true,
                _ => {}
            }
        }
    }

    mask
}

fn selector_matches(
    selector: &mesh_component::style::Selector,
    tag: &str,
    classes: &[String],
    id: Option<&str>,
) -> bool {
    use mesh_component::style::Selector;

    match selector {
        Selector::Universal => true,
        Selector::Tag(tag_name) => tag_name == tag,
        Selector::Class(class_name) => classes.iter().any(|class| class == class_name),
        Selector::Id(id_name) => id == Some(id_name.as_str()),
        Selector::State(tag_name, _state) => tag_name == "*" || tag_name == tag,
        Selector::Compound(parts) => parts
            .iter()
            .all(|part| selector_matches(part, tag, classes, id)),
    }
}

pub(crate) fn child_style_context(
    style: &ComputedStyle,
    parent_context: StyleContext,
) -> StyleContext {
    let width = (resolve_dimension_for_context(style.width, parent_context.container_width)
        - style.margin.horizontal())
    .max(0.0);
    let height = (resolve_dimension_for_context(style.height, parent_context.container_height)
        - style.margin.vertical())
    .max(0.0);

    StyleContext {
        container_width: (width - style.padding.horizontal()).max(0.0),
        container_height: (height - style.padding.vertical()).max(0.0),
    }
}

fn resolve_dimension_for_context(dimension: Dimension, available: f32) -> f32 {
    match dimension {
        Dimension::Px(px) => px,
        Dimension::Percent(percent) => available * percent / 100.0,
        Dimension::Auto | Dimension::Content => available.max(0.0),
    }
}

pub(crate) fn merge_missing_defaults(tag: &str, style: &mut ComputedStyle) {
    let defaults = default_leaf_style(tag);

    if tag == "icon" {
        style.background_color = mesh_ui::Color::TRANSPARENT;
        style.border_radius = mesh_ui::Corners::zero();
        style.padding = mesh_ui::Edges::zero();
    }

    if style.background_color.a == 0 && defaults.background_color.a > 0 {
        style.background_color = defaults.background_color;
    }
    if style.color.a == 0 {
        style.color = defaults.color;
    }
    if style.padding.top == 0.0
        && style.padding.right == 0.0
        && style.padding.bottom == 0.0
        && style.padding.left == 0.0
    {
        style.padding = defaults.padding;
    }
    if style.gap == 0.0 {
        style.gap = defaults.gap;
    }
    if style.border_radius.top_left == 0.0 {
        style.border_radius = defaults.border_radius;
    }
    if style.overflow_x == ComputedStyle::default().overflow_x {
        style.overflow_x = defaults.overflow_x;
    }
    if style.overflow_y == ComputedStyle::default().overflow_y {
        style.overflow_y = defaults.overflow_y;
    }
    if style.font_size == ComputedStyle::default().font_size {
        style.font_size = defaults.font_size;
    }
    if (tag == "column" || tag == "row") && style.direction != defaults.direction {
        style.direction = defaults.direction;
    }
}

pub(crate) fn surface_style(_surface_id: &str, width: u32, height: u32) -> ComputedStyle {
    let mut style = container_style("column");
    style.padding = mesh_ui::Edges::all(0.0);
    style.gap = 0.0;
    style.width = mesh_ui::Dimension::Px(width as f32);
    style.height = mesh_ui::Dimension::Px(height as f32);
    style.background_color = mesh_ui::Color::TRANSPARENT;
    style
}

pub(crate) fn container_style(tag: &str) -> ComputedStyle {
    let mut style = ComputedStyle::default();
    style.direction = if tag == "column" {
        FlexDirection::Column
    } else {
        FlexDirection::Row
    };
    style.padding = mesh_ui::Edges::all(12.0);
    style.gap = 8.0;
    style.color = mesh_ui::Color::WHITE;
    style
}

pub(crate) fn embedded_root_style() -> ComputedStyle {
    let mut style = container_style("column");
    style.padding = mesh_ui::Edges::all(0.0);
    style.gap = 0.0;
    style.background_color = mesh_ui::Color::TRANSPARENT;
    style.width = mesh_ui::Dimension::Auto;
    style.height = mesh_ui::Dimension::Auto;
    style
}

pub(crate) fn slot_style(tag: &str) -> ComputedStyle {
    let mut style = container_style(tag);
    style.padding = mesh_ui::Edges::all(0.0);
    style.background_color = mesh_ui::Color::TRANSPARENT;
    style.border_radius = mesh_ui::Corners::all(0.0);
    style.width = mesh_ui::Dimension::Auto;
    style.height = mesh_ui::Dimension::Auto;
    style
}

pub(crate) fn text_style() -> ComputedStyle {
    let mut style = ComputedStyle::default();
    style.display = Display::Flex;
    style.color = mesh_ui::Color::WHITE;
    style.font_size = 14.0;
    style.background_color = mesh_ui::Color::TRANSPARENT;
    style
}

fn default_leaf_style(tag: &str) -> ComputedStyle {
    let mut style = match tag {
        "column" | "row" => container_style(tag),
        "button" => {
            let mut style = container_style("row");
            style.background_color =
                mesh_ui::Color::from_hex("#2b2633").unwrap_or(mesh_ui::Color::BLACK);
            style.border_radius = mesh_ui::Corners::all(12.0);
            style.padding = mesh_ui::Edges::all(10.0);
            style
        }
        "input" => {
            let mut style = container_style("row");
            style.background_color =
                mesh_ui::Color::from_hex("#221f28").unwrap_or(mesh_ui::Color::BLACK);
            style.border_radius = mesh_ui::Corners::all(10.0);
            style.padding = mesh_ui::Edges::all(10.0);
            style.height = mesh_ui::Dimension::Px(44.0);
            style.border_width = mesh_ui::Edges::all(1.0);
            style.border_color =
                mesh_ui::Color::from_hex("#3b3644").unwrap_or(mesh_ui::Color::WHITE);
            style
        }
        "slider" => {
            let mut style = container_style("row");
            style.height = mesh_ui::Dimension::Px(36.0);
            style.padding = mesh_ui::Edges::all(8.0);
            style
        }
        "scroll" => {
            let mut style = container_style("column");
            style.background_color = mesh_ui::Color::TRANSPARENT;
            style.height = mesh_ui::Dimension::Px(220.0);
            style.padding = mesh_ui::Edges::all(0.0);
            style.overflow_x = mesh_ui::Overflow::Hidden;
            style.overflow_y = mesh_ui::Overflow::Auto;
            style
        }
        "icon" => {
            let mut style = ComputedStyle::default();
            style.width = mesh_ui::Dimension::Px(18.0);
            style.height = mesh_ui::Dimension::Px(18.0);
            style.background_color = mesh_ui::Color::TRANSPARENT;
            style
        }
        "box" => {
            let mut style = ComputedStyle::default();
            style.background_color = mesh_ui::Color::TRANSPARENT;
            style
        }
        "text" => text_style(),
        _ => container_style("column"),
    };

    if tag == "text" {
        style.height = mesh_ui::Dimension::Px(22.0);
    }

    style
}
