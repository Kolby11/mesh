//! Typed placement model for `<popover>` surface promotion.
//!
//! A `<popover>` is authored inline but promoted at runtime into its own
//! `xdg_popup` child surface, because the host buffer is a fixed pixel rectangle
//! that drops out-of-bounds writes — a menu hanging below a short bar needs a
//! surface of its own.
//!
//! The field set maps 1:1 onto `xdg_positioner`, so the Wayland translation in
//! `mesh-core-presentation` is a direct enum mapping with no policy of its own.
//! Placement is parsed from attributes:
//! `<popover anchor="bottom" gravity="bottom" offset-y="4" grab="hover">`.

use crate::tree::WidgetNode;

/// The placement field that produced a [`PopoverPlacementDiagnostic`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopoverPlacementField {
    Anchor,
    Gravity,
    OffsetX,
    OffsetY,
    Grab,
    Constraint,
}

impl std::fmt::Display for PopoverPlacementField {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Anchor => "anchor",
            Self::Gravity => "gravity",
            Self::OffsetX => "offset-x",
            Self::OffsetY => "offset-y",
            Self::Grab => "grab",
            Self::Constraint => "constrain",
        })
    }
}

/// Why an author-facing placement token was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopoverPlacementDiagnosticKind {
    Empty,
    UnknownToken,
    InvalidInteger,
    ConflictingTokens,
}

impl std::fmt::Display for PopoverPlacementDiagnosticKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Empty => "empty value",
            Self::UnknownToken => "unknown token",
            Self::InvalidInteger => "invalid integer",
            Self::ConflictingTokens => "conflicting tokens",
        })
    }
}

/// Structured validation output for a popover placement attribute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PopoverPlacementDiagnostic {
    pub field: PopoverPlacementField,
    pub value: String,
    pub kind: PopoverPlacementDiagnosticKind,
}

impl PopoverPlacementDiagnostic {
    pub fn code(&self) -> &'static str {
        match self.kind {
            PopoverPlacementDiagnosticKind::Empty => "popover_placement_empty",
            PopoverPlacementDiagnosticKind::UnknownToken => "popover_placement_unknown_token",
            PopoverPlacementDiagnosticKind::InvalidInteger => "popover_placement_invalid_integer",
            PopoverPlacementDiagnosticKind::ConflictingTokens => {
                "popover_placement_conflicting_tokens"
            }
        }
    }
}

impl std::fmt::Display for PopoverPlacementDiagnostic {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "popover placement {} has {}: '{}'",
            self.field, self.kind, self.value
        )
    }
}

/// Edge or corner of the anchor rectangle the popup positions against.
/// Mirrors `xdg_positioner.anchor`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PopoverAnchor {
    Center,
    Top,
    #[default]
    Bottom,
    Left,
    Right,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

/// Direction the popup grows away from the anchor point. Mirrors
/// `xdg_positioner.gravity`; a menu below a top bar has gravity `Bottom`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PopoverGravity {
    Center,
    Top,
    #[default]
    Bottom,
    Left,
    Right,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

/// Mirrors `xdg_positioner.constraint_adjustment`. Flip+slide is on by default
/// on both axes so an edge-anchored menu re-anchors instead of clipping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PopoverConstraintAdjustment {
    pub flip_x: bool,
    pub flip_y: bool,
    pub slide_x: bool,
    pub slide_y: bool,
    pub resize_x: bool,
    pub resize_y: bool,
}

impl Default for PopoverConstraintAdjustment {
    fn default() -> Self {
        Self {
            flip_x: true,
            flip_y: true,
            slide_x: true,
            slide_y: true,
            resize_x: false,
            resize_y: false,
        }
    }
}

/// An `xdg_popup` grab needs a recent input serial, so it can only be taken in
/// response to a click; a hover-opened popover cannot grab. [`PopoverGrab::Hover`]
/// therefore leaves dismissal to the core popover controller (letting the cursor
/// travel from trigger to popup), while [`PopoverGrab::Click`] takes the
/// compositor grab and gets outside-click dismissal and a keyboard grab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PopoverGrab {
    #[default]
    Hover,
    Click,
}

/// Fully resolved placement for a promoted `<popover>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PopoverPlacement {
    pub anchor: PopoverAnchor,
    pub gravity: PopoverGravity,
    /// Extra offset applied to the computed position, in surface-local pixels.
    pub offset_x: i32,
    pub offset_y: i32,
    pub constraint_adjustment: PopoverConstraintAdjustment,
    pub grab: PopoverGrab,
}

impl PopoverPlacement {
    /// Missing attributes fall back to menu-friendly defaults: anchor and
    /// gravity below, flip+slide on, hover grab.
    pub fn from_node(node: &WidgetNode) -> Result<Self, Vec<PopoverPlacementDiagnostic>> {
        Self::from_attributes(&node.attributes)
    }

    /// Parse placement from a raw attribute map.
    pub fn from_attributes(
        attrs: &crate::attributes::AttributeMap,
    ) -> Result<Self, Vec<PopoverPlacementDiagnostic>> {
        let mut placement = Self::default();
        let mut diagnostics = Vec::new();

        if let Some(value) = attrs.get("anchor") {
            match parse_anchor(value) {
                Ok(anchor) => placement.anchor = anchor,
                Err(diagnostic) => diagnostics.push(diagnostic),
            }
        }
        if let Some(value) = attrs.get("gravity") {
            match parse_gravity(value) {
                Ok(gravity) => placement.gravity = gravity,
                Err(diagnostic) => diagnostics.push(diagnostic),
            }
        } else if attrs.get("gravity").is_none() && attrs.contains_key("anchor") {
            // With only an anchor given, match gravity to it so a popover
            // "anchored left" also grows left without restating it.
            placement.gravity = gravity_for_anchor(placement.anchor);
        }
        if let Some(value) = attrs.get("offset-x") {
            match parse_offset(value, PopoverPlacementField::OffsetX) {
                Ok(offset) => placement.offset_x = offset,
                Err(diagnostic) => diagnostics.push(diagnostic),
            }
        }
        if let Some(value) = attrs.get("offset-y") {
            match parse_offset(value, PopoverPlacementField::OffsetY) {
                Ok(offset) => placement.offset_y = offset,
                Err(diagnostic) => diagnostics.push(diagnostic),
            }
        }
        if let Some(value) = attrs.get("grab") {
            match parse_grab(value) {
                Ok(grab) => placement.grab = grab,
                Err(diagnostic) => diagnostics.push(diagnostic),
            }
        }
        if let Some(value) = attrs.get("constrain") {
            match parse_constraint(value) {
                Ok(adjustment) => placement.constraint_adjustment = adjustment,
                Err(diagnostic) => diagnostics.push(diagnostic),
            }
        }

        if diagnostics.is_empty() {
            Ok(placement)
        } else {
            Err(diagnostics)
        }
    }
}

fn invalid_token(field: PopoverPlacementField, value: &str) -> PopoverPlacementDiagnostic {
    PopoverPlacementDiagnostic {
        field,
        value: value.to_string(),
        kind: if value.trim().is_empty() {
            PopoverPlacementDiagnosticKind::Empty
        } else {
            PopoverPlacementDiagnosticKind::UnknownToken
        },
    }
}

fn parse_anchor(value: &str) -> Result<PopoverAnchor, PopoverPlacementDiagnostic> {
    Ok(match value.trim().to_ascii_lowercase().as_str() {
        "center" => PopoverAnchor::Center,
        "top" => PopoverAnchor::Top,
        "bottom" => PopoverAnchor::Bottom,
        "left" => PopoverAnchor::Left,
        "right" => PopoverAnchor::Right,
        "top-left" | "top_left" => PopoverAnchor::TopLeft,
        "top-right" | "top_right" => PopoverAnchor::TopRight,
        "bottom-left" | "bottom_left" => PopoverAnchor::BottomLeft,
        "bottom-right" | "bottom_right" => PopoverAnchor::BottomRight,
        _ => return Err(invalid_token(PopoverPlacementField::Anchor, value)),
    })
}

fn parse_gravity(value: &str) -> Result<PopoverGravity, PopoverPlacementDiagnostic> {
    Ok(match value.trim().to_ascii_lowercase().as_str() {
        "center" => PopoverGravity::Center,
        "top" => PopoverGravity::Top,
        "bottom" => PopoverGravity::Bottom,
        "left" => PopoverGravity::Left,
        "right" => PopoverGravity::Right,
        "top-left" | "top_left" => PopoverGravity::TopLeft,
        "top-right" | "top_right" => PopoverGravity::TopRight,
        "bottom-left" | "bottom_left" => PopoverGravity::BottomLeft,
        "bottom-right" | "bottom_right" => PopoverGravity::BottomRight,
        _ => return Err(invalid_token(PopoverPlacementField::Gravity, value)),
    })
}

fn gravity_for_anchor(anchor: PopoverAnchor) -> PopoverGravity {
    match anchor {
        PopoverAnchor::Center => PopoverGravity::Center,
        PopoverAnchor::Top => PopoverGravity::Top,
        PopoverAnchor::Bottom => PopoverGravity::Bottom,
        PopoverAnchor::Left => PopoverGravity::Left,
        PopoverAnchor::Right => PopoverGravity::Right,
        PopoverAnchor::TopLeft => PopoverGravity::TopLeft,
        PopoverAnchor::TopRight => PopoverGravity::TopRight,
        PopoverAnchor::BottomLeft => PopoverGravity::BottomLeft,
        PopoverAnchor::BottomRight => PopoverGravity::BottomRight,
    }
}

fn parse_grab(value: &str) -> Result<PopoverGrab, PopoverPlacementDiagnostic> {
    Ok(match value.trim().to_ascii_lowercase().as_str() {
        "hover" | "none" => PopoverGrab::Hover,
        "click" | "grab" => PopoverGrab::Click,
        _ => return Err(invalid_token(PopoverPlacementField::Grab, value)),
    })
}

fn parse_offset(
    value: &str,
    field: PopoverPlacementField,
) -> Result<i32, PopoverPlacementDiagnostic> {
    if value.trim().is_empty() {
        return Err(PopoverPlacementDiagnostic {
            field,
            value: value.to_string(),
            kind: PopoverPlacementDiagnosticKind::Empty,
        });
    }
    value
        .parse::<i32>()
        .map_err(|_| PopoverPlacementDiagnostic {
            field,
            value: value.to_string(),
            kind: PopoverPlacementDiagnosticKind::InvalidInteger,
        })
}

/// Space/comma separated, e.g. `"flip-y slide-x"`; empty or `"none"` disables
/// every adjustment.
fn parse_constraint(
    value: &str,
) -> Result<PopoverConstraintAdjustment, PopoverPlacementDiagnostic> {
    if value.trim().is_empty() {
        return Err(PopoverPlacementDiagnostic {
            field: PopoverPlacementField::Constraint,
            value: value.to_string(),
            kind: PopoverPlacementDiagnosticKind::Empty,
        });
    }
    let mut adjust = PopoverConstraintAdjustment {
        flip_x: false,
        flip_y: false,
        slide_x: false,
        slide_y: false,
        resize_x: false,
        resize_y: false,
    };
    let tokens: Vec<_> = value.split([' ', ',']).filter(|t| !t.is_empty()).collect();
    for token in &tokens {
        match token.trim().to_ascii_lowercase().as_str() {
            "none" => {
                if tokens.len() != 1 {
                    return Err(PopoverPlacementDiagnostic {
                        field: PopoverPlacementField::Constraint,
                        value: value.to_string(),
                        kind: PopoverPlacementDiagnosticKind::ConflictingTokens,
                    });
                }
                return Ok(PopoverConstraintAdjustment {
                    flip_x: false,
                    flip_y: false,
                    slide_x: false,
                    slide_y: false,
                    resize_x: false,
                    resize_y: false,
                });
            }
            "flip" => {
                adjust.flip_x = true;
                adjust.flip_y = true;
            }
            "flip-x" | "flip_x" => adjust.flip_x = true,
            "flip-y" | "flip_y" => adjust.flip_y = true,
            "slide" => {
                adjust.slide_x = true;
                adjust.slide_y = true;
            }
            "slide-x" | "slide_x" => adjust.slide_x = true,
            "slide-y" | "slide_y" => adjust.slide_y = true,
            "resize" => {
                adjust.resize_x = true;
                adjust.resize_y = true;
            }
            "resize-x" | "resize_x" => adjust.resize_x = true,
            "resize-y" | "resize_y" => adjust.resize_y = true,
            _ => {
                return Err(PopoverPlacementDiagnostic {
                    field: PopoverPlacementField::Constraint,
                    value: (*token).to_string(),
                    kind: PopoverPlacementDiagnosticKind::UnknownToken,
                });
            }
        }
    }
    Ok(adjust)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node_with(attrs: &[(&str, &str)]) -> WidgetNode {
        let mut node = WidgetNode::new("popover");
        for (k, v) in attrs {
            node.attributes.insert((*k).into(), (*v).to_string());
        }
        node
    }

    #[test]
    fn defaults_are_menu_friendly() {
        let placement = PopoverPlacement::from_node(&node_with(&[])).unwrap();
        assert_eq!(placement.anchor, PopoverAnchor::Bottom);
        assert_eq!(placement.gravity, PopoverGravity::Bottom);
        assert_eq!(placement.grab, PopoverGrab::Hover);
        assert!(placement.constraint_adjustment.flip_y);
        assert!(placement.constraint_adjustment.slide_x);
        assert_eq!(placement.offset_x, 0);
        assert_eq!(placement.offset_y, 0);
    }

    #[test]
    fn parses_explicit_placement() {
        let placement = PopoverPlacement::from_node(&node_with(&[
            ("anchor", "top"),
            ("gravity", "top"),
            ("offset-x", "-12"),
            ("offset-y", "4"),
            ("grab", "click"),
        ]))
        .unwrap();
        assert_eq!(placement.anchor, PopoverAnchor::Top);
        assert_eq!(placement.gravity, PopoverGravity::Top);
        assert_eq!(placement.offset_x, -12);
        assert_eq!(placement.offset_y, 4);
        assert_eq!(placement.grab, PopoverGrab::Click);
    }

    #[test]
    fn gravity_defaults_to_anchor_direction() {
        let placement = PopoverPlacement::from_node(&node_with(&[("anchor", "right")])).unwrap();
        assert_eq!(placement.anchor, PopoverAnchor::Right);
        assert_eq!(placement.gravity, PopoverGravity::Right);
    }

    #[test]
    fn constraint_list_parses_axes() {
        let placement =
            PopoverPlacement::from_node(&node_with(&[("constrain", "flip-y slide-x")])).unwrap();
        let c = placement.constraint_adjustment;
        assert!(c.flip_y && c.slide_x);
        assert!(!c.flip_x && !c.slide_y && !c.resize_x && !c.resize_y);
    }

    #[test]
    fn constraint_none_disables_all() {
        let placement = PopoverPlacement::from_node(&node_with(&[("constrain", "none")])).unwrap();
        let c = placement.constraint_adjustment;
        assert!(!c.flip_x && !c.flip_y && !c.slide_x && !c.slide_y);
    }

    #[test]
    fn unknown_values_are_typed_diagnostics_instead_of_fallbacks() {
        let diagnostics = PopoverPlacement::from_node(&node_with(&[
            ("anchor", "sideways"),
            ("grab", "telepathy"),
        ]))
        .unwrap_err();
        assert_eq!(diagnostics.len(), 2);
        assert_eq!(diagnostics[0].field, PopoverPlacementField::Anchor);
        assert_eq!(
            diagnostics[0].kind,
            PopoverPlacementDiagnosticKind::UnknownToken
        );
        assert_eq!(diagnostics[1].field, PopoverPlacementField::Grab);
    }

    #[test]
    fn invalid_offsets_and_constraint_combinations_are_rejected() {
        let diagnostics = PopoverPlacement::from_node(&node_with(&[
            ("offset-x", "1.5"),
            ("constrain", "none slide-y"),
        ]))
        .unwrap_err();
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.field == PopoverPlacementField::OffsetX
                && diagnostic.kind == PopoverPlacementDiagnosticKind::InvalidInteger
        }));
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.field == PopoverPlacementField::Constraint
                && diagnostic.kind == PopoverPlacementDiagnosticKind::ConflictingTokens
        }));
    }
}
