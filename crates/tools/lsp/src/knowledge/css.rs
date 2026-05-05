pub struct CssProp {
    pub name: &'static str,
    pub description: &'static str,
    pub values: &'static [&'static str],
}

/// The complete set of CSS properties supported by `mesh-core-elements`'s `apply_declaration`.
/// Do NOT suggest properties not in this list — unsupported properties produce style diagnostics
/// and would mislead component authors.
pub static CSS_PROPERTIES: &[CssProp] = &[
    // Color / typography
    CssProp {
        name: "background",
        description: "Background color.",
        values: &["transparent", "token()", "var()"],
    },
    CssProp {
        name: "background-color",
        description: "Background color (alias for background).",
        values: &["transparent", "token()", "var()"],
    },
    CssProp {
        name: "color",
        description: "Text color.",
        values: &["transparent", "token()", "var()"],
    },
    CssProp {
        name: "border-color",
        description: "Border color.",
        values: &["transparent", "token()", "var()"],
    },
    CssProp {
        name: "border",
        description: "Practical border shorthand: width, style keyword, color.",
        values: &["none", "1px solid token()"],
    },
    CssProp {
        name: "font",
        description: "Practical font shorthand.",
        values: &["italic 600 16px/1.4 Inter"],
    },
    CssProp {
        name: "font-size",
        description: "Font size in px or token reference.",
        values: &["token()"],
    },
    CssProp {
        name: "font-weight",
        description: "Font weight (400, 500, 700, 900).",
        values: &["400", "500", "700", "900"],
    },
    CssProp {
        name: "font-family",
        description: "Font family name.",
        values: &[],
    },
    CssProp {
        name: "font-style",
        description: "Font style.",
        values: &["normal", "italic"],
    },
    CssProp {
        name: "letter-spacing",
        description: "Letter spacing in px.",
        values: &[],
    },
    CssProp {
        name: "text-overflow",
        description: "Text truncation behavior.",
        values: &["clip", "ellipsis"],
    },
    CssProp {
        name: "line-height",
        description: "Line height multiplier or px.",
        values: &[],
    },
    CssProp {
        name: "text-align",
        description: "Text alignment.",
        values: &["left", "center", "right"],
    },
    // Box model
    CssProp {
        name: "padding",
        description: "Padding on all sides.",
        values: &[],
    },
    CssProp {
        name: "padding-top",
        description: "Top padding.",
        values: &[],
    },
    CssProp {
        name: "padding-right",
        description: "Right padding.",
        values: &[],
    },
    CssProp {
        name: "padding-bottom",
        description: "Bottom padding.",
        values: &[],
    },
    CssProp {
        name: "padding-left",
        description: "Left padding.",
        values: &[],
    },
    CssProp {
        name: "padding-inline",
        description: "Left + right padding (horizontal).",
        values: &[],
    },
    CssProp {
        name: "padding-block",
        description: "Top + bottom padding (vertical).",
        values: &[],
    },
    CssProp {
        name: "padding-x",
        description: "Left + right padding (horizontal alias).",
        values: &[],
    },
    CssProp {
        name: "padding-y",
        description: "Top + bottom padding (vertical alias).",
        values: &[],
    },
    CssProp {
        name: "margin",
        description: "Margin on all sides.",
        values: &[],
    },
    CssProp {
        name: "margin-top",
        description: "Top margin.",
        values: &[],
    },
    CssProp {
        name: "margin-right",
        description: "Right margin.",
        values: &[],
    },
    CssProp {
        name: "margin-bottom",
        description: "Bottom margin.",
        values: &[],
    },
    CssProp {
        name: "margin-left",
        description: "Left margin.",
        values: &[],
    },
    CssProp {
        name: "margin-inline",
        description: "Left + right margin (horizontal).",
        values: &[],
    },
    CssProp {
        name: "margin-block",
        description: "Top + bottom margin (vertical).",
        values: &[],
    },
    CssProp {
        name: "margin-x",
        description: "Left + right margin (horizontal alias).",
        values: &[],
    },
    CssProp {
        name: "margin-y",
        description: "Top + bottom margin (vertical alias).",
        values: &[],
    },
    CssProp {
        name: "width",
        description: "Element width (px, %, auto).",
        values: &["auto", "100%"],
    },
    CssProp {
        name: "height",
        description: "Element height (px, %, auto).",
        values: &["auto", "100%"],
    },
    CssProp {
        name: "min-width",
        description: "Minimum width.",
        values: &[],
    },
    CssProp {
        name: "max-width",
        description: "Maximum width.",
        values: &[],
    },
    CssProp {
        name: "min-height",
        description: "Minimum height.",
        values: &[],
    },
    CssProp {
        name: "max-height",
        description: "Maximum height.",
        values: &[],
    },
    // Border / radius
    CssProp {
        name: "border-radius",
        description: "Corner radius for all corners.",
        values: &[],
    },
    CssProp {
        name: "border-top-left-radius",
        description: "Top-left corner radius.",
        values: &[],
    },
    CssProp {
        name: "border-top-right-radius",
        description: "Top-right corner radius.",
        values: &[],
    },
    CssProp {
        name: "border-bottom-right-radius",
        description: "Bottom-right corner radius.",
        values: &[],
    },
    CssProp {
        name: "border-bottom-left-radius",
        description: "Bottom-left corner radius.",
        values: &[],
    },
    CssProp {
        name: "border-width",
        description: "Border width on all sides.",
        values: &[],
    },
    CssProp {
        name: "border-top-width",
        description: "Top border width.",
        values: &[],
    },
    CssProp {
        name: "border-right-width",
        description: "Right border width.",
        values: &[],
    },
    CssProp {
        name: "border-bottom-width",
        description: "Bottom border width.",
        values: &[],
    },
    CssProp {
        name: "border-left-width",
        description: "Left border width.",
        values: &[],
    },
    // Opacity
    CssProp {
        name: "opacity",
        description: "Opacity (0.0 = invisible, 1.0 = fully visible).",
        values: &["0", "0.5", "1"],
    },
    // Flexbox layout
    CssProp {
        name: "display",
        description: "Display mode.",
        values: &["flex", "none"],
    },
    CssProp {
        name: "visibility",
        description: "Visibility-like behavior; hidden/collapse map to opacity 0.",
        values: &["visible", "hidden", "collapse"],
    },
    CssProp {
        name: "direction",
        description: "Text direction.",
        values: &["ltr", "rtl"],
    },
    CssProp {
        name: "flex-direction",
        description: "Main axis direction.",
        values: &["row", "column", "row-reverse", "column-reverse"],
    },
    CssProp {
        name: "flex-wrap",
        description: "Whether items wrap to the next line.",
        values: &["nowrap", "wrap", "wrap-reverse"],
    },
    CssProp {
        name: "flex-grow",
        description: "Flex grow factor.",
        values: &["0", "1"],
    },
    CssProp {
        name: "flex-shrink",
        description: "Flex shrink factor.",
        values: &["0", "1"],
    },
    CssProp {
        name: "flex-basis",
        description: "Initial main size before flex calculation.",
        values: &["auto", "0"],
    },
    CssProp {
        name: "flex",
        description: "Shorthand: flex-grow, flex-shrink, flex-basis.",
        values: &["none", "auto", "1"],
    },
    CssProp {
        name: "justify-content",
        description: "Alignment along the main axis.",
        values: &[
            "start",
            "center",
            "end",
            "space-between",
            "space-around",
            "flex-start",
            "flex-end",
        ],
    },
    CssProp {
        name: "align-items",
        description: "Alignment along the cross axis.",
        values: &[
            "stretch",
            "center",
            "start",
            "end",
            "flex-start",
            "flex-end",
        ],
    },
    CssProp {
        name: "align-self",
        description: "Override align-items for a single child.",
        values: &["auto", "stretch", "center", "start", "end", "baseline"],
    },
    CssProp {
        name: "align-content",
        description: "Multi-line alignment.",
        values: &[
            "stretch",
            "center",
            "start",
            "end",
            "space-between",
            "space-around",
        ],
    },
    CssProp {
        name: "gap",
        description: "Gap between flex children.",
        values: &[],
    },
    CssProp {
        name: "column-gap",
        description: "Horizontal gap between flex children.",
        values: &[],
    },
    CssProp {
        name: "row-gap",
        description: "Gap alias for Phase 8 flex layout.",
        values: &[],
    },
    CssProp {
        name: "gap-x",
        description: "Gap alias for Phase 8 flex layout.",
        values: &[],
    },
    // Overflow
    CssProp {
        name: "overflow",
        description: "Clip or scroll overflowing content.",
        values: &["visible", "hidden", "auto", "scroll"],
    },
    CssProp {
        name: "overflow-x",
        description: "Horizontal overflow behavior.",
        values: &["visible", "hidden", "auto", "scroll"],
    },
    CssProp {
        name: "overflow-y",
        description: "Vertical overflow behavior.",
        values: &["visible", "hidden", "auto", "scroll"],
    },
    // Positioning
    CssProp {
        name: "position",
        description: "Positioning scheme.",
        values: &["static", "relative", "absolute"],
    },
    CssProp {
        name: "z-index",
        description: "Stacking order for overlapping elements.",
        values: &["0", "1", "10", "100"],
    },
    CssProp {
        name: "top",
        description: "Inset from top edge (for position: absolute).",
        values: &[],
    },
    CssProp {
        name: "right",
        description: "Inset from right edge (for position: absolute).",
        values: &[],
    },
    CssProp {
        name: "bottom",
        description: "Inset from bottom edge (for position: absolute).",
        values: &[],
    },
    CssProp {
        name: "left",
        description: "Inset from left edge (for position: absolute).",
        values: &[],
    },
    CssProp {
        name: "inset",
        description: "Inset on all sides (for position: absolute).",
        values: &[],
    },
    // Transition
    CssProp {
        name: "transition",
        description: "Shorthand for all transition properties.",
        values: &[],
    },
    CssProp {
        name: "transition-duration",
        description: "Duration of the transition (e.g. 200ms).",
        values: &["0ms", "100ms", "200ms", "300ms"],
    },
    CssProp {
        name: "transition-delay",
        description: "Delay before the transition starts.",
        values: &["0ms"],
    },
    CssProp {
        name: "transition-timing-function",
        description: "Easing curve for the transition.",
        values: &["linear", "ease", "ease-in", "ease-out", "ease-in-out"],
    },
    CssProp {
        name: "transition-property",
        description: "Which CSS properties transition.",
        values: &[
            "all",
            "opacity",
            "background-color",
            "color",
            "border-color",
            "border-radius",
        ],
    },
    // Animation metadata; Phase 12 owns scheduling and keyframes.
    CssProp {
        name: "animation",
        description: "Practical animation shorthand stored as metadata only.",
        values: &[],
    },
    CssProp {
        name: "animation-name",
        description: "Animation name metadata.",
        values: &["none"],
    },
    CssProp {
        name: "animation-duration",
        description: "Animation duration metadata.",
        values: &["0ms", "150ms", "300ms"],
    },
    CssProp {
        name: "animation-delay",
        description: "Animation delay metadata.",
        values: &["0ms"],
    },
    CssProp {
        name: "animation-timing-function",
        description: "Animation easing metadata.",
        values: &["linear", "ease", "ease-in", "ease-out", "ease-in-out"],
    },
    CssProp {
        name: "animation-iteration-count",
        description: "Animation iteration metadata.",
        values: &["1", "2", "infinite"],
    },
    CssProp {
        name: "animation-direction",
        description: "Animation direction metadata.",
        values: &["normal", "reverse", "alternate", "alternate-reverse"],
    },
    CssProp {
        name: "animation-fill-mode",
        description: "Animation fill mode metadata.",
        values: &["none", "forwards", "backwards", "both"],
    },
    CssProp {
        name: "animation-play-state",
        description: "Animation play state metadata.",
        values: &["running", "paused"],
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    fn property_names() -> Vec<&'static str> {
        CSS_PROPERTIES
            .iter()
            .map(|property| property.name)
            .collect()
    }

    #[test]
    fn css_completion_includes_phase_8_shorthands() {
        let names = property_names();
        for property in [
            "border", "padding", "margin", "font", "flex", "overflow", "inset",
        ] {
            assert!(names.contains(&property), "{property}");
        }
    }

    #[test]
    fn css_completion_includes_animation_declarations() {
        let names = property_names();
        for property in [
            "animation",
            "animation-name",
            "animation-duration",
            "animation-timing-function",
            "animation-play-state",
        ] {
            assert!(names.contains(&property), "{property}");
        }
    }

    #[test]
    fn css_completion_does_not_include_grid_or_transform() {
        let names = property_names();
        assert!(!names.contains(&"display: grid"));
        assert!(!names.contains(&"grid-template-columns"));
        assert!(!names.contains(&"transform"));
    }
}
