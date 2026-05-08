use super::*;

pub(super) fn parse_edges_shorthand(value: &str) -> Edges {
    let values = shorthand_numbers(value);
    match values.as_slice() {
        [all] => Edges::all(*all),
        [vertical, horizontal] => Edges {
            top: *vertical,
            right: *horizontal,
            bottom: *vertical,
            left: *horizontal,
        },
        [top, horizontal, bottom] => Edges {
            top: *top,
            right: *horizontal,
            bottom: *bottom,
            left: *horizontal,
        },
        [top, right, bottom, left] => Edges {
            top: *top,
            right: *right,
            bottom: *bottom,
            left: *left,
        },
        _ => Edges::zero(),
    }
}

pub(super) fn parse_transform(value: &str) -> Transform2D {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "none" {
        return Transform2D::IDENTITY;
    }

    let mut transform = Transform2D::IDENTITY;
    let mut rest = trimmed;
    while !rest.is_empty() {
        rest = rest.trim_start();
        let Some(open) = rest.find('(') else {
            break;
        };
        let name = rest[..open].trim();
        let after_open = &rest[open + 1..];
        let Some(close) = after_open.find(')') else {
            break;
        };
        let args_str = &after_open[..close];
        let args: Vec<f32> = args_str
            .split(|c: char| c == ',' || c.is_whitespace())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(parse_transform_length)
            .collect();
        let angle_arg: Option<f32> = args_str
            .split(|c: char| c == ',' || c.is_whitespace())
            .map(str::trim)
            .find(|s| !s.is_empty())
            .map(parse_transform_angle);

        match name {
            "translate" => {
                if let Some(&x) = args.first() {
                    transform.translate_x += x;
                }
                if let Some(&y) = args.get(1) {
                    transform.translate_y += y;
                }
            }
            "translateX" => {
                if let Some(&x) = args.first() {
                    transform.translate_x += x;
                }
            }
            "translateY" => {
                if let Some(&y) = args.first() {
                    transform.translate_y += y;
                }
            }
            "scale" => {
                if let Some(&sx) = args.first() {
                    transform.scale_x *= sx;
                    let sy = args.get(1).copied().unwrap_or(sx);
                    transform.scale_y *= sy;
                }
            }
            "scaleX" => {
                if let Some(&sx) = args.first() {
                    transform.scale_x *= sx;
                }
            }
            "scaleY" => {
                if let Some(&sy) = args.first() {
                    transform.scale_y *= sy;
                }
            }
            "rotate" => {
                if let Some(angle) = angle_arg {
                    transform.rotation += angle;
                }
            }
            _ => {}
        }

        rest = &after_open[close + 1..];
    }
    transform
}

pub(super) fn parse_transform_length(token: &str) -> f32 {
    let token = token.trim();
    if let Some(rest) = token.strip_suffix("px") {
        rest.trim().parse::<f32>().unwrap_or(0.0)
    } else {
        token.parse::<f32>().unwrap_or(0.0)
    }
}

pub(super) fn parse_transform_angle(token: &str) -> f32 {
    let token = token.trim();
    if let Some(rest) = token.strip_suffix("deg") {
        rest.trim().parse::<f32>().unwrap_or(0.0).to_radians()
    } else if let Some(rest) = token.strip_suffix("turn") {
        rest.trim().parse::<f32>().unwrap_or(0.0) * std::f32::consts::TAU
    } else if let Some(rest) = token.strip_suffix("rad") {
        rest.trim().parse::<f32>().unwrap_or(0.0)
    } else {
        token.parse::<f32>().unwrap_or(0.0).to_radians()
    }
}

pub(super) fn parse_corners_shorthand(value: &str) -> Corners {
    let edges = parse_edges_shorthand(value);
    Corners {
        top_left: edges.top,
        top_right: edges.right,
        bottom_right: edges.bottom,
        bottom_left: edges.left,
    }
}

pub(super) fn parse_border_color_shorthand(value: &str) -> Color {
    value
        .split_whitespace()
        .find_map(Color::from_hex)
        .or_else(|| Color::from_hex(value.trim()))
        .unwrap_or(Color::TRANSPARENT)
}

pub(super) fn apply_border_shorthand(style: &mut ComputedStyle, value: &str) {
    if value.trim() == "none" {
        style.border_width = Edges::zero();
        style.border_color = Color::TRANSPARENT;
        return;
    }

    for token in value.split_whitespace() {
        if token.ends_with("px") || token.parse::<f32>().is_ok() {
            style.border_width = Edges::all(parse_px(token));
        } else if let Some(color) = Color::from_hex(token) {
            style.border_color = color;
        }
    }
}

pub(super) fn apply_flex_shorthand(style: &mut ComputedStyle, value: &str) {
    let parts = value.split_whitespace().collect::<Vec<_>>();
    if parts.len() >= 3 {
        if let Ok(grow) = parts[0].parse::<f32>() {
            style.flex_grow = grow;
        }
        if let Ok(shrink) = parts[1].parse::<f32>() {
            style.flex_shrink = shrink;
        }
        style.flex_basis = parse_dimension(parts[2]);
    }
}

pub(super) fn apply_font_shorthand(style: &mut ComputedStyle, value: &str) {
    let mut family_parts = Vec::new();
    let mut saw_size = false;

    for token in value.split_whitespace() {
        if token == "italic" || token == "oblique" {
            style.font_style = FontStyle::Italic;
        } else if token == "normal" {
            style.font_style = FontStyle::Normal;
        } else if let Ok(weight) = token.parse::<u16>() {
            style.font_weight = weight;
        } else if token.contains("px") {
            let mut size_parts = token.split('/');
            if let Some(size) = size_parts.next() {
                style.font_size = parse_px(size);
                saw_size = true;
            }
            if let Some(line_height) = size_parts.next() {
                style.line_height = parse_px(line_height);
            }
        } else if saw_size {
            family_parts.push(token.trim_matches('"').trim_matches('\''));
        }
    }

    if !family_parts.is_empty() {
        style.font_family = family_parts.join(" ");
    }
}

pub(super) fn parse_overflow_shorthand(value: &str) -> (Overflow, Overflow) {
    let parts = value.split_whitespace().collect::<Vec<_>>();
    match parts.as_slice() {
        [one] => {
            let overflow = parse_overflow(one);
            (overflow, overflow)
        }
        [x, y, ..] => (parse_overflow(x), parse_overflow(y)),
        [] => (Overflow::Visible, Overflow::Visible),
    }
}

pub(super) fn parse_overflow(value: &str) -> Overflow {
    match value.trim() {
        "hidden" => Overflow::Hidden,
        "auto" => Overflow::Auto,
        "scroll" => Overflow::Scroll,
        _ => Overflow::Visible,
    }
}

pub(super) fn parse_transition_properties(value: &str) -> TransitionProperties {
    let mut properties = TransitionProperties::none();
    for property in value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        match property {
            "all" => return TransitionProperties::all(),
            "border-radius" => properties.border_radius = true,
            "border-width" => properties.border_width = true,
            "opacity" => properties.opacity = true,
            "background-color" | "background" => properties.background_color = true,
            "border-color" => properties.border_color = true,
            "color" => properties.color = true,
            "width" => properties.width = true,
            "height" => properties.height = true,
            "padding" => properties.padding = true,
            "margin" => properties.margin = true,
            "transform" => properties.transform = true,
            "min-width" => properties.min_width = true,
            "max-width" => properties.max_width = true,
            "min-height" => properties.min_height = true,
            "max-height" => properties.max_height = true,
            "font-size" => properties.font_size = true,
            "letter-spacing" => properties.letter_spacing = true,
            "line-height" => properties.line_height = true,
            "gap" => properties.gap = true,
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
            _ => {}
        }
    }
    properties
}

pub(super) fn first_comma_item(value: &str) -> &str {
    let mut depth: i32 = 0;
    let mut split_at = value.len();
    for (idx, ch) in value.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth = (depth - 1).max(0),
            ',' if depth == 0 => {
                split_at = idx;
                break;
            }
            _ => {}
        }
    }
    value[..split_at].trim()
}

pub(super) fn parse_first_time_ms(value: &str) -> u32 {
    parse_time_ms(first_comma_item(value))
}

pub(super) fn split_paren_aware(value: &str, delim: char) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth: i32 = 0;
    let mut buf = String::new();
    for ch in value.chars() {
        match ch {
            '(' => {
                depth += 1;
                buf.push(ch);
            }
            ')' => {
                depth = (depth - 1).max(0);
                buf.push(ch);
            }
            c if c == delim && depth == 0 => {
                out.push(std::mem::take(&mut buf));
            }
            _ => buf.push(ch),
        }
    }
    if !buf.is_empty() {
        out.push(buf);
    }
    out
}

pub(super) fn parse_easing_keyword(value: &str) -> TransitionEasing {
    let trimmed = value.trim();
    match trimmed {
        "linear" => TransitionEasing::Linear,
        "ease" => TransitionEasing::Ease,
        "ease-in" => TransitionEasing::EaseIn,
        "ease-out" => TransitionEasing::EaseOut,
        "ease-in-out" => TransitionEasing::EaseInOut,
        _ => parse_cubic_bezier(trimmed).unwrap_or(TransitionEasing::EaseOut),
    }
}

pub(super) fn parse_cubic_bezier(value: &str) -> Option<TransitionEasing> {
    let inner = value
        .strip_prefix("cubic-bezier(")
        .and_then(|rest| rest.strip_suffix(')'))?;
    let parts: Vec<f32> = inner
        .split(',')
        .map(|part| part.trim().parse::<f32>())
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    if parts.len() != 4 {
        return None;
    }
    Some(TransitionEasing::CubicBezier(
        parts[0].clamp(0.0, 1.0),
        parts[1],
        parts[2].clamp(0.0, 1.0),
        parts[3],
    ))
}

pub(super) fn looks_like_time(token: &str) -> bool {
    if let Some(rest) = token.strip_suffix("ms") {
        rest.trim().parse::<f32>().is_ok()
    } else if let Some(rest) = token.strip_suffix('s') {
        rest.trim().parse::<f32>().is_ok()
    } else {
        token.trim().parse::<f32>().is_ok()
    }
}

pub(super) fn parse_transition_shorthand(
    value: &str,
) -> (TransitionProperties, u32, u32, TransitionEasing) {
    let mut properties = TransitionProperties::none();
    let mut duration_ms = 0u32;
    let mut delay_ms = 0u32;
    let mut easing = TransitionEasing::EaseOut;

    for item in split_paren_aware(value, ',') {
        let item = item.trim();
        if item.is_empty() {
            continue;
        }
        let mut item_time_count = 0;
        for token in split_paren_aware(item, ' ')
            .iter()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            if looks_like_time(token) {
                let ms = parse_time_ms(token);
                if item_time_count == 0 && duration_ms == 0 {
                    duration_ms = ms;
                } else if item_time_count > 0 && delay_ms == 0 {
                    delay_ms = ms;
                }
                item_time_count += 1;
                continue;
            }
            match token {
                "all" => properties = TransitionProperties::all(),
                "border-radius" => properties.border_radius = true,
                "border-width" => properties.border_width = true,
                "opacity" => properties.opacity = true,
                "background-color" | "background" => properties.background_color = true,
                "border-color" => properties.border_color = true,
                "color" => properties.color = true,
                "width" => properties.width = true,
                "height" => properties.height = true,
                "padding" => properties.padding = true,
                "margin" => properties.margin = true,
                "transform" => properties.transform = true,
                "min-width" => properties.min_width = true,
                "max-width" => properties.max_width = true,
                "min-height" => properties.min_height = true,
                "max-height" => properties.max_height = true,
                "font-size" => properties.font_size = true,
                "letter-spacing" => properties.letter_spacing = true,
                "line-height" => properties.line_height = true,
                "gap" => properties.gap = true,
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
                "linear" | "ease" | "ease-in" | "ease-out" | "ease-in-out"
                    if easing == TransitionEasing::EaseOut =>
                {
                    easing = parse_easing_keyword(token)
                }
                _ if token.starts_with("cubic-bezier(") && easing == TransitionEasing::EaseOut => {
                    easing = parse_easing_keyword(token);
                }
                _ => {}
            }
        }
    }

    (properties, duration_ms, delay_ms, easing)
}

pub(super) fn parse_animation_name(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() || value == "none" {
        None
    } else {
        Some(value.to_string())
    }
}

pub(super) fn parse_animation_iteration_count(value: &str) -> AnimationIterationCount {
    let value = value.trim();
    if value == "infinite" {
        AnimationIterationCount::Infinite
    } else {
        AnimationIterationCount::Number(value.parse::<u32>().unwrap_or(1))
    }
}

pub(super) fn parse_animation_direction(value: &str) -> AnimationDirection {
    match value.trim() {
        "reverse" => AnimationDirection::Reverse,
        "alternate" => AnimationDirection::Alternate,
        "alternate-reverse" => AnimationDirection::AlternateReverse,
        _ => AnimationDirection::Normal,
    }
}

pub(super) fn parse_animation_fill_mode(value: &str) -> AnimationFillMode {
    match value.trim() {
        "forwards" => AnimationFillMode::Forwards,
        "backwards" => AnimationFillMode::Backwards,
        "both" => AnimationFillMode::Both,
        _ => AnimationFillMode::None,
    }
}

pub(super) fn parse_animation_play_state(value: &str) -> AnimationPlayState {
    match value.trim() {
        "paused" => AnimationPlayState::Paused,
        _ => AnimationPlayState::Running,
    }
}

pub(super) fn parse_animation_shorthand(value: &str) -> AnimationStyle {
    let mut animation = AnimationStyle::default();
    let mut time_count = 0;

    for token in first_comma_item(value).split_whitespace() {
        if looks_like_explicit_time(token) {
            let ms = parse_time_ms(token);
            if time_count == 0 {
                animation.duration_ms = ms;
            } else {
                animation.delay_ms = ms;
            }
            time_count += 1;
        } else if matches!(
            token,
            "linear" | "ease" | "ease-in" | "ease-out" | "ease-in-out"
        ) {
            animation.easing = parse_easing_keyword(token);
        } else if token == "infinite" || token.parse::<u32>().is_ok() {
            animation.iteration_count = parse_animation_iteration_count(token);
        } else if matches!(
            token,
            "normal" | "reverse" | "alternate" | "alternate-reverse"
        ) {
            animation.direction = parse_animation_direction(token);
        } else if matches!(token, "none" | "forwards" | "backwards" | "both") {
            animation.fill_mode = parse_animation_fill_mode(token);
        } else if matches!(token, "running" | "paused") {
            animation.play_state = parse_animation_play_state(token);
        } else {
            animation.name = parse_animation_name(token);
        }
    }

    animation
}

fn looks_like_explicit_time(token: &str) -> bool {
    let token = token.trim();
    token
        .strip_suffix("ms")
        .is_some_and(|rest| rest.trim().parse::<f32>().is_ok())
        || token
            .strip_suffix('s')
            .is_some_and(|rest| rest.trim().parse::<f32>().is_ok())
}

pub(super) fn parse_time_ms(value: &str) -> u32 {
    let raw = value.trim();
    if let Some(ms) = raw.strip_suffix("ms") {
        return ms.trim().parse::<f32>().unwrap_or(0.0).max(0.0).round() as u32;
    }
    if let Some(seconds) = raw.strip_suffix('s') {
        return (seconds.trim().parse::<f32>().unwrap_or(0.0).max(0.0) * 1000.0).round() as u32;
    }
    raw.parse::<f32>()
        .ok()
        .map(|v| v.max(0.0).round() as u32)
        .unwrap_or(0)
}

pub(super) fn parse_px(s: &str) -> f32 {
    let s = s.trim().trim_end_matches("px");
    s.parse().unwrap_or(0.0)
}

pub(super) fn parse_dimension(s: &str) -> Dimension {
    let s = s.trim();
    match s {
        "auto" => Dimension::Auto,
        "content" | "fit-content" | "max-content" | "min-content" => Dimension::Content,
        _ if s.ends_with('%') => Dimension::Percent(s.trim_end_matches('%').parse().unwrap_or(0.0)),
        _ => Dimension::Px(parse_px(s)),
    }
}

fn shorthand_numbers(value: &str) -> Vec<f32> {
    value
        .split_whitespace()
        .take(4)
        .map(parse_px)
        .collect::<Vec<_>>()
}
