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
        description: "Absolute or plugin-relative path to an icon file",
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

static BUTTON_ATTRS: &[AttrDef] = &[AttrDef {
    name: "variant",
    description: "Visual variant: filled | outlined | text | tonal",
}];

static ICON_BUTTON_ATTRS: &[AttrDef] = &[
    AttrDef {
        name: "name",
        description: "XDG icon name for the button icon",
    },
    AttrDef {
        name: "src",
        description: "Icon file path",
    },
    AttrDef {
        name: "size",
        description: "Icon size in pixels",
    },
    AttrDef {
        name: "tooltip",
        description: "Accessible tooltip text",
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
        attributes: NO_ATTRS,
    },
    TagDef {
        name: "stack",
        category: TagCategory::Layout,
        self_closing: false,
        description: "Layered container where children overlap.",
        bases: BASE_MESH,
        attributes: NO_ATTRS,
    },
    TagDef {
        name: "scroll-view",
        category: TagCategory::Layout,
        self_closing: false,
        description: "Semantic scrollable region.",
        bases: BASE_MESH,
        attributes: NO_ATTRS,
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
        name: "separator",
        category: TagCategory::Layout,
        self_closing: true,
        description: "Horizontal or vertical visual separator line.",
        bases: BASE_MESH,
        attributes: NO_ATTRS,
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
        self_closing: true,
        description: "Icon-only clickable action.",
        bases: BASE_MESH_INTERACTIVE,
        attributes: ICON_BUTTON_ATTRS,
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
        attributes: NO_ATTRS,
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
