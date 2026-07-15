#[derive(Debug, Clone, Copy)]
pub enum TagCategory {
    Layout,
    Content,
    Controls,
    Structure,
    Composition,
}

impl std::fmt::Display for TagCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Layout => write!(f, "layout"),
            Self::Content => write!(f, "content"),
            Self::Controls => write!(f, "controls"),
            Self::Structure => write!(f, "structure"),
            Self::Composition => write!(f, "composition"),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct AttrDef {
    pub name: &'static str,
    pub description: &'static str,
    /// Known value set for enum-like attributes, used to drive attribute-value
    /// completion. Empty means the value is free-form (no completion offered).
    pub values: &'static [&'static str],
}

#[derive(Debug, Clone, Copy)]
pub struct ElementBase {
    pub name: &'static str,
    pub description: &'static str,
    pub attributes: &'static [AttrDef],
}

#[derive(Debug, Clone, Copy)]
pub struct TagDef {
    pub name: &'static str,
    pub category: TagCategory,
    pub description: &'static str,
    pub bases: &'static [&'static ElementBase],
    pub attributes: &'static [AttrDef],
    /// Whether the tag is self-closing (no children).
    pub self_closing: bool,
}

impl TagDef {
    pub fn all_attributes(&self) -> Vec<&'static AttrDef> {
        let mut attrs = Vec::new();
        for base in self.bases {
            for attr in base.attributes {
                push_attr(&mut attrs, attr);
            }
        }
        for attr in self.attributes {
            push_attr(&mut attrs, attr);
        }
        attrs
    }

    pub fn inherited_base_names(&self) -> Vec<&'static str> {
        self.bases.iter().map(|base| base.name).collect()
    }
}

fn push_attr(attrs: &mut Vec<&'static AttrDef>, attr: &'static AttrDef) {
    if attrs.iter().any(|existing| existing.name == attr.name) {
        return;
    }
    attrs.push(attr);
}

/// Shared value set for boolean attributes (`disabled`, `checked`, ...).
const BOOL_VALUES: &[&str] = &["true", "false"];

const MESH_ELEMENT_ATTRS: &[AttrDef] = &[
    AttrDef {
        name: "class",
        description: "CSS class name(s)",
        values: &[],
    },
    AttrDef {
        name: "id",
        description: "Element identifier for CSS targeting",
        values: &[],
    },
    AttrDef {
        name: "ref",
        description: "Stable element reference name for runtime metrics under refs.<name>",
        values: &[],
    },
    AttrDef {
        name: "style",
        description: "Inline CSS styles",
        values: &[],
    },
    AttrDef {
        name: "aria-label",
        description: "Accessible label",
        values: &[],
    },
    AttrDef {
        name: "aria-role",
        description: "WAI-ARIA role override",
        values: &[],
    },
    AttrDef {
        name: "title",
        description: "Tooltip / accessible title",
        values: &[],
    },
    AttrDef {
        name: "aria-hidden",
        description: "Hide from accessibility tree",
        values: BOOL_VALUES,
    },
];

const INTERACTIVE_ELEMENT_ATTRS: &[AttrDef] = &[
    AttrDef {
        name: "onclick",
        description: "Click / tap handler function",
        values: &[],
    },
    AttrDef {
        name: "onactivate",
        description: "Keyboard or command activation handler function",
        values: &[],
    },
    AttrDef {
        name: "onchange",
        description: "Value change handler function",
        values: &[],
    },
    AttrDef {
        name: "oninput",
        description: "User input handler function",
        values: &[],
    },
    AttrDef {
        name: "onfocus",
        description: "Focus received handler function",
        values: &[],
    },
    AttrDef {
        name: "onblur",
        description: "Focus lost handler function",
        values: &[],
    },
    AttrDef {
        name: "onkeydown",
        description: "Key press handler function",
        values: &[],
    },
    AttrDef {
        name: "ontwofingerscroll",
        description: "Continuous two-finger trackpad scroll handler; event.delta contains x/y motion and event.pointer contains the surface position",
        values: &[],
    },
    AttrDef {
        name: "onswipe",
        description: "Trackpad swipe handler; event.phase is start/move/end with fingers, delta, total_delta, and terminal direction/velocity/duration/cancelled fields",
        values: &[],
    },
    AttrDef {
        name: "onpinch",
        description: "Trackpad pinch handler; event.phase is start/move/end with fingers, scale, rotation, delta, total_delta, and cancelled fields",
        values: &[],
    },
    AttrDef {
        name: "onhold",
        description: "Trackpad hold handler; event.phase is start/end with fingers, duration, and cancelled fields",
        values: &[],
    },
    AttrDef {
        name: "ontouchstart",
        description: "Raw touchscreen contact start handler; event.touch is the changed point and event.touches lists active contacts",
        values: &[],
    },
    AttrDef {
        name: "ontouchmove",
        description: "Raw touchscreen contact motion handler; event.touch is the changed point and event.touches lists active contacts",
        values: &[],
    },
    AttrDef {
        name: "ontouchend",
        description: "Raw touchscreen contact end handler; event.changed_touches contains the released point and event.touches lists remaining contacts",
        values: &[],
    },
    AttrDef {
        name: "ontouchcancel",
        description: "Raw touchscreen cancellation handler; event.cancelled is true and event.changed_touches lists the cancelled contacts",
        values: &[],
    },
    AttrDef {
        name: "ontap",
        description: "Single-touch tap handler; event.touch, duration, tap_count, pointer, and current_target describe the synthesized activation",
        values: &[],
    },
    AttrDef {
        name: "ondoubletap",
        description: "Second nearby tap handler; event.tap_count is 2 and event.touch/current_target identify the captured target",
        values: &[],
    },
    AttrDef {
        name: "onlongpress",
        description: "Single-touch press held for 500 ms without moving beyond 12 px; event.duration and event.touch describe the press",
        values: &[],
    },
];

const VALUE_ELEMENT_ATTRS: &[AttrDef] = &[
    AttrDef {
        name: "value",
        description: "Current control value",
        values: &[],
    },
    AttrDef {
        name: "disabled",
        description: "Disables user interaction",
        values: BOOL_VALUES,
    },
];

const TEXT_INPUT_ELEMENT_ATTRS: &[AttrDef] = &[
    AttrDef {
        name: "placeholder",
        description: "Placeholder text shown while empty",
        values: &[],
    },
    AttrDef {
        name: "readonly",
        description: "Makes the input read-only",
        values: BOOL_VALUES,
    },
    AttrDef {
        name: "type",
        description: "Input type: text | password | number | email | url | search",
        values: &["text", "password", "number", "email", "url", "search"],
    },
];

const RANGE_ELEMENT_ATTRS: &[AttrDef] = &[
    AttrDef {
        name: "min",
        description: "Minimum value",
        values: &[],
    },
    AttrDef {
        name: "max",
        description: "Maximum value",
        values: &[],
    },
    AttrDef {
        name: "step",
        description: "Step size",
        values: &[],
    },
];

pub static MESH_ELEMENT_BASE: ElementBase = ElementBase {
    name: "MeshElement",
    description: "Base element data shared by every tag. Refs for any element expose runtime layout metrics such as width, height, left, top, right, bottom, client_width, client_height, client_bound_rect, clientBoundRect, and bounding_client_rect.",
    attributes: MESH_ELEMENT_ATTRS,
};

pub static INTERACTIVE_ELEMENT_BASE: ElementBase = ElementBase {
    name: "InteractiveElement",
    description: "Adds common interaction handlers available on clickable or focusable elements.",
    attributes: INTERACTIVE_ELEMENT_ATTRS,
};

pub static VALUE_ELEMENT_BASE: ElementBase = ElementBase {
    name: "ValueElement",
    description: "Adds a current value and disabled state for controls that hold user-editable data.",
    attributes: VALUE_ELEMENT_ATTRS,
};

pub static INPUT_ELEMENT_BASE: ElementBase = ElementBase {
    name: "InputElement",
    description: "Builds on ValueElement with text-entry behavior such as placeholder and readonly state.",
    attributes: TEXT_INPUT_ELEMENT_ATTRS,
};

pub static RANGE_ELEMENT_BASE: ElementBase = ElementBase {
    name: "RangeElement",
    description: "Adds numeric range bounds shared by slider-like controls.",
    attributes: RANGE_ELEMENT_ATTRS,
};

pub static UNIVERSAL_ATTRS: &[AttrDef] = MESH_ELEMENT_ATTRS;
pub static EVENT_ATTRS: &[AttrDef] = INTERACTIVE_ELEMENT_ATTRS;

static NO_ATTRS: &[AttrDef] = &[];

static TEXT_ATTRS: &[AttrDef] = &[AttrDef {
    name: "selectable",
    description: "Whether text can be selected by the user",
    values: &[],
}];

static LABEL_ATTRS: &[AttrDef] = &[AttrDef {
    name: "for",
    description: "ID of the associated input element",
    values: &[],
}];

static ICON_ATTRS: &[AttrDef] = &[
    AttrDef {
        name: "name",
        description: "XDG icon name (e.g. audio-volume-high)",
        values: &[],
    },
    AttrDef {
        name: "src",
        description: "Absolute or module-relative path to an icon file",
        values: &[],
    },
    AttrDef {
        name: "size",
        description: "Icon size hint in pixels for XDG resolution",
        values: &[],
    },
    AttrDef {
        name: "alt",
        description: "Accessible alternative text",
        values: &[],
    },
];

static IMAGE_ATTRS: &[AttrDef] = &[
    AttrDef {
        name: "src",
        description: "Image source path",
        values: &[],
    },
    AttrDef {
        name: "alt",
        description: "Accessible alternative text",
        values: &[],
    },
];

static LAYOUT_ATTRS: &[AttrDef] = &[
    AttrDef {
        name: "align",
        description: "Cross-axis alignment",
        values: &["start", "end", "center", "stretch"],
    },
    AttrDef {
        name: "justify",
        description: "Main-axis justification",
        values: &["start", "end", "center", "space-between", "space-around"],
    },
    AttrDef {
        name: "spacing",
        description: "Spacing between children",
        values: &[],
    },
    AttrDef {
        name: "gap",
        description: "Gap between children",
        values: &[],
    },
    AttrDef {
        name: "overflow",
        description: "Overflow behavior",
        values: &["visible", "hidden", "auto", "scroll"],
    },
    AttrDef {
        name: "overflow-x",
        description: "Horizontal overflow behavior",
        values: &["visible", "hidden", "auto", "scroll"],
    },
    AttrDef {
        name: "overflow-y",
        description: "Vertical overflow behavior",
        values: &["visible", "hidden", "auto", "scroll"],
    },
];

static GRID_ATTRS: &[AttrDef] = &[
    AttrDef {
        name: "columns",
        description: "Grid columns as fixed px or auto tracks",
        values: &[],
    },
    AttrDef {
        name: "rows",
        description: "Grid rows as fixed px or auto tracks",
        values: &[],
    },
    AttrDef {
        name: "column",
        description: "Grid column placement",
        values: &[],
    },
    AttrDef {
        name: "row",
        description: "Grid row placement",
        values: &[],
    },
    AttrDef {
        name: "column-span",
        description: "Grid column span",
        values: &[],
    },
    AttrDef {
        name: "row-span",
        description: "Grid row span",
        values: &[],
    },
];

static STACK_ATTRS: &[AttrDef] = &[AttrDef {
    name: "layer",
    description: "Layer order for overlapping children",
    values: &[],
}];

static DISPLAY_ATTRS: &[AttrDef] = &[
    AttrDef {
        name: "label",
        description: "Accessible label",
        values: &[],
    },
    AttrDef {
        name: "tooltip",
        description: "Accessible tooltip text",
        values: &[],
    },
];

static SHORTCUT_ATTRS: &[AttrDef] = &[AttrDef {
    name: "key",
    description: "Shortcut key label",
    values: &[],
}];

static TOOLTIP_ATTRS: &[AttrDef] = &[AttrDef {
    name: "tooltip-for",
    description: "ID of the element that owns this tooltip",
    values: &[],
}];

static PROGRESS_ATTRS: &[AttrDef] = &[
    AttrDef {
        name: "value",
        description: "Current progress value",
        values: &[],
    },
    AttrDef {
        name: "min",
        description: "Minimum progress value",
        values: &[],
    },
    AttrDef {
        name: "max",
        description: "Maximum progress value",
        values: &[],
    },
    AttrDef {
        name: "indeterminate",
        description: "Progress has no determinate value",
        values: BOOL_VALUES,
    },
];

static BUTTON_ATTRS: &[AttrDef] = &[
    AttrDef {
        name: "variant",
        description: "Visual variant: filled | outlined | text | tonal",
        values: &["filled", "outlined", "text", "tonal"],
    },
    AttrDef {
        name: "pressed",
        description: "Whether the button is in a pressed/toggled state",
        values: BOOL_VALUES,
    },
    AttrDef {
        name: "busy",
        description: "Whether the action is currently busy",
        values: BOOL_VALUES,
    },
    AttrDef {
        name: "default",
        description: "Marks the default action in a group",
        values: BOOL_VALUES,
    },
    AttrDef {
        name: "destructive",
        description: "Marks a destructive action",
        values: BOOL_VALUES,
    },
    AttrDef {
        name: "keybind",
        description: "Associated keybind id or display shortcut",
        values: &[],
    },
    AttrDef {
        name: "command",
        description: "Command intent metadata",
        values: &[],
    },
    AttrDef {
        name: "href",
        description: "Link intent metadata; navigation is handled by Luau",
        values: &[],
    },
];

static TEXTAREA_ATTRS: &[AttrDef] = &[AttrDef {
    name: "multiline",
    description: "Whether this configured input accepts multiple lines",
    values: BOOL_VALUES,
}];

static PASSWORD_ATTRS: &[AttrDef] = &[AttrDef {
    name: "masked",
    description: "Whether this configured input masks displayed text",
    values: BOOL_VALUES,
}];

static NUMERIC_INPUT_ATTRS: &[AttrDef] = &[
    AttrDef {
        name: "min",
        description: "Minimum numeric value",
        values: &[],
    },
    AttrDef {
        name: "max",
        description: "Maximum numeric value",
        values: &[],
    },
    AttrDef {
        name: "step",
        description: "Positive numeric step size",
        values: &[],
    },
];

static SWITCH_ATTRS: &[AttrDef] = &[AttrDef {
    name: "checked",
    description: "Whether the switch is on",
    values: BOOL_VALUES,
}];

static CHECKBOX_ATTRS: &[AttrDef] = &[
    AttrDef {
        name: "checked",
        description: "Whether the checkbox is checked",
        values: BOOL_VALUES,
    },
    AttrDef {
        name: "label",
        description: "Associated label text",
        values: &[],
    },
];

static SELECT_ATTRS: &[AttrDef] = &[
    AttrDef {
        name: "value",
        description: "Selected option value",
        values: &[],
    },
    AttrDef {
        name: "disabled",
        description: "Disables selection changes",
        values: BOOL_VALUES,
    },
    AttrDef {
        name: "required",
        description: "Requires a selected value",
        values: BOOL_VALUES,
    },
];

static OPTION_ATTRS: &[AttrDef] = &[
    AttrDef {
        name: "value",
        description: "Value sent to the parent select onchange handler",
        values: &[],
    },
    AttrDef {
        name: "selected",
        description: "Marks this option as selected",
        values: BOOL_VALUES,
    },
    AttrDef {
        name: "disabled",
        description: "Disables this option",
        values: BOOL_VALUES,
    },
];

static RADIO_ATTRS: &[AttrDef] = &[
    AttrDef {
        name: "value",
        description: "Value sent to the parent radio-group onchange handler",
        values: &[],
    },
    AttrDef {
        name: "name",
        description: "Radio group name when not nested in a radio-group",
        values: &[],
    },
    AttrDef {
        name: "checked",
        description: "Whether this radio choice is checked",
        values: BOOL_VALUES,
    },
    AttrDef {
        name: "disabled",
        description: "Disables this radio choice",
        values: BOOL_VALUES,
    },
];

static MENU_ATTRS: &[AttrDef] = &[
    AttrDef {
        name: "expanded",
        description: "Whether the menu is expanded",
        values: BOOL_VALUES,
    },
    AttrDef {
        name: "disabled",
        description: "Disables the menu",
        values: BOOL_VALUES,
    },
];

static MENU_ITEM_ATTRS: &[AttrDef] = &[
    AttrDef {
        name: "disabled",
        description: "Disables item activation",
        values: BOOL_VALUES,
    },
    AttrDef {
        name: "selected",
        description: "Whether this item is selected",
        values: BOOL_VALUES,
    },
    AttrDef {
        name: "keybind",
        description: "Associated keybind id or display shortcut",
        values: &[],
    },
    AttrDef {
        name: "command",
        description: "Command intent metadata",
        values: &[],
    },
];

static CONTAINER_ATTRS: &[AttrDef] = &[
    AttrDef {
        name: "open",
        description: "Whether the container is open",
        values: BOOL_VALUES,
    },
    AttrDef {
        name: "expanded",
        description: "Whether the container is expanded",
        values: BOOL_VALUES,
    },
    AttrDef {
        name: "label",
        description: "Accessible label",
        values: &[],
    },
];

static TAB_ATTRS: &[AttrDef] = &[
    AttrDef {
        name: "selected",
        description: "Whether this tab is selected",
        values: BOOL_VALUES,
    },
    AttrDef {
        name: "disabled",
        description: "Disables tab activation",
        values: BOOL_VALUES,
    },
];

static COLLECTION_ATTRS: &[AttrDef] = &[
    AttrDef {
        name: "selected",
        description: "Whether this item is selected",
        values: BOOL_VALUES,
    },
    AttrDef {
        name: "disabled",
        description: "Disables item activation",
        values: BOOL_VALUES,
    },
    AttrDef {
        name: "active",
        description: "Marks the active row or item",
        values: BOOL_VALUES,
    },
];

static SLOT_ATTRS: &[AttrDef] = &[AttrDef {
    name: "name",
    description: "Slot name (matches the parent component's slot)",
    values: &[],
}];

/// Shared value set for `<popover>` anchor/gravity edge attributes, mirroring
/// `PopoverAnchor`/`PopoverGravity` in `mesh-core-elements/src/popover.rs`.
const POPOVER_EDGE_VALUES: &[&str] = &[
    "center",
    "top",
    "bottom",
    "left",
    "right",
    "top-left",
    "top-right",
    "bottom-left",
    "bottom-right",
];

static POPOVER_ATTRS: &[AttrDef] = &[
    AttrDef {
        name: "open",
        description: "Whether the popover is promoted/visible",
        values: BOOL_VALUES,
    },
    AttrDef {
        name: "anchor-ref",
        description: "Element `ref` name of the trigger the popover is positioned against",
        values: &[],
    },
    AttrDef {
        name: "anchor",
        description: "Edge/corner of the anchor rectangle the popup is positioned against",
        values: POPOVER_EDGE_VALUES,
    },
    AttrDef {
        name: "gravity",
        description: "Direction the popup grows away from the anchor point; defaults to match anchor",
        values: POPOVER_EDGE_VALUES,
    },
    AttrDef {
        name: "offset-x",
        description: "Extra horizontal offset applied to the computed position, in pixels",
        values: &[],
    },
    AttrDef {
        name: "offset-y",
        description: "Extra vertical offset applied to the computed position, in pixels",
        values: &[],
    },
    AttrDef {
        name: "grab",
        description: "Input-grab policy: hover (no compositor grab) or click (takes the click-serial grab)",
        values: &["hover", "click"],
    },
    AttrDef {
        name: "constrain",
        description: "Space/comma separated constraint-adjustment tokens: flip | flip-x | flip-y | slide | slide-x | slide-y | resize | resize-x | resize-y | none",
        values: &[
            "flip", "flip-x", "flip-y", "slide", "slide-x", "slide-y", "resize", "resize-x",
            "resize-y", "none",
        ],
    },
];

const BASE_MESH: &[&ElementBase] = &[&MESH_ELEMENT_BASE];
const BASE_MESH_INTERACTIVE: &[&ElementBase] = &[&MESH_ELEMENT_BASE, &INTERACTIVE_ELEMENT_BASE];
const BASE_MESH_VALUE: &[&ElementBase] = &[&MESH_ELEMENT_BASE, &VALUE_ELEMENT_BASE];
const BASE_MESH_INPUT: &[&ElementBase] = &[
    &MESH_ELEMENT_BASE,
    &INTERACTIVE_ELEMENT_BASE,
    &VALUE_ELEMENT_BASE,
    &INPUT_ELEMENT_BASE,
];
const BASE_MESH_RANGE: &[&ElementBase] = &[
    &MESH_ELEMENT_BASE,
    &INTERACTIVE_ELEMENT_BASE,
    &VALUE_ELEMENT_BASE,
    &RANGE_ELEMENT_BASE,
];

pub static TAG_DEFS: &[TagDef] = &[
    TagDef {
        name: "panel",
        category: TagCategory::Layout,
        self_closing: false,
        description: "Generic surface/container root.",
        bases: BASE_MESH,
        attributes: NO_ATTRS,
    },
    TagDef {
        name: "box",
        category: TagCategory::Layout,
        self_closing: false,
        description: "Generic container.",
        bases: BASE_MESH,
        attributes: NO_ATTRS,
    },
    TagDef {
        name: "row",
        category: TagCategory::Layout,
        self_closing: false,
        description: "Horizontal layout container.",
        bases: BASE_MESH,
        attributes: NO_ATTRS,
    },
    TagDef {
        name: "column",
        category: TagCategory::Layout,
        self_closing: false,
        description: "Vertical layout container.",
        bases: BASE_MESH,
        attributes: LAYOUT_ATTRS,
    },
    TagDef {
        name: "grid",
        category: TagCategory::Layout,
        self_closing: false,
        description: "Conservative grid container with fixed px or auto tracks.",
        bases: BASE_MESH,
        attributes: GRID_ATTRS,
    },
    TagDef {
        name: "stack",
        category: TagCategory::Layout,
        self_closing: false,
        description: "Layered container where children overlap.",
        bases: BASE_MESH,
        attributes: STACK_ATTRS,
    },
    TagDef {
        name: "scroll-view",
        category: TagCategory::Layout,
        self_closing: false,
        description: "Semantic scrollable region.",
        bases: BASE_MESH,
        attributes: LAYOUT_ATTRS,
    },
    TagDef {
        name: "scroll-area",
        category: TagCategory::Layout,
        self_closing: false,
        description: "Canonical semantic scrollable region.",
        bases: BASE_MESH,
        attributes: LAYOUT_ATTRS,
    },
    TagDef {
        name: "scroll",
        category: TagCategory::Layout,
        self_closing: false,
        description: "Scrollable region.",
        bases: BASE_MESH,
        attributes: NO_ATTRS,
    },
    TagDef {
        name: "spacer",
        category: TagCategory::Layout,
        self_closing: true,
        description: "Flexible empty space that expands to fill available room.",
        bases: BASE_MESH,
        attributes: NO_ATTRS,
    },
    TagDef {
        name: "divider",
        category: TagCategory::Layout,
        self_closing: true,
        description: "Horizontal or vertical visual divider line.",
        bases: BASE_MESH,
        attributes: NO_ATTRS,
    },
    TagDef {
        name: "separator",
        category: TagCategory::Layout,
        self_closing: true,
        description: "Horizontal or vertical visual separator line.",
        bases: BASE_MESH,
        attributes: NO_ATTRS,
    },
    TagDef {
        name: "section",
        category: TagCategory::Structure,
        self_closing: false,
        description: "Semantic section container.",
        bases: BASE_MESH,
        attributes: DISPLAY_ATTRS,
    },
    TagDef {
        name: "header",
        category: TagCategory::Structure,
        self_closing: false,
        description: "Semantic header container.",
        bases: BASE_MESH,
        attributes: DISPLAY_ATTRS,
    },
    TagDef {
        name: "footer",
        category: TagCategory::Structure,
        self_closing: false,
        description: "Semantic footer container.",
        bases: BASE_MESH,
        attributes: DISPLAY_ATTRS,
    },
    TagDef {
        name: "group",
        category: TagCategory::Structure,
        self_closing: false,
        description: "Semantic grouped controls or content.",
        bases: BASE_MESH,
        attributes: DISPLAY_ATTRS,
    },
    TagDef {
        name: "form-row",
        category: TagCategory::Structure,
        self_closing: false,
        description: "Semantic label/control row.",
        bases: BASE_MESH,
        attributes: DISPLAY_ATTRS,
    },
    TagDef {
        name: "text",
        category: TagCategory::Content,
        self_closing: false,
        description: "Text content.",
        bases: BASE_MESH,
        attributes: TEXT_ATTRS,
    },
    TagDef {
        name: "label",
        category: TagCategory::Content,
        self_closing: false,
        description: "Accessible label typically paired with an input.",
        bases: BASE_MESH,
        attributes: LABEL_ATTRS,
    },
    TagDef {
        name: "icon",
        category: TagCategory::Content,
        self_closing: true,
        description: "Icon from the XDG icon theme or a file path.",
        bases: BASE_MESH,
        attributes: ICON_ATTRS,
    },
    TagDef {
        name: "image",
        category: TagCategory::Content,
        self_closing: true,
        description: "Raster or vector image.",
        bases: BASE_MESH,
        attributes: IMAGE_ATTRS,
    },
    TagDef {
        name: "badge",
        category: TagCategory::Content,
        self_closing: false,
        description: "Compact status text.",
        bases: BASE_MESH,
        attributes: DISPLAY_ATTRS,
    },
    TagDef {
        name: "progress",
        category: TagCategory::Content,
        self_closing: true,
        description: "Progress indicator with determinate or indeterminate state.",
        bases: BASE_MESH,
        attributes: PROGRESS_ATTRS,
    },
    TagDef {
        name: "meter",
        category: TagCategory::Content,
        self_closing: true,
        description: "Reserved taxonomy entry; runtime behavior is deferred.",
        bases: BASE_MESH,
        attributes: NO_ATTRS,
    },
    TagDef {
        name: "tooltip",
        category: TagCategory::Content,
        self_closing: false,
        description: "Tooltip content associated with another element.",
        bases: BASE_MESH,
        attributes: TOOLTIP_ATTRS,
    },
    TagDef {
        name: "avatar",
        category: TagCategory::Content,
        self_closing: true,
        description: "Avatar image or icon.",
        bases: BASE_MESH,
        attributes: IMAGE_ATTRS,
    },
    TagDef {
        name: "shortcut",
        category: TagCategory::Content,
        self_closing: false,
        description: "Keyboard shortcut label.",
        bases: BASE_MESH,
        attributes: SHORTCUT_ATTRS,
    },
    TagDef {
        name: "button",
        category: TagCategory::Controls,
        self_closing: false,
        description: "Clickable action element.",
        bases: BASE_MESH_INTERACTIVE,
        attributes: BUTTON_ATTRS,
    },
    TagDef {
        name: "icon-button",
        category: TagCategory::Controls,
        self_closing: false,
        description: "Compatibility alias for configured <button>; put <icon> inside button markup.",
        bases: BASE_MESH_INTERACTIVE,
        attributes: BUTTON_ATTRS,
    },
    TagDef {
        name: "toggle-button",
        category: TagCategory::Controls,
        self_closing: false,
        description: "Compatibility alias for configured <button pressed=...>.",
        bases: BASE_MESH_INTERACTIVE,
        attributes: BUTTON_ATTRS,
    },
    TagDef {
        name: "command-button",
        category: TagCategory::Controls,
        self_closing: false,
        description: "Compatibility alias for configured <button command=...>.",
        bases: BASE_MESH_INTERACTIVE,
        attributes: BUTTON_ATTRS,
    },
    TagDef {
        name: "link-button",
        category: TagCategory::Controls,
        self_closing: false,
        description: "Compatibility alias for configured <button href=...>.",
        bases: BASE_MESH_INTERACTIVE,
        attributes: BUTTON_ATTRS,
    },
    TagDef {
        name: "input",
        category: TagCategory::Controls,
        self_closing: true,
        description: "Text input.",
        bases: BASE_MESH_INPUT,
        attributes: NO_ATTRS,
    },
    TagDef {
        name: "text-input",
        category: TagCategory::Controls,
        self_closing: true,
        description: "Semantic text input.",
        bases: BASE_MESH_INPUT,
        attributes: NO_ATTRS,
    },
    TagDef {
        name: "password-input",
        category: TagCategory::Controls,
        self_closing: true,
        description: "Semantic password input.",
        bases: BASE_MESH_INPUT,
        attributes: NO_ATTRS,
    },
    TagDef {
        name: "search-input",
        category: TagCategory::Controls,
        self_closing: true,
        description: "Semantic search input.",
        bases: BASE_MESH_INPUT,
        attributes: NO_ATTRS,
    },
    TagDef {
        name: "number-input",
        category: TagCategory::Controls,
        self_closing: true,
        description: "Semantic numeric input.",
        bases: BASE_MESH_INPUT,
        attributes: NUMERIC_INPUT_ATTRS,
    },
    TagDef {
        name: "textarea",
        category: TagCategory::Controls,
        self_closing: true,
        description: "Configured input with multiline source semantics.",
        bases: BASE_MESH_INPUT,
        attributes: TEXTAREA_ATTRS,
    },
    TagDef {
        name: "search",
        category: TagCategory::Controls,
        self_closing: true,
        description: "Configured input with search source semantics.",
        bases: BASE_MESH_INPUT,
        attributes: NO_ATTRS,
    },
    TagDef {
        name: "password",
        category: TagCategory::Controls,
        self_closing: true,
        description: "Configured input with masked text semantics.",
        bases: BASE_MESH_INPUT,
        attributes: PASSWORD_ATTRS,
    },
    TagDef {
        name: "stepper",
        category: TagCategory::Controls,
        self_closing: true,
        description: "Configured numeric input with stepper semantics.",
        bases: BASE_MESH_INPUT,
        attributes: NUMERIC_INPUT_ATTRS,
    },
    TagDef {
        name: "email-input",
        category: TagCategory::Controls,
        self_closing: true,
        description: "Semantic email input.",
        bases: BASE_MESH_INPUT,
        attributes: NO_ATTRS,
    },
    TagDef {
        name: "url-input",
        category: TagCategory::Controls,
        self_closing: true,
        description: "Semantic URL input.",
        bases: BASE_MESH_INPUT,
        attributes: NO_ATTRS,
    },
    TagDef {
        name: "slider",
        category: TagCategory::Controls,
        self_closing: true,
        description: "Range input control.",
        bases: BASE_MESH_RANGE,
        attributes: NO_ATTRS,
    },
    TagDef {
        name: "switch",
        category: TagCategory::Controls,
        self_closing: true,
        description: "Switch control.",
        bases: BASE_MESH_VALUE,
        attributes: SWITCH_ATTRS,
    },
    TagDef {
        name: "checkbox",
        category: TagCategory::Controls,
        self_closing: true,
        description: "Checkbox control.",
        bases: BASE_MESH_VALUE,
        attributes: CHECKBOX_ATTRS,
    },
    TagDef {
        name: "select",
        category: TagCategory::Controls,
        self_closing: false,
        description: "Native choice control with static child option elements.",
        bases: BASE_MESH_INTERACTIVE,
        attributes: SELECT_ATTRS,
    },
    TagDef {
        name: "option",
        category: TagCategory::Controls,
        self_closing: false,
        description: "Static option inside a select.",
        bases: BASE_MESH,
        attributes: OPTION_ATTRS,
    },
    TagDef {
        name: "radio-group",
        category: TagCategory::Controls,
        self_closing: false,
        description: "Exclusive group for radio choices.",
        bases: BASE_MESH_INTERACTIVE,
        attributes: SELECT_ATTRS,
    },
    TagDef {
        name: "radio",
        category: TagCategory::Controls,
        self_closing: false,
        description: "Exclusive radio choice inside a radio-group.",
        bases: BASE_MESH_INTERACTIVE,
        attributes: RADIO_ATTRS,
    },
    TagDef {
        name: "segmented-control",
        category: TagCategory::Controls,
        self_closing: false,
        description: "Configured choice group; native split behavior is deferred.",
        bases: BASE_MESH_INTERACTIVE,
        attributes: SELECT_ATTRS,
    },
    TagDef {
        name: "menu",
        category: TagCategory::Controls,
        self_closing: false,
        description: "Roving-focus command list.",
        bases: BASE_MESH_INTERACTIVE,
        attributes: MENU_ATTRS,
    },
    TagDef {
        name: "menu-item",
        category: TagCategory::Controls,
        self_closing: false,
        description: "Command item inside a menu.",
        bases: BASE_MESH_INTERACTIVE,
        attributes: MENU_ITEM_ATTRS,
    },
    TagDef {
        name: "command-item",
        category: TagCategory::Controls,
        self_closing: false,
        description: "Command-oriented menu item alias.",
        bases: BASE_MESH_INTERACTIVE,
        attributes: MENU_ITEM_ATTRS,
    },
    TagDef {
        name: "preference-row",
        category: TagCategory::Controls,
        self_closing: false,
        description: "Configured menu/preference row.",
        bases: BASE_MESH_INTERACTIVE,
        attributes: MENU_ITEM_ATTRS,
    },
    TagDef {
        name: "list",
        category: TagCategory::Structure,
        self_closing: false,
        description: "Selectable collection container.",
        bases: BASE_MESH_INTERACTIVE,
        attributes: CONTAINER_ATTRS,
    },
    TagDef {
        name: "list-item",
        category: TagCategory::Structure,
        self_closing: false,
        description: "Activatable item inside a list.",
        bases: BASE_MESH_INTERACTIVE,
        attributes: COLLECTION_ATTRS,
    },
    TagDef {
        name: "empty-state",
        category: TagCategory::Structure,
        self_closing: false,
        description: "Fallback content shown when a collection is empty.",
        bases: BASE_MESH,
        attributes: CONTAINER_ATTRS,
    },
    TagDef {
        name: "tabs",
        category: TagCategory::Composition,
        self_closing: false,
        description: "Tab group container.",
        bases: BASE_MESH,
        attributes: CONTAINER_ATTRS,
    },
    TagDef {
        name: "tab",
        category: TagCategory::Controls,
        self_closing: false,
        description: "Activatable tab inside tabs.",
        bases: BASE_MESH_INTERACTIVE,
        attributes: TAB_ATTRS,
    },
    TagDef {
        name: "accordion",
        category: TagCategory::Composition,
        self_closing: false,
        description: "Expandable section group.",
        bases: BASE_MESH_INTERACTIVE,
        attributes: CONTAINER_ATTRS,
    },
    TagDef {
        name: "details",
        category: TagCategory::Composition,
        self_closing: false,
        description: "Expandable details container.",
        bases: BASE_MESH_INTERACTIVE,
        attributes: CONTAINER_ATTRS,
    },
    TagDef {
        name: "popover",
        category: TagCategory::Composition,
        self_closing: false,
        description: "Popover container promoted to its own xdg_popup child surface, positioned via anchor/gravity against an anchor-ref trigger.",
        bases: BASE_MESH_INTERACTIVE,
        attributes: POPOVER_ATTRS,
    },
    TagDef {
        name: "dialog",
        category: TagCategory::Composition,
        self_closing: false,
        description: "Dialog container; full modal trapping is deferred.",
        bases: BASE_MESH_INTERACTIVE,
        attributes: CONTAINER_ATTRS,
    },
    TagDef {
        name: "sheet",
        category: TagCategory::Composition,
        self_closing: false,
        description: "Configured sheet container.",
        bases: BASE_MESH,
        attributes: CONTAINER_ATTRS,
    },
    TagDef {
        name: "table",
        category: TagCategory::Structure,
        self_closing: false,
        description: "Semantic table collection; rich behavior is deferred.",
        bases: BASE_MESH,
        attributes: CONTAINER_ATTRS,
    },
    TagDef {
        name: "cell",
        category: TagCategory::Structure,
        self_closing: false,
        description: "Semantic table cell.",
        bases: BASE_MESH,
        attributes: COLLECTION_ATTRS,
    },
    TagDef {
        name: "tree",
        category: TagCategory::Structure,
        self_closing: false,
        description: "Semantic tree collection; rich behavior is deferred.",
        bases: BASE_MESH,
        attributes: CONTAINER_ATTRS,
    },
    TagDef {
        name: "slot",
        category: TagCategory::Structure,
        self_closing: true,
        description: "Projection slot for child content.",
        bases: BASE_MESH,
        attributes: SLOT_ATTRS,
    },
    TagDef {
        name: "surface",
        category: TagCategory::Composition,
        self_closing: false,
        description: "Surface composition primitive.",
        bases: BASE_MESH,
        attributes: NO_ATTRS,
    },
    TagDef {
        name: "widget",
        category: TagCategory::Composition,
        self_closing: false,
        description: "Widget composition primitive.",
        bases: BASE_MESH,
        attributes: NO_ATTRS,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(name: &str) -> &'static TagDef {
        TAG_DEFS
            .iter()
            .find(|tag| tag.name == name)
            .unwrap_or_else(|| panic!("missing tag {name}"))
    }

    fn attr_names(tag: &TagDef) -> Vec<&'static str> {
        tag.all_attributes()
            .into_iter()
            .map(|attr| attr.name)
            .collect()
    }

    #[test]
    fn gesture_and_touch_handlers_are_available_to_completion_and_hover() {
        for name in [
            "ontwofingerscroll",
            "onswipe",
            "onpinch",
            "onhold",
            "ontouchstart",
            "ontouchmove",
            "ontouchend",
            "ontouchcancel",
            "ontap",
            "ondoubletap",
            "onlongpress",
        ] {
            let attr = EVENT_ATTRS
                .iter()
                .find(|attr| attr.name == name)
                .unwrap_or_else(|| panic!("missing LSP event metadata for {name}"));
            assert!(
                !attr.description.is_empty(),
                "missing hover docs for {name}"
            );
        }
    }

    #[test]
    fn phase87_lsp_tag_knowledge_includes_layout_display_primitives() {
        for name in [
            "grid",
            "scroll-area",
            "section",
            "header",
            "footer",
            "group",
            "form-row",
            "badge",
            "progress",
            "tooltip",
            "avatar",
            "shortcut",
        ] {
            tag(name);
        }
    }

    #[test]
    fn phase87_lsp_progress_and_grid_attributes_match_contract() {
        let progress_attrs = attr_names(tag("progress"));
        for name in ["value", "min", "max", "indeterminate"] {
            assert!(
                progress_attrs.contains(&name),
                "progress should complete {name}"
            );
        }

        let grid_attrs = attr_names(tag("grid"));
        for name in [
            "columns",
            "rows",
            "column",
            "row",
            "column-span",
            "row-span",
        ] {
            assert!(grid_attrs.contains(&name), "grid should complete {name}");
        }

        let meter_attrs = attr_names(tag("meter"));
        assert!(
            !meter_attrs.contains(&"indeterminate"),
            "meter stays taxonomy-only in Phase 87"
        );
    }

    #[test]
    fn phase88_lsp_prefers_single_button_without_icon_shortcut_attributes() {
        let button_attrs = attr_names(tag("button"));
        for name in [
            "variant",
            "pressed",
            "busy",
            "default",
            "destructive",
            "keybind",
        ] {
            assert!(
                button_attrs.contains(&name),
                "button should complete {name}"
            );
        }
        for name in ["icon", "name", "src"] {
            assert!(
                !button_attrs.contains(&name),
                "button should not complete icon shortcut attr {name}"
            );
        }

        for name in [
            "icon-button",
            "toggle-button",
            "command-button",
            "link-button",
        ] {
            let compat = tag(name);
            assert!(
                compat.description.contains("Compatibility alias"),
                "{name} should be documented as a compatibility alias"
            );
            assert!(!attr_names(compat).contains(&"src"));
        }
    }

    #[test]
    fn phase88_lsp_input_variants_include_config_attrs() {
        for name in [
            "input",
            "textarea",
            "search",
            "password",
            "number-input",
            "stepper",
        ] {
            tag(name);
        }

        let textarea_attrs = attr_names(tag("textarea"));
        assert!(textarea_attrs.contains(&"placeholder"));
        assert!(textarea_attrs.contains(&"multiline"));

        let password_attrs = attr_names(tag("password"));
        assert!(password_attrs.contains(&"type"));
        assert!(password_attrs.contains(&"masked"));

        let number_attrs = attr_names(tag("number-input"));
        for name in ["value", "min", "max", "step"] {
            assert!(
                number_attrs.contains(&name),
                "number-input should complete {name}"
            );
        }
    }

    #[test]
    fn phase89_lsp_choice_and_menu_tags_include_static_authoring_attrs() {
        for name in [
            "select",
            "option",
            "checkbox",
            "switch",
            "radio-group",
            "radio",
            "segmented-control",
            "menu",
            "menu-item",
            "command-item",
            "preference-row",
        ] {
            tag(name);
        }

        let select_attrs = attr_names(tag("select"));
        assert!(select_attrs.contains(&"value"));
        assert!(select_attrs.contains(&"onchange"));

        let option_attrs = attr_names(tag("option"));
        assert!(option_attrs.contains(&"value"));
        assert!(option_attrs.contains(&"selected"));

        let menu_item_attrs = attr_names(tag("menu-item"));
        assert!(menu_item_attrs.contains(&"onactivate"));
        assert!(menu_item_attrs.contains(&"disabled"));
        assert!(menu_item_attrs.contains(&"keybind"));
    }

    #[test]
    fn phase90_lsp_container_and_collection_tags_include_authoring_attrs() {
        for name in [
            "popover",
            "dialog",
            "sheet",
            "tabs",
            "tab",
            "accordion",
            "details",
            "list",
            "list-item",
            "table",
            "cell",
            "tree",
            "empty-state",
        ] {
            tag(name);
        }

        let tab_attrs = attr_names(tag("tab"));
        assert!(tab_attrs.contains(&"selected"));
        assert!(tab_attrs.contains(&"onactivate"));

        let item_attrs = attr_names(tag("list-item"));
        assert!(item_attrs.contains(&"selected"));
        assert!(item_attrs.contains(&"onactivate"));

        let dialog_attrs = attr_names(tag("dialog"));
        assert!(dialog_attrs.contains(&"open"));
        assert!(dialog_attrs.contains(&"label"));
    }
}
