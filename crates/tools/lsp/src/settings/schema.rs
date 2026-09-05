//! The settings document's schema, derived from the runtime's own tables.
//!
//! `mesh_core_config::SHELL_SETTINGS_FIELDS` defines what the `"shell"`
//! namespace accepts and `mesh_core_surface_config::MODULE_NAMESPACE_FIELDS`
//! defines what a module namespace accepts. Restating either here would let the
//! editor and the shell disagree about the same file, so this module translates
//! them into [`crate::json::schema`] nodes instead. What it adds on top is
//! prose — a [`FieldKind`] has no documentation — and the values discovered on
//! this machine (themes, locales, installed packs) that a static table cannot
//! know.

use mesh_core_config::validate::{FieldKind, FieldSpec};
use mesh_core_config::{SHELL_NAMESPACE, SHELL_SETTINGS_FIELDS};
use mesh_core_surface_config::MODULE_NAMESPACE_FIELDS;

use crate::json::schema::{
    Field, Node, array, discovered, enumeration, field, map, obj, open_obj, scalar,
};
use crate::module_registry::ModuleRegistry;

/// Values that exist on this machine rather than in a schema.
struct Discovered {
    themes: Vec<String>,
    locales: Vec<String>,
    icon_packs: Vec<String>,
}

impl Discovered {
    fn from_registry(registry: &ModuleRegistry) -> Self {
        Self {
            themes: registry.themes.clone(),
            locales: registry.locales.clone(),
            icon_packs: registry.resource_snapshot.icon_pack_ids().to_vec(),
        }
    }
}

/// The settings document schema, for the modules and resources `registry` found.
pub fn root(registry: &ModuleRegistry) -> Node {
    let found = Discovered::from_registry(registry);

    let mut fields = vec![
        field(
            "schemaVersion",
            false,
            scalar("On-disk shape of this file. Currently `1`.", "number"),
        ),
        field(SHELL_NAMESPACE, false, shell_namespace(&found)),
    ];

    // Every installed module and interface is a namespace the user may
    // configure. Listing them makes the namespaces completable; unlisted keys
    // are still valid (a module may be installed elsewhere, and an uninstalled
    // module's namespace is kept, not deleted — spec 08 §7).
    for id in namespace_ids(registry) {
        let doc = registry
            .module_summary(&id)
            .unwrap_or_else(|| interface_namespace_doc(&id));
        fields.push(field(id, false, module_namespace(&found, doc)));
    }

    open_obj(
        "MESH settings: one sparse JSON document holding **only the values you \
         changed**. Defaults come from module declarations, never from this \
         file, so a module that changes its own default still reaches you.\n\n\
         Top-level keys are namespaces: `shell` for core preferences, \
         `mesh.<interface>` for props shared across providers, and \
         `@scope/name` for one module.",
        fields,
        module_namespace(
            &found,
            "A namespace for one module (`@scope/name`) or interface \
             (`mesh.audio`). No module by this id was found in the workspace — \
             it may be installed elsewhere, or left over from one that was \
             removed. Its overrides are kept either way."
                .to_string(),
        ),
    )
}

/// Namespace keys worth listing: installed module ids and interface ids.
fn namespace_ids(registry: &ModuleRegistry) -> Vec<String> {
    let mut ids = registry.module_ids();
    ids.extend(registry.interface_ids());
    ids.sort();
    ids.dedup();
    ids
}

fn interface_namespace_doc(id: &str) -> String {
    format!(
        "Props shared by every provider of the `{id}` interface. Values here \
         survive swapping the provider module."
    )
}

fn shell_namespace(found: &Discovered) -> Node {
    let doc = "Core shell preferences: theme, locale, icon packs, keyboard, \
               reduced motion, tooltip, sounds, and render quality.";
    node_from_fields(doc, SHELL_SETTINGS_FIELDS, SHELL_NAMESPACE, found)
}

fn module_namespace(found: &Discovered, doc: String) -> Node {
    // The prefix `<module>` marks paths inside a module namespace, whose own
    // key is the module id and so cannot be part of a static documentation path.
    node_from_fields(doc, MODULE_NAMESPACE_FIELDS, "<module>", found)
}

/// Translate a runtime field table into a schema object node. `path` is the
/// dotted path of the containing object, used to look up documentation and the
/// discovered-value overrides.
fn node_from_fields(
    doc: impl Into<String>,
    fields: &'static [FieldSpec],
    path: &str,
    found: &Discovered,
) -> Node {
    obj(
        doc,
        fields
            .iter()
            .map(|spec| {
                let child_path = format!("{path}.{}", spec.key);
                Field {
                    name: spec.key.to_string(),
                    required: false,
                    node: node_from_kind(&spec.kind, &child_path, found),
                }
            })
            .collect(),
    )
}

/// Translate one runtime field kind into a schema node.
fn node_from_kind(kind: &'static FieldKind, path: &str, found: &Discovered) -> Node {
    if let Some(node) = discovered_values(path, found) {
        return node;
    }

    let doc = doc_for(path);
    match kind {
        FieldKind::Str => scalar(doc, "string"),
        FieldKind::Locale => scalar(doc, "BCP 47 locale tag"),
        FieldKind::Bool => scalar(doc, "boolean"),
        FieldKind::UInt => scalar(doc, "integer ≥ 0"),
        FieldKind::UIntRange { min, max } => scalar(doc, format!("integer {min}–{max}")),
        FieldKind::FloatRange { min, max } => scalar(
            doc,
            max.map_or_else(
                || format!("number ≥ {min}"),
                |max| format!("number {min}–{max}"),
            ),
        ),
        FieldKind::Float => scalar(doc, "number"),
        FieldKind::StrArray => array(doc, "array<string>", scalar(doc, "string")),
        FieldKind::LocalizedText => scalar(doc, "string or localized text object"),
        FieldKind::Enum { values, .. } => enumeration(doc, values),
        FieldKind::Section(fields) => node_from_fields(doc, fields, path, found),
        FieldKind::Map(inner) => map(
            doc,
            "object",
            node_from_kind(inner, &format!("{path}.*"), found),
        ),
        // Props: their declaration is the component's `<props>` block, which
        // the LSP does not read yet, so anything goes rather than everything
        // being flagged. See `docs/spec/03-components.md`.
        FieldKind::Opaque => map(doc, "object", scalar("A declared prop value.", "any")),
        FieldKind::ThemeModePolicy => map(doc, "object", scalar("Theme mode policy field.", "any")),
        FieldKind::Token => scalar(doc, "string, number, or boolean"),
    }
}

/// The machine-discovered vocabulary for a path, when it has one.
///
/// These are suggestions, never constraints: a theme, pack, or locale can come
/// from outside the workspace the LSP scanned, and refusing to accept it would
/// be worse than not offering it.
fn discovered_values(path: &str, found: &Discovered) -> Option<Node> {
    let node = match path {
        "shell.theme.active" => discovered(
            format!(
                "{}\n\nThemes found on this machine are listed below; a theme \
                 module id (`@scope/name`) also works.",
                doc_for(path)
            ),
            found.themes.clone(),
        ),
        "shell.i18n.locale" | "shell.i18n.fallback_locale" => discovered(
            format!(
                "{}\n\nLocales with a catalog somewhere in the module graph are \
                 listed below.",
                doc_for(path)
            ),
            found.locales.clone(),
        ),
        "shell.icons.default_pack" => discovered(doc_for(path), found.icon_packs.clone()),
        "<module>.icons.use_packs" => array(
            doc_for(path),
            "array<module-id>",
            discovered(
                "An icon-pack module id. Earlier entries win.",
                found.icon_packs.clone(),
            ),
        ),
        _ => return None,
    };
    Some(node)
}

/// Documentation for a settings path. Sourced from `docs/spec/08-settings.md`
/// and the runtime structs the values feed.
fn doc_for(path: &str) -> &'static str {
    match path {
        // shell.theme
        "shell.theme" => "Theme selection. Winner-takes-all: exactly one theme is active.",
        "shell.theme.active" => "Id of the active theme.",

        // shell.i18n
        "shell.i18n" => "Locale selection for shell and module text.",
        "shell.i18n.policy" => {
            "Locale policy: `manual` keeps the selected locale; `follow_system` resolves the host locale."
        }
        "shell.i18n.locale" => "Active locale, e.g. `en` or `sk-SK`.",
        "shell.i18n.fallback_locale" => {
            "Locale used for keys the active locale has no translation for."
        }

        // shell.sounds
        "shell.sounds" => "Paths to sound files played on shell events.",
        "shell.sounds.startup" => "Played when the shell starts.",
        "shell.sounds.shutdown" => "Played when the shell exits.",
        "shell.sounds.device_connected" => "Played when a device is connected.",
        "shell.sounds.device_disconnected" => "Played when a device is disconnected.",
        "shell.sounds.error" => "Played on an error notification.",
        "shell.sounds.notification" => "Played on a notification.",

        // shell.keyboard
        "shell.keyboard" => "Keyboard activation keys and per-surface shortcuts (spec 10).",
        "shell.keyboard.button_activation_keys" => {
            "Keys that activate a focused `button`, e.g. `[\"Return\", \"space\"]`. \
             Replaces the default list wholesale."
        }
        "shell.keyboard.toggle_activation_keys" => "Keys that flip a focused toggle.",
        "shell.keyboard.slider_decrement_keys" => "Keys that decrease a focused slider.",
        "shell.keyboard.slider_increment_keys" => "Keys that increase a focused slider.",
        "shell.keyboard.surface_shortcuts" => {
            "Per-module keybind overrides: module id → action name → `{ \"key\": … }`."
        }

        // shell.icons
        "shell.icons" => "Shell-wide icon configuration (spec 05).",
        "shell.icons.default_pack" => {
            "Icon-pack module id prepended to every frontend's pack chain, unless \
             the frontend sets `icons.ignore_shell_default`."
        }

        // shell.motion
        "shell.motion" => "Accessibility preferences for visual motion.",
        "shell.motion.reduced" => {
            "Clamp non-essential transitions, keyframes, scrolling, inertia, tooltips, and surface motion to an immediate state."
        }

        // shell.render
        "shell.render" => "Render quality dials.",
        "shell.render.blur" => {
            "Blur quality. Element `filter: blur()` costs scale with the area \
             covered rather than the element count, so it gets a user dial."
        }
        "shell.render.blur.passes" => {
            "Blur passes per filtered layer (1–3, default 1). Each pass runs at a \
             reduced sigma keeping total blur constant: more passes buy a smoother \
             falloff, not a wider one, at roughly proportional cost (~2.8x for two). \
             Out-of-range values clamp."
        }
        "shell.render.blur.max_radius" => {
            "Radii above this (default 96) are dropped with a painter diagnostic \
             instead of rasterized, bounding the worst frame a stylesheet can ask for."
        }

        // shell.tooltip
        "shell.tooltip" => "Tooltip timing and placement defaults.",
        "shell.tooltip.position" => {
            "Default tooltip placement. `auto` picks a side from the space \
             available; `cursor` follows the pointer."
        }
        "shell.tooltip.delay_ms" => "Delay before a tooltip appears, in milliseconds.",
        "shell.tooltip.gap" => "Gap in logical pixels between the tooltip and its anchor.",
        "shell.tooltip.cursor_offset_x" => {
            "Horizontal offset from the cursor, for `position: cursor`."
        }
        "shell.tooltip.cursor_offset_y" => {
            "Vertical offset from the cursor, for `position: cursor`."
        }

        // module namespace
        "<module>.surface" => {
            "Surface placement overrides for this module, layered over its \
             manifest `mesh.surface`. `mesh-shell config eject <module-id>` \
             writes the effective placement here."
        }
        "<module>.surface.role" => "`layer` for a shell surface, `window` for an ordinary window.",
        "<module>.surface.title" => "Window title (`role: window`).",
        "<module>.surface.app_id" => "Wayland app id (`role: window`).",
        "<module>.surface.resizable" => "Whether the window can be resized (`role: window`).",
        "<module>.surface.decorations" => "Who draws the window decorations (`role: window`).",
        "<module>.surface.anchor" => {
            "Screen edge the layer surface is anchored to (`role: layer`)."
        }
        "<module>.surface.layer" => "Layer-shell layer the surface sits on (`role: layer`).",
        "<module>.surface.keyboard_mode" => "How the surface takes keyboard focus.",
        "<module>.surface.visible_on_start" => {
            "Whether the surface is mapped when the shell starts."
        }
        "<module>.surface.blur" => {
            "Ask the compositor to blur behind this surface. Appends `:blur` to the \
             layer-shell namespace so one compositor rule can match every opted-in surface."
        }
        "<module>.props" => "Prop overrides for this module.",
        "<module>.props.global" => "Values applied to every instance of the module.",
        "<module>.props.instances" => {
            "Per-instance values, keyed by composition instance key \
             (`@mesh/navigation-bar#top/import:audio`). More specific than `global`."
        }
        "<module>.icons" => "This module's icon resolution (spec 05).",
        "<module>.icons.use_packs" => "Ordered icon-pack chain for this module; earlier packs win.",
        "<module>.icons.overrides" => "Per-name icon overrides: semantic name → concrete icon.",
        "<module>.icons.ignore_shell_default" => {
            "Drop `shell.icons.default_pack` from this module's chain."
        }
        _ => "",
    }
}
