pub struct CssProp {
    pub name: &'static str,
    pub description: &'static str,
    pub values: &'static [&'static str],
}

/// The complete set of CSS properties supported by `mesh-elements`'s `apply_declaration`.
/// Do NOT suggest properties not in this list — unsupported properties are silently ignored
/// by the renderer and would mislead component authors.
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
            "border-radius",
        ],
    },
];
