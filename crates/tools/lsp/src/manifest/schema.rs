//! A small, hand-authored description of the canonical `module.json` schema.
//!
//! This mirrors the runtime structs in `mesh_core_module` (`ModuleManifest` /
//! `MeshModuleSection` for per-module manifests and `RootModuleGraphManifest`
//! for the workspace `config/module.json`). It is the single source of truth for
//! manifest key completion, hover documentation, and unknown-key / enum
//! diagnostics. When the runtime schema changes, update this tree to match.

use crate::json::schema::{Kind, Node, field, obj, scalar};

/// Build a node from this file's `&'static str` literals.
fn node(doc: &'static str, type_hint: &'static str, kind: Kind) -> Node {
    Node {
        doc: doc.to_string(),
        type_hint: type_hint.to_string(),
        kind,
    }
}

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

/// Resolve the schema root for a flavor.
pub fn root(flavor: ManifestFlavor) -> Node {
    match flavor {
        ManifestFlavor::Module => module_root(),
        ManifestFlavor::RootConfig => root_config_root(),
    }
}

fn localized_text(doc: &'static str) -> Node {
    // A localized string is either a bare string or `{ t, fallback }`, so it is
    // modelled as a permissive scalar to accept both shapes without flagging.
    scalar(doc, "string | { t, fallback }")
}

fn dependency_map(doc: &'static str) -> Node {
    node(
        doc,
        "object<string, version-spec>",
        Kind::Map(Box::new(scalar(
            "Semver requirement (e.g. \">=1.0\", \"^0.1.0\").",
            "version-spec",
        ))),
    )
}

fn string_array(doc: &'static str, element_doc: &'static str) -> Node {
    node(
        doc,
        "array<string>",
        Kind::Array(Box::new(scalar(element_doc, "string"))),
    )
}

fn capabilities_array(doc: &'static str) -> Node {
    node(
        doc,
        "array<capability>",
        Kind::Array(Box::new(node(
            "A capability string in `<domain>.<action>` form. Capabilities are \
                  extensible, so unknown values are allowed.",
            "capability",
            Kind::Suggest(KNOWN_CAPABILITIES),
        ))),
    )
}

fn binary_dependency(doc: &'static str) -> Node {
    node(
        doc,
        "array",
        Kind::Array(Box::new(obj(
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
                    node(
                        "Distro → package name providing this binary.",
                        "object<string, string>",
                        Kind::Map(Box::new(scalar("Package name.", "string"))),
                    ),
                ),
            ],
        ))),
    )
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
                node(
                    "The module role. Determines how the core loads and wires the module.",
                    "enum",
                    Kind::Enum(MODULE_KINDS),
                ),
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
                node(
                    "Declarative keybind metadata, keyed by action id.",
                    "object",
                    Kind::Map(Box::new(keybind_node())),
                ),
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
                node(
                    "Interfaces this backend module implements.",
                    "array",
                    Kind::Array(Box::new(implements_node())),
                ),
            ),
            field("interface", false, interface_node()),
            field(
                "interfaces",
                false,
                node(
                    "Inline interface contract declarations on a backend module — \
                          the low-friction contract carrier for single-provider domains. \
                          Multi-provider domains keep a standalone interface module.",
                    "array",
                    Kind::Array(Box::new(interface_node())),
                ),
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
                node(
                    "Unvalidated experimental fields. Anything here is passed through untouched.",
                    "any",
                    Kind::Scalar,
                ),
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
                node(
                    "Layout entrypoints this module contributes.",
                    "array",
                    Kind::Array(Box::new(obj(
                        "A layout contribution.",
                        vec![
                            field("id", true, scalar("Layout id.", "string")),
                            field("entrypoint", true, scalar("Entrypoint path.", "path")),
                            field("label", false, localized_text("Display label.")),
                        ],
                    ))),
                ),
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
                            node(
                                "JSON-schema-like settings definition.",
                                "object",
                                Kind::Scalar,
                            ),
                        ),
                    ],
                ),
            ),
            field(
                "themes",
                false,
                node(
                    "Theme contributions.",
                    "array",
                    Kind::Array(Box::new(obj(
                        "A theme contribution.",
                        vec![
                            field("id", true, scalar("Theme id.", "string")),
                            field("label", false, localized_text("Display label.")),
                            field("defaultMode", false, scalar("Default mode id.", "string")),
                            field("modes", false, dependency_map("Mode id → token-set path.")),
                        ],
                    ))),
                ),
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
                node(
                    "Localization bundle contributions.",
                    "array",
                    Kind::Array(Box::new(obj(
                        "An i18n contribution.",
                        vec![
                            field("id", true, scalar("Bundle id.", "string")),
                            field("locale", true, scalar("Locale code, e.g. `en`.", "string")),
                            field("path", true, scalar("Path to the locale bundle.", "path")),
                        ],
                    ))),
                ),
            ),
            field(
                "libraries",
                false,
                node(
                    "Luau library contributions.",
                    "array",
                    Kind::Array(Box::new(obj(
                        "A library contribution.",
                        vec![
                            field("namespace", true, scalar("Importable namespace.", "string")),
                            field("path", true, scalar("Path to the library source.", "path")),
                        ],
                    ))),
                ),
            ),
        ],
    )
}

fn path_contribution_array(doc: &'static str) -> Node {
    node(
        doc,
        "array",
        Kind::Array(Box::new(obj(
            "A path contribution.",
            vec![
                field("id", true, scalar("Resource id.", "string")),
                field("path", true, scalar("Path to the resource.", "path")),
                field("label", false, localized_text("Display label.")),
            ],
        ))),
    )
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
                node(
                    "Relationship to the extended interface.",
                    "enum",
                    Kind::Enum(&["base", "extension", "independent"]),
                ),
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
                node(
                    "Public state fields every provider must emit; read through \
                          the service proxy as plain field access.",
                    "array",
                    Kind::Array(Box::new(typed_field())),
                ),
            ),
            field(
                "methods",
                false,
                node(
                    "Mutating command methods callable from frontend scripts.",
                    "array",
                    Kind::Array(Box::new(obj(
                        "A command method declaration.",
                        vec![
                            field("name", true, scalar("Command name.", "string")),
                            field(
                                "args",
                                false,
                                node(
                                    "Typed command arguments.",
                                    "array",
                                    Kind::Array(Box::new(typed_field())),
                                ),
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
                                "stateBinding",
                                false,
                                obj(
                                    "Reactive command-to-state binding applied by the shell. \
                                     Declare exactly one of `fromArg` or `toggle: true`.",
                                    vec![
                                        field(
                                            "field",
                                            true,
                                            scalar(
                                                "Shared service-state field to update.",
                                                "string",
                                            ),
                                        ),
                                        field(
                                            "fromArg",
                                            false,
                                            scalar(
                                                "Command argument supplying the bound value.",
                                                "string",
                                            ),
                                        ),
                                        field(
                                            "toggle",
                                            false,
                                            scalar(
                                                "Negate the current boolean state field.",
                                                "boolean",
                                            ),
                                        ),
                                    ],
                                ),
                            ),
                        ],
                    ))),
                ),
            ),
            field(
                "events",
                false,
                node(
                    "Named events with typed payload fields.",
                    "array",
                    Kind::Array(Box::new(obj(
                        "An event declaration.",
                        vec![
                            field("name", true, scalar("Event name.", "string")),
                            field(
                                "payload",
                                false,
                                node(
                                    "Typed payload fields.",
                                    "array",
                                    Kind::Array(Box::new(typed_field())),
                                ),
                            ),
                        ],
                    ))),
                ),
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
        "Surface placement for a frontend module. A `layer` surface is shell \
         chrome placed by anchor/layer/exclusive_zone/margins; a `window` \
         surface is an xdg_toplevel placed by the compositor and configured by \
         title/appId/resizable/decorations. Fields belonging to the other role \
         are rejected as graph diagnostics, unless `promotable` says the surface \
         holds both roles over its life. Surface sizing and the show/hide \
         transition are CSS concerns on the component root, not manifest fields.",
        vec![
            field(
                "role",
                false,
                node(
                    "Which compositor protocol realizes the surface: shell \
                          chrome (`layer`, the default) or an ordinary \
                          application window (`window`).",
                    "enum",
                    Kind::Enum(&["layer", "window"]),
                ),
            ),
            field(
                "promotable",
                false,
                scalar(
                    "Whether the surface may be moved between roles while \
                     running — popped out into a window and docked back. Such a \
                     surface may declare both roles' fields, and `role` says \
                     which it starts as. Runtime role changes are refused for \
                     surfaces that do not set this.",
                    "boolean",
                ),
            ),
            field(
                "title",
                false,
                scalar(
                    "Window title (role `window`). Localizable: a string, or \
                     `{ \"t\": key, \"fallback\": text }`. Defaults to the module id.",
                    "string",
                ),
            ),
            field(
                "appId",
                false,
                scalar(
                    "`xdg_toplevel` app id (role `window`) — what compositor \
                     window rules match on. Defaults to the module id.",
                    "string",
                ),
            ),
            field(
                "resizable",
                false,
                scalar(
                    "Whether the user may resize the window (role `window`). \
                     False pins it to its CSS-measured size.",
                    "boolean",
                ),
            ),
            field(
                "decorations",
                false,
                node(
                    "Who draws the window's title bar (role `window`). \
                          MESH paints its own chrome, so `client` is the default.",
                    "enum",
                    Kind::Enum(&["client", "server"]),
                ),
            ),
            field(
                "anchor",
                false,
                node(
                    "Screen edge the surface anchors to.",
                    "enum",
                    Kind::Enum(&["top", "bottom", "left", "right"]),
                ),
            ),
            field(
                "layer",
                false,
                node(
                    "Layer-shell stacking layer.",
                    "enum",
                    Kind::Enum(&["background", "bottom", "top", "overlay"]),
                ),
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
                node(
                    "Keyboard interactivity mode.",
                    "enum",
                    Kind::Enum(&["none", "on_demand", "exclusive"]),
                ),
            ),
            field(
                "blur",
                false,
                scalar(
                    "Request compositor background blur. MESH gives the surface a \
                     `:blur` namespace suffix a single compositor rule can target \
                     (Hyprland: `layerrule = blur, :blur$`).",
                    "boolean",
                ),
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
                            node(
                                "Explicit installed module set, keyed by module id.",
                                "object",
                                Kind::Map(Box::new(obj(
                                    "An installed module entry.",
                                    vec![
                                        field(
                                            "kind",
                                            true,
                                            node("Module kind.", "enum", Kind::Enum(MODULE_KINDS)),
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
                            ),
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
