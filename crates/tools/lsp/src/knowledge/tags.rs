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

const MESH_ELEMENT_ATTRS: &[AttrDef] = &[
    AttrDef {
        name: "class",
        description: "CSS class name(s)",
    },
    AttrDef {
        name: "id",
        description: "Element identifier for CSS targeting",
    },
    AttrDef {
        name: "ref",
        description: "Stable element reference name for runtime metrics under refs.<name>",
    },
    AttrDef {
        name: "style",
        description: "Inline CSS styles",
    },
    AttrDef {
        name: "aria-label",
        description: "Accessible label",
    },
    AttrDef {
        name: "aria-role",
        description: "WAI-ARIA role override",
    },
    AttrDef {
        name: "title",
        description: "Tooltip / accessible title",
    },
    AttrDef {
        name: "aria-hidden",
        description: "Hide from accessibility tree",
    },
];

const INTERACTIVE_ELEMENT_ATTRS: &[AttrDef] = &[
    AttrDef {
        name: "onclick",
        description: "Click / tap handler function",
    },
    AttrDef {
        name: "onactivate",
        description: "Keyboard or command activation handler function",
    },
    AttrDef {
        name: "onchange",
        description: "Value change handler function",
    },
    AttrDef {
        name: "oninput",
        description: "User input handler function",
    },
    AttrDef {
        name: "onfocus",
        description: "Focus received handler function",
    },
    AttrDef {
        name: "onblur",
        description: "Focus lost handler function",
    },
    AttrDef {
        name: "onkeydown",
        description: "Key press handler function",
    },
];

const VALUE_ELEMENT_ATTRS: &[AttrDef] = &[
    AttrDef {
        name: "value",
        description: "Current control value",
    },
    AttrDef {
        name: "disabled",
        description: "Disables user interaction",
    },
];

const TEXT_INPUT_ELEMENT_ATTRS: &[AttrDef] = &[
    AttrDef {
        name: "placeholder",
        description: "Placeholder text shown while empty",
    },
    AttrDef {
        name: "readonly",
        description: "Makes the input read-only",
    },
    AttrDef {
        name: "type",
        description: "Input type: text | password | number | email | url | search",
    },
];

const RANGE_ELEMENT_ATTRS: &[AttrDef] = &[
    AttrDef {
        name: "min",
        description: "Minimum value",
    },
    AttrDef {
        name: "max",
        description: "Maximum value",
    },
    AttrDef {
        name: "step",
        description: "Step size",
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
}];

static LABEL_ATTRS: &[AttrDef] = &[AttrDef {
    name: "for",
    description: "ID of the associated input element",
}];

static ICON_ATTRS: &[AttrDef] = &[
    AttrDef {
        name: "name",
        description: "XDG icon name (e.g. audio-volume-high)",
    },
    AttrDef {
        name: "src",
        description: "Absolute or module-relative path to an icon file",
    },
    AttrDef {
        name: "size",
        description: "Icon size hint in pixels for XDG resolution",
    },
    AttrDef {
        name: "alt",
        description: "Accessible alternative text",
    },
];

static IMAGE_ATTRS: &[AttrDef] = &[
    AttrDef {
        name: "src",
        description: "Image source path",
    },
    AttrDef {
        name: "alt",
        description: "Accessible alternative text",
    },
];

static LAYOUT_ATTRS: &[AttrDef] = &[
    AttrDef {
        name: "align",
        description: "Cross-axis alignment",
    },
    AttrDef {
        name: "justify",
        description: "Main-axis justification",
    },
    AttrDef {
        name: "spacing",
        description: "Spacing between children",
    },
    AttrDef {
        name: "gap",
        description: "Gap between children",
    },
    AttrDef {
        name: "overflow",
        description: "Overflow behavior",
    },
    AttrDef {
        name: "overflow-x",
        description: "Horizontal overflow behavior",
    },
    AttrDef {
        name: "overflow-y",
        description: "Vertical overflow behavior",
    },
];

static GRID_ATTRS: &[AttrDef] = &[
    AttrDef {
        name: "columns",
        description: "Grid columns as fixed px or auto tracks",
    },
    AttrDef {
        name: "rows",
        description: "Grid rows as fixed px or auto tracks",
    },
    AttrDef {
        name: "column",
        description: "Grid column placement",
    },
    AttrDef {
        name: "row",
        description: "Grid row placement",
    },
    AttrDef {
        name: "column-span",
        description: "Grid column span",
    },
    AttrDef {
        name: "row-span",
        description: "Grid row span",
    },
];

static STACK_ATTRS: &[AttrDef] = &[AttrDef {
    name: "layer",
    description: "Layer order for overlapping children",
}];

static DISPLAY_ATTRS: &[AttrDef] = &[
    AttrDef {
        name: "label",
        description: "Accessible label",
    },
    AttrDef {
        name: "tooltip",
        description: "Accessible tooltip text",
    },
];

static SHORTCUT_ATTRS: &[AttrDef] = &[AttrDef {
    name: "key",
    description: "Shortcut key label",
}];

static TOOLTIP_ATTRS: &[AttrDef] = &[AttrDef {
    name: "tooltip-for",
    description: "ID of the element that owns this tooltip",
}];

static PROGRESS_ATTRS: &[AttrDef] = &[
    AttrDef {
        name: "value",
        description: "Current progress value",
    },
    AttrDef {
        name: "min",
        description: "Minimum progress value",
    },
    AttrDef {
        name: "max",
        description: "Maximum progress value",
    },
    AttrDef {
        name: "indeterminate",
        description: "Progress has no determinate value",
    },
];

static BUTTON_ATTRS: &[AttrDef] = &[
    AttrDef {
        name: "variant",
        description: "Visual variant: filled | outlined | text | tonal",
    },
    AttrDef {
        name: "pressed",
        description: "Whether the button is in a pressed/toggled state",
    },
    AttrDef {
        name: "busy",
        description: "Whether the action is currently busy",
    },
    AttrDef {
        name: "default",
        description: "Marks the default action in a group",
    },
    AttrDef {
        name: "destructive",
        description: "Marks a destructive action",
    },
    AttrDef {
        name: "keybind",
        description: "Associated keybind id or display shortcut",
    },
    AttrDef {
        name: "command",
        description: "Command intent metadata",
    },
    AttrDef {
        name: "href",
        description: "Link intent metadata; navigation is handled by Luau",
    },
];

static TEXTAREA_ATTRS: &[AttrDef] = &[AttrDef {
    name: "multiline",
    description: "Whether this configured input accepts multiple lines",
}];

static PASSWORD_ATTRS: &[AttrDef] = &[AttrDef {
    name: "masked",
    description: "Whether this configured input masks displayed text",
}];

static NUMERIC_INPUT_ATTRS: &[AttrDef] = &[
    AttrDef {
        name: "min",
        description: "Minimum numeric value",
    },
    AttrDef {
        name: "max",
        description: "Maximum numeric value",
    },
    AttrDef {
        name: "step",
        description: "Positive numeric step size",
    },
];

static SWITCH_ATTRS: &[AttrDef] = &[AttrDef {
    name: "checked",
    description: "Whether the switch is on",
}];

static CHECKBOX_ATTRS: &[AttrDef] = &[
    AttrDef {
        name: "checked",
        description: "Whether the checkbox is checked",
    },
    AttrDef {
        name: "label",
        description: "Associated label text",
    },
];

static SELECT_ATTRS: &[AttrDef] = &[
    AttrDef {
        name: "value",
        description: "Selected option value",
    },
    AttrDef {
        name: "disabled",
        description: "Disables selection changes",
    },
    AttrDef {
        name: "required",
        description: "Requires a selected value",
    },
];

static OPTION_ATTRS: &[AttrDef] = &[
    AttrDef {
        name: "value",
        description: "Value sent to the parent select onchange handler",
    },
    AttrDef {
        name: "selected",
        description: "Marks this option as selected",
    },
    AttrDef {
        name: "disabled",
        description: "Disables this option",
    },
];

static RADIO_ATTRS: &[AttrDef] = &[
    AttrDef {
        name: "value",
        description: "Value sent to the parent radio-group onchange handler",
    },
    AttrDef {
        name: "name",
        description: "Radio group name when not nested in a radio-group",
    },
    AttrDef {
        name: "checked",
        description: "Whether this radio choice is checked",
    },
    AttrDef {
        name: "disabled",
        description: "Disables this radio choice",
    },
];

static MENU_ATTRS: &[AttrDef] = &[
    AttrDef {
        name: "expanded",
        description: "Whether the menu is expanded",
    },
    AttrDef {
        name: "disabled",
        description: "Disables the menu",
    },
];

static MENU_ITEM_ATTRS: &[AttrDef] = &[
    AttrDef {
        name: "disabled",
        description: "Disables item activation",
    },
    AttrDef {
        name: "selected",
        description: "Whether this item is selected",
    },
    AttrDef {
        name: "keybind",
        description: "Associated keybind id or display shortcut",
    },
    AttrDef {
        name: "command",
        description: "Command intent metadata",
    },
];

static LIST_ITEM_ATTRS: &[AttrDef] = &[
    AttrDef {
        name: "selected",
        description: "Whether this item is selected",
    },
    AttrDef {
        name: "disabled",
        description: "Disables the list item",
    },
];

static SLOT_ATTRS: &[AttrDef] = &[AttrDef {
    name: "name",
    description: "Slot name (matches the parent component's slot)",
}];

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
        description: "List container.",
        bases: BASE_MESH,
        attributes: NO_ATTRS,
    },
    TagDef {
        name: "list-item",
        category: TagCategory::Structure,
        self_closing: false,
        description: "A single item inside a list.",
        bases: BASE_MESH,
        attributes: LIST_ITEM_ATTRS,
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
}
