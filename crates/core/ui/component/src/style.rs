//! Style AST — CSS-like styling with theme token references.

/// The style block containing all rules for a component.
#[derive(Debug, Clone)]
pub struct StyleBlock {
    pub rules: Vec<StyleRule>,
    pub keyframes: Vec<KeyframeRule>,
    /// The style body in the owning `.mesh` source.
    pub span: crate::SourceSpan,
}

/// A single style rule: selector + declarations.
#[derive(Debug, Clone)]
pub struct StyleRule {
    pub selector: Selector,
    pub declarations: Vec<Declaration>,
    pub container_query: Option<ContainerQuery>,
}

/// A simplified CSS container query evaluated against the current container size.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ContainerQuery {
    pub min_width: Option<f32>,
    pub max_width: Option<f32>,
    pub min_height: Option<f32>,
    pub max_height: Option<f32>,
}

impl ContainerQuery {
    pub fn matches(&self, width: f32, height: f32) -> bool {
        if let Some(min_width) = self.min_width
            && width < min_width
        {
            return false;
        }
        if let Some(max_width) = self.max_width
            && width > max_width
        {
            return false;
        }
        if let Some(min_height) = self.min_height
            && height < min_height
        {
            return false;
        }
        if let Some(max_height) = self.max_height
            && height > max_height
        {
            return false;
        }

        true
    }

    pub fn intersect(self, other: Self) -> Self {
        Self {
            min_width: match (self.min_width, other.min_width) {
                (Some(left), Some(right)) => Some(left.max(right)),
                (Some(value), None) | (None, Some(value)) => Some(value),
                (None, None) => None,
            },
            max_width: match (self.max_width, other.max_width) {
                (Some(left), Some(right)) => Some(left.min(right)),
                (Some(value), None) | (None, Some(value)) => Some(value),
                (None, None) => None,
            },
            min_height: match (self.min_height, other.min_height) {
                (Some(left), Some(right)) => Some(left.max(right)),
                (Some(value), None) | (None, Some(value)) => Some(value),
                (None, None) => None,
            },
            max_height: match (self.max_height, other.max_height) {
                (Some(left), Some(right)) => Some(left.min(right)),
                (Some(value), None) | (None, Some(value)) => Some(value),
                (None, None) => None,
            },
        }
    }
}

pub use mesh_core_theme::css::Selector;

/// A pre-compiled selector shape used at runtime by `StyleResolver`.
///
/// Lowered from `Selector` during the compile/lower stage. Only selectors that
/// MESH can afford to match at runtime are representable here; unsupported
/// selectors (descendant combinators, `:has()`, etc.) are rejected with a
/// diagnostic before reaching this type.
#[derive(Debug, Clone)]
pub enum LoweredSelector {
    Simple(SimpleSelector),
    State(SimpleSelector, StateSelector),
}

/// The structural part of a lowered selector: optional tag, optional id, class set.
#[derive(Debug, Clone, Default)]
pub struct SimpleSelector {
    pub tag: Option<String>,
    pub id: Option<String>,
    pub classes: Vec<String>,
}

/// Runtime state that a selector can match against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateSelector {
    Hover,
    Focus,
    Active,
    Disabled,
    Checked,
}

/// A single CSS-like property declaration.
#[derive(Debug, Clone)]
pub struct Declaration {
    pub property: String,
    pub value: StyleValue,
}

/// Where the jumps land in a CSS `steps()` timing function. The legacy `start`
/// / `end` keywords map onto `JumpStart` / `JumpEnd`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum StepPosition {
    /// Jump at the start of each interval (`jump-start` / `start`).
    JumpStart,
    /// Jump at the end of each interval (`jump-end` / `end`). CSS default.
    #[default]
    JumpEnd,
    /// No jump at either end — `n` stops including both 0 and 1 (`jump-none`).
    JumpNone,
    /// Jump at both ends — neither 0 nor 1 is held (`jump-both`).
    JumpBoth,
}

/// A validated CSS timing function shared by component parsing and computed
/// element animation styles.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum TransitionEasing {
    Linear,
    Ease,
    EaseIn,
    #[default]
    EaseOut,
    EaseInOut,
    CubicBezier(f32, f32, f32, f32),
    Steps(u32, StepPosition),
}

impl std::hash::Hash for TransitionEasing {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            Self::CubicBezier(a, b, c, d) => {
                a.to_bits().hash(state);
                b.to_bits().hash(state);
                c.to_bits().hash(state);
                d.to_bits().hash(state);
            }
            Self::Steps(count, position) => {
                count.hash(state);
                position.hash(state);
            }
            _ => {}
        }
    }
}

/// Parse one of the timing functions supported by MESH's animation sampler.
///
/// Component parsing uses this strict form so a keyframe-local timing
/// function can never enter the runtime as an unvalidated string. The element
/// style resolver may still choose its historical defaulting behavior for
/// unsupported authored declarations.
pub fn parse_easing(value: &str) -> Option<TransitionEasing> {
    let trimmed = value.trim();
    match trimmed {
        "linear" => Some(TransitionEasing::Linear),
        "ease" => Some(TransitionEasing::Ease),
        "ease-in" => Some(TransitionEasing::EaseIn),
        "ease-out" => Some(TransitionEasing::EaseOut),
        "ease-in-out" => Some(TransitionEasing::EaseInOut),
        "step-start" => Some(TransitionEasing::Steps(1, StepPosition::JumpStart)),
        "step-end" => Some(TransitionEasing::Steps(1, StepPosition::JumpEnd)),
        _ if trimmed.starts_with("steps(") => parse_steps(trimmed),
        _ => parse_cubic_bezier(trimmed),
    }
}

fn parse_steps(value: &str) -> Option<TransitionEasing> {
    let inner = value
        .strip_prefix("steps(")
        .and_then(|rest| rest.strip_suffix(')'))?;
    let mut parts = inner.split(',');
    let count = parts.next()?.trim().parse::<u32>().ok()?;
    if count == 0 {
        return None;
    }
    let position = match parts.next().map(str::trim) {
        None => StepPosition::JumpEnd,
        Some("jump-start") | Some("start") => StepPosition::JumpStart,
        Some("jump-end") | Some("end") => StepPosition::JumpEnd,
        Some("jump-none") => StepPosition::JumpNone,
        Some("jump-both") => StepPosition::JumpBoth,
        Some(_) => return None,
    };
    if parts.next().is_some() {
        return None;
    }
    Some(TransitionEasing::Steps(count, position))
}

fn parse_cubic_bezier(value: &str) -> Option<TransitionEasing> {
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

/// A named `@keyframes` rule parsed from a style block.
#[derive(Debug, Clone)]
pub struct KeyframeRule {
    pub name: String,
    pub stops: Vec<KeyframeStop>,
}

/// A single percentage stop within a keyframe rule.
#[derive(Debug, Clone)]
pub struct KeyframeStop {
    pub offset: f32,
    pub declarations: Vec<Declaration>,
    /// Segment-local timing function starting at this stop.
    pub easing: Option<TransitionEasing>,
}

/// Reserved variable-store key prefix under which resolved component-prop values
/// are published, so `StyleValue::Prop(name)` resolves through the same variable
/// map as `var(--…)`. See `docs/spec/03-components.md`.
pub const PROP_VAR_PREFIX: &str = "--mesh-prop-";

/// The variable-store key that holds the resolved value of prop `name`.
pub fn prop_variable_key(name: &str) -> String {
    format!("{PROP_VAR_PREFIX}{name}")
}

/// A style value that may reference local or theme CSS variables.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum StyleValue {
    /// A literal value: `#ff0000`, `16px`, `bold`.
    Literal(String),
    /// A variable reference: `var(--custom-prop)`.
    Var(String),
    /// A component-prop reference: `prop(name)`. Resolved against the per-instance
    /// prop value map (published under `prop_variable_key(name)`).
    Prop(String),
}

/// Classify one lowered CSS value while preserving the shared theme variable
/// syntax. Embedded references remain literals and are resolved by the style
/// resolver after component props and inherited custom properties are known.
pub fn classify_style_value(value: &str) -> StyleValue {
    let value = value.trim();
    if let Some(name) = mesh_core_theme::css::variable_reference(value) {
        StyleValue::Var(name)
    } else if let Some(name) = standalone_prop_reference(value) {
        StyleValue::Prop(name)
    } else {
        StyleValue::Literal(value.to_string())
    }
}

fn standalone_prop_reference(value: &str) -> Option<String> {
    let inner = value.strip_prefix("prop(")?.strip_suffix(')')?;
    if inner.contains('(') || inner.contains(')') {
        return None;
    }
    Some(inner.trim().to_string())
}

pub fn is_transition_safe_keyframe_property(property: &str) -> bool {
    mesh_core_theme::css::is_transition_safe_keyframe_property(property)
}
