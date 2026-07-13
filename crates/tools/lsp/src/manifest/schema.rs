//! A small, hand-authored description of the canonical `module.json` schema.
//!
//! This mirrors the runtime structs in `mesh_core_module` (`ModuleManifest` /
//! `MeshModuleSection` for per-module manifests and `RootModuleGraphManifest`
//! for the workspace `config/module.json`). It is the single source of truth for
//! manifest key completion, hover documentation, and unknown-key / enum
//! diagnostics. When the runtime schema changes, update this tree to match.

/// Which manifest flavor a document is. Both share the `name`/`version`/`mesh`
/// envelope but the contents of the `mesh` section differ completely.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManifestFlavor {
    /// A per-module manifest (`mesh.kind`, `mesh.apiVersion`, ...).
    Module,
    /// The workspace root graph config (`mesh.schemaVersion`, `mesh.modulesDir`, ...).
    RootConfig,
}

/// The valid `mesh.kind` values, matching `ModuleKind` (serde kebab-case).
pub const MODULE_KINDS: &[&str] = &[
    "frontend",
    "backend",
    "interface",
    "theme",
    "icon-pack",
    "font-pack",
    "language-pack",
    "library",
    "component",
];

/// A curated set of well-known capability strings. Capabilities are extensible
/// (`<domain>.<action>`), so this drives completion only — unknown capabilities
/// are never flagged as errors.
pub const KNOWN_CAPABILITIES: &[&str] = &[
    "shell.surface",
    "shell.widget",
    "service.audio.read",
    "service.audio.control",
    "service.network.read",
    "service.media.read",
    "service.power.read",
    "service.notifications.post",
    "service.hyprland.read",
    "service.hyprland.control",
    "service.debug.read",
    "theme.read",
    "theme.write",
    "locale.read",
    "locale.write",
    "exec.launch-app",
];

/// One node in the schema tree.
pub struct Node {
    pub doc: &'static str,
    pub type_hint: &'static str,
    pub kind: Kind,
}

pub enum Kind {
    /// An object with a fixed set of known properties.
    Object(Vec<Field>),
    /// An object with arbitrary string keys, each mapping to `value`.
    Map(Box<Node>),
    /// An array whose elements match `element`.
    Array(Box<Node>),
    /// A string constrained to one of these values.
    Enum(&'static [&'static str]),
    /// A string with suggested values that are *not* enforced. Used for
    /// extensible vocabularies like capabilities (`<domain>.<action>`), so
    /// completion offers known entries but unknown values are never flagged.
    Suggest(&'static [&'static str]),
    /// A leaf scalar (string / number / bool / freeform). Accepts any JSON.
    Scalar,
}

/// A named property of an object node.
pub struct Field {
    pub name: &'static str,
    /// True if the field must be present.
    pub required: bool,
    pub node: Node,
}

fn obj(doc: &'static str, fields: Vec<Field>) -> Node {
    Node {
        doc,
        type_hint: "object",
        kind: Kind::Object(fields),
    }
}

fn scalar(doc: &'static str, type_hint: &'static str) -> Node {
    Node {
        doc,
        type_hint,
        kind: Kind::Scalar,
    }
}

fn field(name: &'static str, required: bool, node: Node) -> Field {
    Field {
        name,
        required,
        node,
    }
}

/// Resolve the schema root for a flavor.
pub fn root(flavor: ManifestFlavor) -> Node {
    match flavor {
        ManifestFlavor::Module => module_root(),
        ManifestFlavor::RootConfig => root_config_root(),
    }
}

/// The synthetic path segment used for "an element inside an array".
pub const ARRAY_ELEMENT: &str = "[]";

/// Navigate the schema tree following a container path of object keys. Array
/// element steps use the [`ARRAY_ELEMENT`] sentinel. Returns the node at the
/// path, or `None` if the path leaves the known schema (e.g. inside a free-form
/// map or `experimental`).
pub fn navigate<'a>(node: &'a Node, path: &[String]) -> Option<&'a Node> {
    let Some((head, rest)) = path.split_first() else {
        return Some(node);
    };
    let next = match &node.kind {
        Kind::Object(fields) => fields.iter().find(|f| f.name == head).map(|f| &f.node),
        Kind::Map(value) => Some(value.as_ref()),
        Kind::Array(element) if head == ARRAY_ELEMENT => Some(element.as_ref()),
        _ => None,
    }?;
    navigate(next, rest)
}

fn localized_text(doc: &'static str) -> Node {
    // A localized string is either a bare string or `{ t, fallback }`, so it is
    // modelled as a permissive scalar to accept both shapes without flagging.
    scalar(doc, "string | { t, fallback }")
}

fn dependency_map(doc: &'static str) -> Node {
    Node {
        doc,
        type_hint: "object<string, version-spec>",
        kind: Kind::Map(Box::new(scalar(
            "Semver requirement (e.g. \">=1.0\", \"^0.1.0\").",
            "version-spec",
        ))),
    }
}

fn string_array(doc: &'static str, element_doc: &'static str) -> Node {
    Node {
        doc,
        type_hint: "array<string>",
        kind: Kind::Array(Box::new(scalar(element_doc, "string"))),
    }
}

fn capabilities_array(doc: &'static str) -> Node {
    Node {
        doc,
        type_hint: "array<capability>",
        kind: Kind::Array(Box::new(Node {
            doc: "A capability string in `<domain>.<action>` form. Capabilities are \
                  extensible, so unknown values are allowed.",
            type_hint: "capability",
            kind: Kind::Suggest(KNOWN_CAPABILITIES),
        })),
    }
}

fn binary_dependency(doc: &'static str) -> Node {
    Node {
        doc,
        type_hint: "array",
        kind: Kind::Array(Box::new(obj(
            "A required external binary.",
            vec![
                field(
                    "name",
                    true,
                    scalar("Executable name, e.g. `wpctl`.", "string"),
                ),
                field(
                    "version",
                    false,
                    scalar("Minimum version, if any.", "string"),
                ),
                field(
                    "reason",
                    false,
                    scalar("Why this binary is needed.", "string"),
                ),
                field(
                    "optional",
                    false,
                    scalar("Whether the binary is optional.", "boolean"),
                ),
                field(
                    "packages",
                    false,
                    Node {
                        doc: "Distro → package name providing this binary.",
                        type_hint: "object<string, string>",
                        kind: Kind::Map(Box::new(scalar("Package name.", "string"))),
                    },
                ),
            ],
        ))),
    }
}

fn module_root() -> Node {
    obj(
        "A MESH module manifest. The npm-style envelope (`name`, `version`) wraps \
         a `mesh` section describing the module.",
        vec![
            field(
                "name",
                true,
                scalar("Scoped module id, e.g. `@mesh/navigation-bar`.", "string"),
            ),
            field(
                "version",
                true,
                scalar("Semver version of this module.", "string"),
            ),
            field(
                "description",
                false,
                scalar("Human-readable description of the module.", "string"),
            ),
            field(
                "license",
                false,
                scalar("SPDX license identifier.", "string"),
            ),
            field(
                "authors",
                false,
                string_array("Module authors.", "Author name."),
            ),
            field(
                "keywords",
                false,
                string_array("Search keywords.", "Keyword."),
            ),
            field("homepage", false, scalar("Project homepage URL.", "string")),
            field(
                "private",
                false,
                scalar("Marks the package as never-published.", "boolean"),
            ),
            field(
                "repository",
                false,
                obj(
                    "Source repository metadata.",
                    vec![
                        field("type", false, scalar("VCS type, e.g. `git`.", "string")),
                        field("url", false, scalar("Repository URL.", "string")),
                    ],
                ),
            ),
            field("mesh", true, mesh_section()),
        ],
    )
}

fn mesh_section() -> Node {
    obj(
        "MESH-specific module metadata.",
        vec![
            field(
                "apiVersion",
                true,
                scalar(
                    "MESH module API version this manifest targets, e.g. \"0.1\".",
                    "string",
                ),
            ),
            field(
                "kind",
                true,
                Node {
                    doc: "The module role. Determines how the core loads and wires the module.",
                    type_hint: "enum",
                    kind: Kind::Enum(MODULE_KINDS),
                },
            ),
            field(
                "entry",
                false,
                scalar(
                    "Path to the entrypoint, e.g. `src/main.mesh` (frontend) or `src/main.luau` (backend).",
                    "path",
                ),
            ),
            field(
                "compatibility",
                false,
                obj(
                    "Runtime / compositor compatibility constraints.",
                    vec![
                        field(
                            "mesh",
                            false,
                            scalar("MESH runtime version requirement.", "version-spec"),
                        ),
                        field(
                            "compositors",
                            false,
                            string_array(
                                "Required compositor protocols.",
                                "Protocol, e.g. `wlr-layer-shell-v1`.",
                            ),
                        ),
                    ],
                ),
            ),
            field("uses", false, mesh_uses()),
            field(
                "capabilities",
                false,
                obj(
                    "Capability gates this module requires or optionally uses.",
                    vec![
                        field(
                            "required",
                            false,
                            capabilities_array("Capabilities that must be granted."),
                        ),
                        field(
                            "optional",
                            false,
                            capabilities_array("Capabilities used if available."),
                        ),
                    ],
                ),
            ),
            field(
                "entrypoints",
                false,
                obj(
                    "Named entrypoints for the module.",
                    vec![
                        field("main", false, scalar("Primary entrypoint path.", "path")),
                        field(
                            "settingsUi",
                            false,
                            scalar("Settings UI entrypoint path.", "path"),
                        ),
                    ],
                ),
            ),
            field(
                "keybinds",
                false,
                Node {
                    doc: "Declarative keybind metadata, keyed by action id.",
                    type_hint: "object",
                    kind: Kind::Map(Box::new(keybind_node())),
                },
            ),
            field("dependencies", false, mesh_dependencies()),
            field(
                "provides",
                false,
                mesh_contributes("Resources this module provides (legacy alias of `contributes`)."),
            ),
            field(
                "contributes",
                false,
                mesh_contributes("Resources this module contributes to the shell."),
            ),
            field(
                "implements",
                false,
                Node {
                    doc: "Interfaces this backend module implements.",
                    type_hint: "array",
                    kind: Kind::Array(Box::new(implements_node())),
                },
            ),
            field("interface", false, interface_node()),
            field(
                "interfaces",
                false,
                Node {
                    doc: "Inline interface contract declarations on a backend module — \
                          the low-friction contract carrier for single-provider domains. \
                          Multi-provider domains keep a standalone interface module.",
                    type_hint: "array",
                    kind: Kind::Array(Box::new(interface_node())),
                },
            ),
            field(
                "theme",
                false,
                scalar(
                    "Theme definition contributed by this module (tokens, modes, base, extends).",
                    "object",
                ),
            ),
            field(
                "i18n",
                false,
                obj(
                    "Localization metadata.",
                    vec![
                        field(
                            "defaultLocale",
                            false,
                            scalar("Default locale, e.g. `en`.", "string"),
                        ),
                        field(
                            "supportedLocales",
                            false,
                            string_array("Locales this module ships.", "Locale code."),
                        ),
                    ],
                ),
            ),
            field(
                "iconRequirements",
                false,
                obj(
                    "Icons this module expects to be resolvable from the active icon theme.",
                    vec![
                        field(
                            "required",
                            false,
                            string_array("Required icon names.", "Icon name."),
                        ),
                        field(
                            "optional",
                            false,
                            string_array("Optional icon names.", "Icon name."),
                        ),
                    ],
                ),
            ),
            field("icons", false, scalar("Icon set contents.", "object")),
            field("icon_pack", false, scalar("Icon pack metadata.", "object")),
            field("surface", false, surface_layout_node()),
            field("surfaceLayout", false, surface_layout_node()),
            field(
                "accessibility",
                false,
                obj(
                    "Default accessibility metadata for the module's surface.",
                    vec![
                        field("role", false, scalar("Accessibility role.", "string")),
                        field("label", false, scalar("Accessibility label.", "string")),
                        field(
                            "description",
                            false,
                            scalar("Accessibility description.", "string"),
                        ),
                    ],
                ),
            ),
            field(
                "experimental",
                false,
                Node {
                    doc: "Unvalidated experimental fields. Anything here is passed through untouched.",
                    type_hint: "any",
                    kind: Kind::Scalar,
                },
            ),
        ],
    )
}

fn mesh_uses() -> Node {
    obj(
        "Declares what this module consumes: other modules, interface contracts, \
         resources, and capabilities.",
        vec![
            field(
                "modules",
                false,
                dependency_map("Module id → version requirement."),
            ),
            field(
                "interfaces",
                false,
                dependency_map("Interface name → version requirement (required)."),
            ),
            field(
                "optionalInterfaces",
                false,
                dependency_map("Interface name → version requirement (optional)."),
            ),
            field(
                "resources",
                false,
                obj(
                    "Resource packs this module draws from.",
                    vec![
                        field(
                            "icons",
                            false,
                            string_array("Icon pack module ids.", "Icon pack id."),
                        ),
                        field(
                            "fonts",
                            false,
                            string_array("Font pack module ids.", "Font pack id."),
                        ),
                        field(
                            "themes",
                            false,
                            string_array("Theme module ids.", "Theme id."),
                        ),
                    ],
                ),
            ),
            field(
                "capabilities",
                false,
                capabilities_array("Required capabilities."),
            ),
            field(
                "optionalCapabilities",
                false,
                capabilities_array("Optional capabilities."),
            ),
            field(
                "binaries",
                false,
                binary_dependency("External binaries this module requires."),
            ),
            field(
                "iconRequirements",
                false,
                obj(
                    "Icon requirements for this module.",
                    vec![
                        field(
                            "required",
                            false,
                            string_array("Required icon names.", "Icon name."),
                        ),
                        field(
                            "optional",
                            false,
                            string_array("Optional icon names.", "Icon name."),
                        ),
                    ],
                ),
            ),
        ],
    )
}

fn mesh_dependencies() -> Node {
    obj(
        "Concrete dependency pins (distinct from `uses`, which declares contracts).",
        vec![
            field("modules", false, dependency_map("Module id → version.")),
            field(
                "backend",
                false,
                dependency_map("Interface name → backend provider module id."),
            ),
            field(
                "optionalBackend",
                false,
                dependency_map("Optional backend providers."),
            ),
            field("icons", false, dependency_map("Icon pack id → version.")),
            field("fonts", false, dependency_map("Font pack id → version.")),
            field("themes", false, dependency_map("Theme id → version.")),
            field(
                "binaries",
                false,
                binary_dependency("External binaries this module depends on."),
            ),
        ],
    )
}

fn mesh_contributes(doc: &'static str) -> Node {
    obj(
        doc,
        vec![
            field(
                "layout",
                false,
                Node {
                    doc: "Layout entrypoints this module contributes.",
                    type_hint: "array",
                    kind: Kind::Array(Box::new(obj(
                        "A layout contribution.",
                        vec![
                            field("id", true, scalar("Layout id.", "string")),
                            field("entrypoint", true, scalar("Entrypoint path.", "path")),
                            field("label", false, localized_text("Display label.")),
                        ],
                    ))),
                },
            ),
            field(
                "settings",
                false,
                obj(
                    "Settings schema contribution.",
                    vec![
                        field("namespace", true, scalar("Settings namespace.", "string")),
                        field(
                            "schema",
                            false,
                            Node {
                                doc: "JSON-schema-like settings definition.",
                                type_hint: "object",
                                kind: Kind::Scalar,
                            },
                        ),
                    ],
                ),
            ),
            field(
                "themes",
                false,
                Node {
                    doc: "Theme contributions.",
                    type_hint: "array",
                    kind: Kind::Array(Box::new(obj(
                        "A theme contribution.",
                        vec![
                            field("id", true, scalar("Theme id.", "string")),
                            field("label", false, localized_text("Display label.")),
                            field("defaultMode", false, scalar("Default mode id.", "string")),
                            field("modes", false, dependency_map("Mode id → token-set path.")),
                        ],
                    ))),
                },
            ),
            field(
                "icons",
                false,
                path_contribution_array("Icon contributions."),
            ),
            field(
                "fonts",
                false,
                path_contribution_array("Font contributions."),
            ),
            field(
                "i18n",
                false,
                Node {
                    doc: "Localization bundle contributions.",
                    type_hint: "array",
                    kind: Kind::Array(Box::new(obj(
                        "An i18n contribution.",
                        vec![
                            field("id", true, scalar("Bundle id.", "string")),
                            field("locale", true, scalar("Locale code, e.g. `en`.", "string")),
                            field("path", true, scalar("Path to the locale bundle.", "path")),
                        ],
                    ))),
                },
            ),
            field(
                "libraries",
                false,
                Node {
                    doc: "Luau library contributions.",
                    type_hint: "array",
                    kind: Kind::Array(Box::new(obj(
                        "A library contribution.",
                        vec![
                            field("namespace", true, scalar("Importable namespace.", "string")),
                            field("path", true, scalar("Path to the library source.", "path")),
                        ],
                    ))),
                },
            ),
        ],
    )
}

fn path_contribution_array(doc: &'static str) -> Node {
    Node {
        doc,
        type_hint: "array",
        kind: Kind::Array(Box::new(obj(
            "A path contribution.",
            vec![
                field("id", true, scalar("Resource id.", "string")),
                field("path", true, scalar("Path to the resource.", "path")),
                field("label", false, localized_text("Display label.")),
            ],
        ))),
    }
}

fn keybind_node() -> Node {
    obj(
        "A declarative keybind definition.",
        vec![
            field(
                "label",
                false,
                localized_text("Localized label for the shortcut."),
            ),
            field(
                "description",
                false,
                localized_text("Localized description."),
            ),
            field(
                "category",
                false,
                localized_text("Localized category grouping."),
            ),
            field(
                "trigger",
                false,
                obj(
                    "How the keybind is triggered.",
                    vec![
                        field(
                            "kind",
                            false,
                            scalar("Trigger kind, e.g. `shortcut`.", "string"),
                        ),
                        field("key", false, scalar("Key, e.g. `m`.", "string")),
                        field(
                            "modifiers",
                            false,
                            string_array("Modifier keys.", "Modifier, e.g. `super`."),
                        ),
                    ],
                ),
            ),
        ],
    )
}

fn implements_node() -> Node {
    obj(
        "An interface implementation declaration.",
        vec![
            field(
                "interface",
                true,
                scalar("Interface name, e.g. `mesh.audio`.", "string"),
            ),
            field(
                "version",
                false,
                scalar("Implemented interface version.", "string"),
            ),
            field(
                "baseModule",
                false,
                scalar("Base module id this provider extends.", "string"),
            ),
            field("provider", false, scalar("Provider id.", "string")),
            field("label", false, localized_text("Display label.")),
            field(
                "priority",
                false,
                scalar("Selection priority (higher wins).", "number"),
            ),
        ],
    )
}

fn interface_node() -> Node {
    obj(
        "Interface contract declared by an `interface` module.",
        vec![
            field(
                "name",
                true,
                scalar("Interface name, e.g. `mesh.audio`.", "string"),
            ),
            field("version", false, scalar("Interface version.", "string")),
            field("contract", false, contract_node()),
            field(
                "domain",
                false,
                scalar("Capability domain, e.g. `audio`.", "string"),
            ),
            field(
                "extends",
                false,
                scalar("Interface this one extends.", "string"),
            ),
            field(
                "relationship",
                false,
                Node {
                    doc: "Relationship to the extended interface.",
                    type_hint: "enum",
                    kind: Kind::Enum(&["base", "extension", "independent"]),
                },
            ),
            field(
                "reason",
                false,
                scalar("Why this relationship exists.", "string"),
            ),
        ],
    )
}

fn contract_node() -> Node {
    let typed_field = || {
        obj(
            "A named, typed field.",
            vec![
                field("name", true, scalar("Field name.", "string")),
                field(
                    "type",
                    true,
                    scalar(
                        "Type expression: string, int, float, boolean, object, any, a named \
                         type from `types`, with optional `[]` (array) and `?` (optional) \
                         suffixes.",
                        "string",
                    ),
                ),
                field("description", false, scalar("Field description.", "string")),
            ],
        )
    };
    obj(
        "Inline interface contract JSON: state fields, command methods, events, \
         named types, and consumer capabilities.",
        vec![
            field(
                "state",
                false,
                Node {
                    doc: "Public state fields every provider must emit; read through \
                          the service proxy as plain field access.",
                    type_hint: "array",
                    kind: Kind::Array(Box::new(typed_field())),
                },
            ),
            field(
                "methods",
                false,
                Node {
                    doc: "Mutating command methods callable from frontend scripts.",
                    type_hint: "array",
                    kind: Kind::Array(Box::new(obj(
                        "A command method declaration.",
                        vec![
                            field("name", true, scalar("Command name.", "string")),
                            field(
                                "args",
                                false,
                                Node {
                                    doc: "Typed command arguments.",
                                    type_hint: "array",
                                    kind: Kind::Array(Box::new(typed_field())),
                                },
                            ),
                            field(
                                "returns",
                                false,
                                scalar("Return type expression.", "string"),
                            ),
                            field(
                                "coalesce",
                                false,
                                scalar(
                                    "Coalesce queued duplicates to the most recent payload \
                                     (idempotent setters only).",
                                    "boolean",
                                ),
                            ),
                            field(
                                "optimistic",
                                false,
                                obj(
                                    "Optimistic state patch applied on dispatch: set `field` \
                                     from `fromArg`, or toggle the boolean field when \
                                     `fromArg` is omitted.",
                                    vec![
                                        field(
                                            "field",
                                            true,
                                            scalar("State field to patch.", "string"),
                                        ),
                                        field(
                                            "fromArg",
                                            false,
                                            scalar(
                                                "Argument supplying the optimistic value.",
                                                "string",
                                            ),
                                        ),
                                    ],
                                ),
                            ),
                        ],
                    ))),
                },
            ),
            field(
                "events",
                false,
                Node {
                    doc: "Named events with typed payload fields.",
                    type_hint: "array",
                    kind: Kind::Array(Box::new(obj(
                        "An event declaration.",
                        vec![
                            field("name", true, scalar("Event name.", "string")),
                            field(
                                "payload",
                                false,
                                Node {
                                    doc: "Typed payload fields.",
                                    type_hint: "array",
                                    kind: Kind::Array(Box::new(typed_field())),
                                },
                            ),
                        ],
                    ))),
                },
            ),
            field(
                "types",
                false,
                scalar(
                    "Named record types referenced by type expressions, keyed by \
                     PascalCase name; each has a `fields` array.",
                    "object",
                ),
            ),
            field(
                "capabilities",
                false,
                obj(
                    "Consumer capabilities for this interface.",
                    vec![
                        field(
                            "required",
                            false,
                            scalar("Capabilities consumers must hold.", "array"),
                        ),
                        field(
                            "optional",
                            false,
                            scalar("Capabilities consumers may hold.", "array"),
                        ),
                    ],
                ),
            ),
        ],
    )
}

fn surface_layout_node() -> Node {
    obj(
        "Surface placement for a frontend module: anchor, layer, exclusive_zone, \
         keyboard_mode, visible_on_start, and margins. Surface sizing and the \
         show/hide transition are CSS concerns on the component root, not manifest \
         fields.",
        vec![
            field(
                "anchor",
                false,
                Node {
                    doc: "Screen edge the surface anchors to.",
                    type_hint: "enum",
                    kind: Kind::Enum(&["top", "bottom", "left", "right"]),
                },
            ),
            field(
                "layer",
                false,
                Node {
                    doc: "Layer-shell stacking layer.",
                    type_hint: "enum",
                    kind: Kind::Enum(&["background", "bottom", "top", "overlay"]),
                },
            ),
            field(
                "exclusive_zone",
                false,
                scalar("Reserved compositor space in px.", "number"),
            ),
            field(
                "visible_on_start",
                false,
                scalar("Whether the surface starts visible at boot.", "boolean"),
            ),
            field(
                "keyboard_mode",
                false,
                Node {
                    doc: "Keyboard interactivity mode.",
                    type_hint: "enum",
                    kind: Kind::Enum(&["none", "on_demand", "exclusive"]),
                },
            ),
            field(
                "margins",
                false,
                obj(
                    "Per-edge surface margins (px).",
                    vec![
                        field("top", false, scalar("Top margin.", "number")),
                        field("right", false, scalar("Right margin.", "number")),
                        field("bottom", false, scalar("Bottom margin.", "number")),
                        field("left", false, scalar("Left margin.", "number")),
                    ],
                ),
            ),
        ],
    )
}

fn root_config_root() -> Node {
    obj(
        "The workspace root module-graph config (`config/module.json`). Selects \
         which modules are enabled, the active providers, layout, and theme.",
        vec![
            field(
                "name",
                false,
                scalar("Config package name, e.g. `@mesh/local-config`.", "string"),
            ),
            field("version", false, scalar("Config version.", "string")),
            field(
                "private",
                false,
                scalar("Marks the config as never-published.", "boolean"),
            ),
            field(
                "mesh",
                true,
                obj(
                    "Root module-graph selection.",
                    vec![
                        field(
                            "schemaVersion",
                            true,
                            scalar("Graph schema version. Must be 1.", "number"),
                        ),
                        field(
                            "modulesDir",
                            false,
                            scalar("Relative path to the modules directory.", "path"),
                        ),
                        field(
                            "modules",
                            false,
                            Node {
                                doc: "Explicit installed module set, keyed by module id.",
                                type_hint: "object",
                                kind: Kind::Map(Box::new(obj(
                                    "An installed module entry.",
                                    vec![
                                        field(
                                            "kind",
                                            true,
                                            Node {
                                                doc: "Module kind.",
                                                type_hint: "enum",
                                                kind: Kind::Enum(MODULE_KINDS),
                                            },
                                        ),
                                        field(
                                            "path",
                                            true,
                                            scalar("Relative path to the module.", "path"),
                                        ),
                                        field(
                                            "enabled",
                                            false,
                                            scalar("Whether the module is enabled.", "boolean"),
                                        ),
                                    ],
                                ))),
                            },
                        ),
                        field(
                            "disabled",
                            false,
                            string_array(
                                "Module ids to keep disabled during auto-discovery.",
                                "Module id.",
                            ),
                        ),
                        field(
                            "providers",
                            false,
                            dependency_map("Interface name → selected provider module id."),
                        ),
                        field(
                            "layout",
                            false,
                            obj(
                                "Active layout selection.",
                                vec![field(
                                    "entrypoint",
                                    true,
                                    scalar(
                                        "`<module-id>:<entrypoint-id>` of the active layout.",
                                        "string",
                                    ),
                                )],
                            ),
                        ),
                        field(
                            "theme",
                            false,
                            obj(
                                "Active theme selection.",
                                vec![
                                    field("active", true, scalar("Active theme id.", "string")),
                                    field("mode", false, scalar("Active theme mode.", "string")),
                                ],
                            ),
                        ),
                    ],
                ),
            ),
        ],
    )
}
