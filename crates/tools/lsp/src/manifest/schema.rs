//! Editor descriptions reconciled with the core manifest field declarations.
//!
//! Core serde declarations supply MESH keys and kinds (`ModuleManifest` /
//! `MeshModuleSection` for per-module manifests and `RootModuleGraphManifest`
//! for the workspace `config/module.json`). It supplies key completion, hover
//! documentation, and structural editor hints. Parse and semantic validation
//! is performed by the runtime contracts in `manifest::diagnostics`, so this
//! tree must not become a second runtime validation authority.

use crate::json::schema::{ContractSyntax, Kind, Node, enumeration, field, obj, scalar};

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

/// Resolve the schema root for a flavor.
pub fn root(flavor: ManifestFlavor) -> Node {
    let mut root = match flavor {
        ManifestFlavor::Module => module_root(),
        ManifestFlavor::RootConfig => root_config_root(),
    };
    reconcile_core_fields(
        &mut root,
        flavor == ManifestFlavor::RootConfig,
        &mut Vec::new(),
    );
    root
}

fn reconcile_core_fields(node: &mut Node, root_graph: bool, path: &mut Vec<String>) {
    let parts = path.iter().map(String::as_str).collect::<Vec<_>>();
    if let Some(names) = mesh_core_module::package::manifest_field_names(root_graph, &parts) {
        if !matches!(node.kind, Kind::Object(_)) {
            node.kind = Kind::Object(Vec::new());
            node.type_hint = "object".into();
        }
        if let Kind::Object(fields) = &mut node.kind {
            if !path.is_empty() {
                fields.retain(|field| names.contains(&field.name.as_str()));
            }
            for name in names {
                if !fields.iter().any(|field| field.name == name) {
                    fields.push(field(
                        name,
                        false,
                        scalar("Declared by the core manifest contract.", "value"),
                    ));
                }
            }
        }
    }
    match &mut node.kind {
        Kind::Object(fields) | Kind::OpenObject { fields, .. } => {
            for field in fields {
                path.push(field.name.clone());
                reconcile_core_fields(&mut field.node, root_graph, path);
                path.pop();
            }
        }
        Kind::Array(element) | Kind::Map(element) => {
            path.push("*".into());
            reconcile_core_fields(element, root_graph, path);
            path.pop();
        }
        _ => {}
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
            Kind::SuggestDiscovered(
                mesh_core_capability::BUILTIN_CAPABILITIES
                    .iter()
                    .map(|(id, _)| (*id).to_string())
                    .collect(),
            ),
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
                    Kind::Enum(mesh_core_module::package::module_kind_names()),
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
                    vec![field(
                        "main",
                        false,
                        scalar("Primary entrypoint path.", "path"),
                    )],
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
            field(
                "extensionPoints",
                false,
                node(
                    "UI extension point contracts declared by this interface module. \
                     Keyed by dotted contract name (`mesh.settings.page`), never by \
                     module id, so a host can be replaced without breaking contributors.",
                    "object",
                    Kind::Map(Box::new(extension_point_declaration_node())),
                ),
            ),
            field(
                "hosts",
                false,
                node(
                    "Extension points this module renders contributions into. Hosting \
                     is explicit because a host renders another module's UI inside its \
                     own surface.",
                    "object",
                    Kind::Map(Box::new(obj(
                        "How this host accepts and lays out contributions.",
                        vec![
                            field(
                                "version",
                                false,
                                scalar("Accepted contract version range.", "string"),
                            ),
                            field(
                                "layout",
                                false,
                                enumeration(
                                    "How contributions are laid out.",
                                    &["row", "column", "stack"],
                                ),
                            ),
                            field(
                                "max",
                                false,
                                scalar("Cap on rendered contributions.", "integer"),
                            ),
                            field(
                                "slots",
                                false,
                                node(
                                    "Author-declared customizable slots, keyed by the stable local slot name used in markup.",
                                    "object",
                                    Kind::Map(Box::new(obj(
                                        "Defaults for one customizable slot.",
                                        vec![field(
                                            "defaults",
                                            false,
                                            string_array(
                                                "Ordered default contribution references (`module-id:contribution-id`).",
                                                "Contribution reference.",
                                            ),
                                        )],
                                    ))),
                                ),
                            ),
                        ],
                    ))),
                ),
            ),
            field("compose", false, compose_node()),
            field(
                "extends",
                false,
                scalar(
                    "Composition modules only: the composition this one refines.",
                    "string",
                ),
            ),
            field("dependencies", false, mesh_dependencies()),
            field(
                "provides",
                false,
                mesh_contributes(
                    "General graph contributions; legacy theme, icon, and i18n entries normalize into `contributes`.",
                ),
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
                    "Styling hooks of a ui module or library token family: `tokens` (dotted \
                     names with defaults built from semantic contract roles), `defaults`, and \
                     `independent: true` for a module that ignores the active theme.",
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

fn extension_point_declaration_node() -> Node {
    obj(
        "One UI extension point contract.",
        vec![
            field("version", true, scalar("Contract version.", "string")),
            field(
                "description",
                false,
                scalar("What this point is for.", "string"),
            ),
            field(
                "multiple",
                false,
                scalar(
                    "Whether every contribution renders (default true) or only the \
                     highest-precedence one.",
                    "boolean",
                ),
            ),
            field(
                "props",
                false,
                node(
                    "Props the host passes to each contribution, keyed by name.",
                    "object<string, type | object>",
                    Kind::Map(Box::new(contract_type(
                        "The prop's type expression, or an object with `type` and \
                         `description`.",
                        vec![
                            field("type", true, contract_type(TYPE_EXPRESSION_DOC, Vec::new())),
                            field("description", false, scalar("What it carries.", "string")),
                        ],
                    ))),
                ),
            ),
        ],
    )
}

fn compose_node() -> Node {
    obj(
        "Composition modules only: what this composition selects. A composition \
         binds providers and composes roots; it never owns them.",
        vec![
            field(
                "removeBackgroundServices",
                false,
                node(
                    "Inherited background services to remove.",
                    "array",
                    Kind::Array(Box::new(scalar("Module id.", "string"))),
                ),
            ),
            field(
                "removeProviders",
                false,
                node(
                    "Inherited provider bindings to remove.",
                    "array",
                    Kind::Array(Box::new(scalar("Interface name.", "string"))),
                ),
            ),
            field(
                "roots",
                false,
                node(
                    "Root component instances, keyed `module-id#instance-id`.",
                    "object",
                    Kind::Map(Box::new(obj(
                        "A root instance.",
                        vec![
                            field("module", true, scalar("Module id.", "string")),
                            field("entrypoint", false, scalar("Entrypoint id.", "string")),
                            field(
                                "active",
                                false,
                                scalar("Whether it is composed.", "boolean"),
                            ),
                            field(
                                "surface",
                                false,
                                node(
                                    "Sparse placement override; omitted fields inherit \
                                     the module's `mesh.surface`.",
                                    "object",
                                    Kind::Map(Box::new(scalar("Placement value.", "any"))),
                                ),
                            ),
                        ],
                    ))),
                ),
            ),
            field(
                "backgroundServices",
                false,
                node(
                    "Backend modules started with this composition.",
                    "array",
                    Kind::Array(Box::new(scalar("Module id.", "string"))),
                ),
            ),
            field(
                "providers",
                false,
                node(
                    "Explicit provider bindings, interface name to module id. Needed \
                     only where several modules implement one interface.",
                    "object",
                    Kind::Map(Box::new(scalar("Provider module id.", "string"))),
                ),
            ),
            field(
                "resources",
                false,
                obj(
                    "Resource selections. Ordered chains replace rather than merge.",
                    vec![
                        field(
                            "theme",
                            false,
                            scalar(
                                "Recommended theme; applies only while the user has not chosen one.",
                                "string",
                            ),
                        ),
                        field(
                            "icons",
                            false,
                            node(
                                "Ordered icon pack chain.",
                                "array",
                                Kind::Array(Box::new(scalar("Module id.", "string"))),
                            ),
                        ),
                        field(
                            "fonts",
                            false,
                            node(
                                "Ordered font pack chain.",
                                "array",
                                Kind::Array(Box::new(scalar("Module id.", "string"))),
                            ),
                        ),
                        field(
                            "languages",
                            false,
                            node(
                                "Ordered language pack chain.",
                                "array",
                                Kind::Array(Box::new(scalar("Module id.", "string"))),
                            ),
                        ),
                    ],
                ),
            ),
            field(
                "slots",
                false,
                node(
                    "Per-extension-point overrides: how this composition rearranges \
                     the UI its members contribute, without editing any member.",
                    "object",
                    Kind::Map(Box::new(obj(
                        "Overrides for one extension point.",
                        vec![
                            field(
                                "replace",
                                false,
                                node(
                                    "Contributing module id to replacement module id.",
                                    "object",
                                    Kind::Map(Box::new(scalar("Replacement module id.", "string"))),
                                ),
                            ),
                            field(
                                "suppress",
                                false,
                                node(
                                    "Contributing modules whose contributions are hidden.",
                                    "array",
                                    Kind::Array(Box::new(scalar("Module id.", "string"))),
                                ),
                            ),
                            field(
                                "order",
                                false,
                                node(
                                    "Explicit render order by contributing module id.",
                                    "array",
                                    Kind::Array(Box::new(scalar("Module id.", "string"))),
                                ),
                            ),
                        ],
                    ))),
                ),
            ),
            field(
                "nodeSlots",
                false,
                node(
                    "Sparse visual-placement overrides, keyed by root instance and then by author-declared slot name. Each list replaces the inherited list wholesale.",
                    "object",
                    Kind::Map(Box::new(node(
                        "Customizable slots on one root instance.",
                        "object",
                        Kind::Map(Box::new(obj(
                            "One ordered customizable-slot override.",
                            vec![field(
                                "nodes",
                                true,
                                node(
                                    "Ordered placement nodes. An empty list intentionally empties the slot.",
                                    "array",
                                    Kind::Array(Box::new(obj(
                                        "A stable public component placement.",
                                        vec![
                                            field(
                                                "id",
                                                true,
                                                scalar("Stable placement id.", "string"),
                                            ),
                                            field(
                                                "use",
                                                true,
                                                scalar(
                                                    "Public contribution reference (`module-id:contribution-id`).",
                                                    "string",
                                                ),
                                            ),
                                            field(
                                                "props",
                                                false,
                                                node(
                                                    "Literal overrides for public component props.",
                                                    "object",
                                                    Kind::Map(Box::new(scalar(
                                                        "Literal prop value.",
                                                        "any",
                                                    ))),
                                                ),
                                            ),
                                        ],
                                    ))),
                                ),
                            )],
                        ))),
                    ))),
                ),
            ),
            field(
                "settings",
                false,
                node(
                    "Composition-scoped sparse settings, by namespace.",
                    "object",
                    Kind::Map(Box::new(scalar("Namespace object.", "object"))),
                ),
            ),
        ],
    )
}

fn mesh_contributes(doc: &'static str) -> Node {
    obj(
        doc,
        vec![
            field(
                "extensionPoints",
                false,
                node(
                    "Contributions to extension points hosted by other modules, keyed \
                     by the point's contract name. A contribution names the point, \
                     never the host.",
                    "object",
                    Kind::Map(Box::new(node(
                        "Contributions to one extension point.",
                        "array",
                        Kind::Array(Box::new(obj(
                            "One contribution.",
                            vec![
                                field(
                                    "id",
                                    true,
                                    scalar("Contribution id, unique per module.", "string"),
                                ),
                                field(
                                    "entry",
                                    true,
                                    scalar(
                                        "Module-relative `.mesh` component, compiled as an \
                                         alternate root of this module.",
                                        "path",
                                    ),
                                ),
                                field(
                                    "order",
                                    false,
                                    scalar("Render order within the point.", "integer"),
                                ),
                                field(
                                    "props",
                                    false,
                                    node(
                                        "Props passed to the contribution, typechecked \
                                         against the point's declaration.",
                                        "object",
                                        Kind::Map(Box::new(scalar("Prop value.", "any"))),
                                    ),
                                ),
                            ],
                        ))),
                    ))),
                ),
            ),
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
                            field(
                                "tokenContract",
                                false,
                                scalar(
                                    "Semantic token contract version every mode implements, e.g. `1.0`.",
                                    "string",
                                ),
                            ),
                            field("default_mode", false, scalar("Default mode id.", "string")),
                            field("modes", false, dependency_map("Mode id → token-set path.")),
                            field(
                                "mode_metadata",
                                false,
                                node(
                                    "Rendering metadata for each theme mode.",
                                    "object<string, mode-metadata>",
                                    Kind::Map(Box::new(obj(
                                        "Theme mode rendering metadata.",
                                        vec![
                                            field(
                                                "color_scheme",
                                                false,
                                                scalar("Light or dark color scheme.", "string"),
                                            ),
                                            field(
                                                "contrast",
                                                false,
                                                scalar("Contrast preference.", "string"),
                                            ),
                                        ],
                                    ))),
                                ),
                            ),
                            field(
                                "recommends",
                                false,
                                obj(
                                    "Resources adopted when this theme is selected; the first installed entry of each list wins.",
                                    vec![
                                        field(
                                            "fonts",
                                            false,
                                            string_array(
                                                "Preferred UI font families, most preferred first.",
                                                "Exact fontconfig family name.",
                                            ),
                                        ),
                                        field(
                                            "icons",
                                            false,
                                            string_array(
                                                "Preferred icon themes, most preferred first.",
                                                "XDG icon-theme id or icon-pack id.",
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
                "icons",
                false,
                icon_contribution_array("Icon contributions."),
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
            field(
                "keybinds",
                false,
                node(
                    "Declarative keybind contributions, keyed by action id.",
                    "object",
                    Kind::Map(Box::new(keybind_node())),
                ),
            ),
        ],
    )
}

fn icon_contribution_array(doc: &'static str) -> Node {
    node(
        doc,
        "array",
        Kind::Array(Box::new(obj(
            "An icon-pack or path-based icon contribution.",
            vec![
                field("id", true, scalar("Icon contribution id.", "string")),
                field(
                    "path",
                    false,
                    scalar("Path to a legacy icon resource.", "path"),
                ),
                field("label", false, localized_text("Display label.")),
                field("kind", false, scalar("Icon asset source kind.", "string")),
                field(
                    "covers",
                    false,
                    scalar("Vocabulary coverage constraints.", "object"),
                ),
                field(
                    "requires",
                    false,
                    scalar("Required system assets.", "object"),
                ),
                field("axes", false, scalar("Variable icon axes.", "object")),
                field(
                    "mappings",
                    false,
                    scalar("Logical icon mappings.", "object"),
                ),
                field(
                    "vocabularies",
                    false,
                    scalar("Namespaced icon mappings.", "object"),
                ),
            ],
        ))),
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
                "scope",
                false,
                scalar(
                    "Where the binding is active: `surface` (default), `focus` (only while focus is inside the subscribing element) or `access`.",
                    "string",
                ),
            ),
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
                            scalar(
                                "Trigger kind: `shortcut`, `access_key` or `auto` (the shell assigns a key from `shell.keyboard.key_priority`).",
                                "string",
                            ),
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
            field(
                "localizedTriggers",
                false,
                node(
                    "Locale-specific access-key defaults.",
                    "object<string, trigger>",
                    Kind::Map(Box::new(obj(
                        "A localized keybind trigger.",
                        vec![
                            field("kind", false, scalar("Trigger kind.", "string")),
                            field("key", false, scalar("Key.", "string")),
                            field(
                                "modifiers",
                                false,
                                string_array("Modifier keys.", "Modifier."),
                            ),
                        ],
                    ))),
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
            field(
                "features",
                false,
                string_array(
                    "Optional contract features this provider supplies.",
                    "Feature name declared by the contract.",
                ),
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

fn contract_type(doc: &'static str, fields: Vec<crate::json::schema::Field>) -> Node {
    node(
        doc,
        if fields.is_empty() {
            "type"
        } else {
            "type | object"
        },
        Kind::ContractText {
            syntax: ContractSyntax::Type,
            fields,
        },
    )
}

const TYPE_EXPRESSION_DOC: &str = "Type expression: `string`, `int`, `float`, `boolean`, \
     `blob`, `object`, `any`, `Result`, or a named type from `types`, with optional `[]` \
     (array) and `?` (optional) suffixes.";

fn contract_node() -> Node {
    let field_map = |doc: &'static str| {
        node(
            doc,
            "object<string, type>",
            Kind::Map(Box::new(contract_type(TYPE_EXPRESSION_DOC, Vec::new()))),
        )
    };
    let state_field = contract_type(
        "A state field's type expression, or an object with `type` and optional \
         `description`, `minimum`, `maximum` and `default`.",
        vec![
            field("type", true, contract_type(TYPE_EXPRESSION_DOC, Vec::new())),
            field("description", false, scalar("Field description.", "string")),
            field(
                "minimum",
                false,
                scalar(
                    "Lowest value of an int or float field; providers are clamped to it.",
                    "number",
                ),
            ),
            field(
                "maximum",
                false,
                scalar(
                    "Highest value of an int or float field; providers are clamped to it.",
                    "number",
                ),
            ),
            field(
                "default",
                false,
                scalar("Value used when a provider omits the field.", "any"),
            ),
        ],
    );
    let signature = || {
        node(
            "Method signature: `(name: type, …) -> Type`. Arguments are in call order; \
             omit `-> Type` for a method without a declared return.",
            "signature",
            Kind::ContractText {
                syntax: ContractSyntax::Signature,
                fields: Vec::new(),
            },
        )
    };
    let method = node(
        "A signature string such as `(id: int) -> Result`, or an object with \
         `signature` and optional `coalesce` and `stateBinding`.",
        "signature | object",
        Kind::ContractText {
            syntax: ContractSyntax::Signature,
            fields: vec![
                field("signature", true, signature()),
                field(
                    "coalesce",
                    false,
                    scalar(
                        "Replace queued calls with the newest: `true`, or \
                         `{ \"key\": [args] }` to coalesce per argument values \
                         (idempotent setters only).",
                        "boolean | { key }",
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
                                scalar("Shared service-state field to update.", "string"),
                            ),
                            field(
                                "fromArg",
                                false,
                                scalar("Command argument supplying the bound value.", "string"),
                            ),
                            field(
                                "toggle",
                                false,
                                scalar("Negate the current boolean state field.", "boolean"),
                            ),
                        ],
                    ),
                ),
            ],
        },
    );
    obj(
        "Inline interface contract JSON: state fields, command methods, events, \
         named types, and consumer capabilities.",
        vec![
            field(
                "state",
                false,
                node(
                    "Public state fields every provider must emit, keyed by name; read \
                     through the service proxy as plain field access.",
                    "object<string, type | object>",
                    Kind::Map(Box::new(state_field)),
                ),
            ),
            field(
                "methods",
                false,
                node(
                    "Command methods callable from frontend scripts, keyed by name.",
                    "object<string, signature | object>",
                    Kind::Map(Box::new(method)),
                ),
            ),
            field(
                "events",
                false,
                node(
                    "Events keyed by name; each maps payload field names to types.",
                    "object<string, object<string, type>>",
                    Kind::Map(Box::new(field_map("Payload fields of this event."))),
                ),
            ),
            field(
                "types",
                false,
                node(
                    "Named record types keyed by PascalCase name; each maps field names \
                     to types.",
                    "object<string, object<string, type>>",
                    Kind::Map(Box::new(field_map("Fields of this record type."))),
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
                        field(
                            "read",
                            false,
                            string_array(
                                "Capabilities required for reading interface state.",
                                "Capability id.",
                            ),
                        ),
                        field(
                            "events",
                            false,
                            node(
                                "Capabilities required for named interface events.",
                                "object<string, array<capability>>",
                                Kind::Map(Box::new(string_array(
                                    "Capabilities required by this event.",
                                    "Capability id.",
                                ))),
                            ),
                        ),
                        field(
                            "methods",
                            false,
                            node(
                                "Capabilities required for named interface methods.",
                                "object<string, array<capability>>",
                                Kind::Map(Box::new(string_array(
                                    "Capabilities required by this method.",
                                    "Capability id.",
                                ))),
                            ),
                        ),
                        field(
                            "permissions",
                            false,
                            node(
                                "Service-operation capabilities this interface declares, \
                                 keyed by capability id, with the privilege level \
                                 reviewed at install.",
                                "object<string, privilege>",
                                Kind::Map(Box::new(node(
                                    "Privilege level of this service permission.",
                                    "enum",
                                    Kind::Enum(mesh_core_capability::PrivilegeLevel::NAMES),
                                ))),
                            ),
                        ),
                    ],
                ),
            ),
            field(
                "features",
                false,
                node(
                    "Optional features, keyed by lowercase dashed name. A provider supplies \
                     a feature's members only when its `implements` entry lists the feature; \
                     consumers check `service:supports(name)`.",
                    "object<string, feature>",
                    Kind::Map(Box::new(obj(
                        "One optional contract feature.",
                        vec![
                            field("description", false, scalar("What the feature adds.", "string")),
                            field(
                                "members",
                                true,
                                string_array(
                                    "State fields (optional types), methods and events the feature gates.",
                                    "Member name.",
                                ),
                            ),
                        ],
                    ))),
                ),
            ),
        ],
    )
}

fn surface_layout_node() -> Node {
    obj(
        "Surface placement for a frontend module. A `layer` surface is shell \
         chrome placed by anchor/layer; a `window` \
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
                                            node(
                                                "Module kind.",
                                                "enum",
                                                Kind::Enum(
                                                    mesh_core_module::package::module_kind_names(),
                                                ),
                                            ),
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
                            "capabilityApprovals",
                            false,
                            node(
                                "Explicit capability approvals keyed by module id.",
                                "object<module-id, array<capability>>",
                                Kind::Map(Box::new(string_array(
                                    "Capabilities approved for the module.",
                                    "Capability id.",
                                ))),
                            ),
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

#[cfg(test)]
mod core_schema_tests {
    use super::*;

    #[test]
    fn canonical_mesh_keys_and_capabilities_come_from_core() {
        let schema = root(ManifestFlavor::Module);
        let Kind::Object(fields) = schema.kind else {
            panic!("module object");
        };
        let mesh = fields
            .into_iter()
            .find(|field| field.name == "mesh")
            .unwrap();
        let Kind::Object(fields) = mesh.node.kind else {
            panic!("mesh object");
        };
        let mut actual = fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>();
        let mut expected =
            mesh_core_module::package::manifest_field_names(false, &["mesh"]).unwrap();
        actual.sort_unstable();
        expected.sort_unstable();
        assert_eq!(actual, expected);
        assert!(actual.contains(&"fonts") && actual.contains(&"font_pack"));
        assert!(!actual.contains(&"surfaceLayout") && !actual.contains(&"compatibility"));
        let Kind::Array(capability) = capabilities_array("Capabilities").kind else {
            panic!("array");
        };
        let Kind::SuggestDiscovered(capabilities) = capability.kind else {
            panic!("suggestions");
        };
        assert!(capabilities.iter().any(|id| id == "service.wm.read"));
        assert!(!capabilities.iter().any(|id| id == "service.hyprland.read"));
        for capability in capabilities {
            assert!(
                mesh_core_capability::CapabilityCatalog::builtin()
                    .validate(&capability)
                    .is_ok()
            );
        }
    }
}
