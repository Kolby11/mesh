//! The single settings store.
//!
//! One JSON file holds every user decision, keyed by namespace
//! (`docs/spec/08-settings.md` §1). Defaults never live here — they come from
//! [`ShellSettings`]'s serde defaults and from module manifests / `<props>`
//! declarations, so a module updating its defaults still reaches users who
//! never overrode them.
//!
//! ```json
//! {
//!   "schemaVersion": 1,
//!   "shell": { "theme": { "active": "gruvbox-dark" } },
//!   "@mesh/navigation-bar": {
//!     "surface": { "anchor": "bottom" },
//!     "props": { "global": { "density": "compact" } }
//!   }
//! }
//! ```

use crate::validate::{SettingsDiagnostic, describe, unknown_key_diagnostic_from, validate_object};
use crate::{ConfigError, SHELL_SETTINGS_FIELDS, ShellSettings, mesh_home_path};
use serde_json::{Map as JsonMap, Value as JsonValue};
use std::path::{Path, PathBuf};

pub const SETTINGS_SCHEMA_VERSION: u64 = 1;

/// Namespace holding core shell preferences (theme, locale, icons, keyboard,
/// tooltip, sounds). Every other top-level key is a module id
/// (`@scope/name`, optionally `#instance`) or an interface id (`mesh.audio`).
pub const SHELL_NAMESPACE: &str = "shell";

const SCHEMA_VERSION_KEY: &str = "schemaVersion";

/// Every user-owned setting in the shell, loaded from one file.
#[derive(Debug, Clone)]
pub struct SettingsStore {
    path: PathBuf,
    root: JsonMap<String, JsonValue>,
    shell: ShellSettings,
    diagnostics: Vec<SettingsDiagnostic>,
}

impl Default for SettingsStore {
    fn default() -> Self {
        Self {
            path: default_settings_path(),
            root: JsonMap::new(),
            shell: ShellSettings::default(),
            diagnostics: Vec::new(),
        }
    }
}

impl SettingsStore {
    /// A missing file is not an error: it means the user changed nothing.
    pub fn load() -> Result<Self, ConfigError> {
        Self::load_from(&default_settings_path())
    }

    pub fn load_from(path: &Path) -> Result<Self, ConfigError> {
        let root = if path.exists() {
            let content = std::fs::read_to_string(path)?;
            match serde_json::from_str::<JsonValue>(&content)? {
                JsonValue::Object(map) => map,
                other => {
                    return Err(ConfigError::Validation(format!(
                        "{} must contain a JSON object, found {}",
                        path.display(),
                        json_type_name(&other)
                    )));
                }
            }
        } else {
            JsonMap::new()
        };

        let (shell, diagnostics) = resolve_shell_settings(&root);
        Ok(Self {
            path: path.to_path_buf(),
            root,
            shell,
            diagnostics,
        })
    }

    /// Build a store from an already-parsed document.
    pub fn from_value(path: impl Into<PathBuf>, value: JsonValue) -> Result<Self, ConfigError> {
        let root = match value {
            JsonValue::Object(map) => map,
            other => {
                return Err(ConfigError::Validation(format!(
                    "settings must be a JSON object, found {}",
                    json_type_name(&other)
                )));
            }
        };
        let (shell, diagnostics) = resolve_shell_settings(&root);
        Ok(Self {
            path: path.into(),
            root,
            shell,
            diagnostics,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Everything rejected while resolving the `"shell"` namespace and the
    /// file's top level. Module namespaces are validated by their own readers.
    pub fn diagnostics(&self) -> &[SettingsDiagnostic] {
        &self.diagnostics
    }

    /// Core shell preferences with declared defaults already applied.
    pub fn shell(&self) -> &ShellSettings {
        &self.shell
    }

    /// The raw stored overrides for one namespace, or `{}`. For an instance
    /// key (`@mesh/navigation-bar#top`) the bare module namespace is the base
    /// and the instance object layers over it.
    pub fn namespace(&self, name: &str) -> JsonValue {
        let mut resolved = match name.split_once('#') {
            Some((base, _)) => self.stored(base),
            None => JsonValue::Object(JsonMap::new()),
        };
        merge_json(&mut resolved, &self.stored(name));
        resolved
    }

    /// Whether anything is stored under `name` or, for an instance key, its
    /// base module.
    pub fn has_namespace(&self, name: &str) -> bool {
        if self.root.contains_key(name) {
            return true;
        }
        match name.split_once('#') {
            Some((base, _)) => self.root.contains_key(base),
            None => false,
        }
    }

    /// Namespaces with stored overrides, in file order.
    pub fn namespace_names(&self) -> impl Iterator<Item = &str> {
        self.root
            .keys()
            .map(String::as_str)
            .filter(|key| *key != SHELL_NAMESPACE && *key != SCHEMA_VERSION_KEY)
    }

    /// Replace one namespace's overrides; an empty object removes it so the
    /// store stays sparse. A value the schema rejects still lands in the file
    /// and surfaces in [`Self::diagnostics`], as a hand-edited one would.
    pub fn set_namespace(&mut self, name: &str, value: JsonValue) {
        let is_empty = value.as_object().is_some_and(JsonMap::is_empty);
        if value.is_null() || is_empty {
            self.root.remove(name);
        } else {
            self.root.insert(name.to_string(), value);
        }
        if name == SHELL_NAMESPACE {
            let (shell, diagnostics) = resolve_shell_settings(&self.root);
            self.shell = shell;
            self.diagnostics = diagnostics;
        }
    }

    /// Merge overrides into a namespace, keeping unrelated stored keys.
    pub fn merge_namespace(&mut self, name: &str, value: &JsonValue) {
        let mut current = self.stored(name);
        merge_json(&mut current, value);
        self.set_namespace(name, current);
    }

    /// Drop every override in a namespace; declared defaults win again.
    pub fn reset_namespace(&mut self, name: &str) {
        self.set_namespace(name, JsonValue::Null);
    }

    /// The full document as written to disk, in `BTreeMap` key order so saves
    /// diff cleanly regardless of which namespace was touched.
    pub fn to_value(&self) -> JsonValue {
        let mut root = self.root.clone();
        root.insert(
            SCHEMA_VERSION_KEY.to_string(),
            JsonValue::from(SETTINGS_SCHEMA_VERSION),
        );
        JsonValue::Object(root)
    }

    /// Written atomically: a crash mid-write cannot truncate the file.
    pub fn save(&self) -> Result<(), ConfigError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut content = serde_json::to_string_pretty(&self.to_value())?;
        content.push('\n');
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, content)?;
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }

    fn stored(&self, name: &str) -> JsonValue {
        self.root
            .get(name)
            .cloned()
            .unwrap_or_else(|| JsonValue::Object(JsonMap::new()))
    }
}

/// `MESH_SETTINGS_PATH` wins; otherwise a checked-out repo uses
/// `config/settings.json` so a dev shell stays out of the user's dotfiles.
pub fn default_settings_path() -> PathBuf {
    if let Ok(path) = std::env::var("MESH_SETTINGS_PATH") {
        if !path.trim().is_empty() {
            return PathBuf::from(path);
        }
    }

    let repo_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../..")
        .join("config/settings.json");
    if repo_path.exists() {
        return repo_path;
    }

    mesh_home_path().join("settings.json")
}

/// Load just the core shell preferences.
pub fn load_shell_settings() -> Result<ShellSettings, ConfigError> {
    Ok(SettingsStore::load()?.shell)
}

/// Objects merge key by key so setting one field does not erase its siblings.
/// Every other kind replaces wholesale: a stored array (a pack chain, a key
/// list) is a complete replacement by intent.
pub fn merge_json(base: &mut JsonValue, overlay: &JsonValue) {
    match (base, overlay) {
        (JsonValue::Object(base), JsonValue::Object(overlay)) => {
            for (key, value) in overlay {
                match base.get_mut(key) {
                    Some(existing) => merge_json(existing, value),
                    None => {
                        base.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        (base, overlay) => *base = overlay.clone(),
    }
}

/// Resolve the `"shell"` namespace, dropping and reporting what it cannot use.
///
/// Infallible on purpose: a hand-edited file must never stop the shell from
/// starting. Values are checked against [`SHELL_SETTINGS_FIELDS`] before the
/// merge, so serde only ever sees well-typed input.
fn resolve_shell_settings(
    root: &JsonMap<String, JsonValue>,
) -> (ShellSettings, Vec<SettingsDiagnostic>) {
    let mut diagnostics = validate_settings_root(root);

    let overrides = root.get(SHELL_NAMESPACE).map(|overrides| {
        validate_object(
            SHELL_NAMESPACE,
            "",
            SHELL_SETTINGS_FIELDS,
            overrides,
            &mut diagnostics,
        )
    });

    let mut resolved = match serde_json::to_value(ShellSettings::default()) {
        Ok(value) => value,
        Err(_) => return (ShellSettings::default(), diagnostics),
    };
    if let Some(overrides) = &overrides {
        merge_json(&mut resolved, overrides);
    }

    match serde_json::from_value(resolved) {
        Ok(settings) => (settings, diagnostics),
        Err(err) => {
            // Reaching here means the schema table and `ShellSettings` disagree.
            diagnostics.push(SettingsDiagnostic::error(
                SHELL_NAMESPACE,
                "",
                format!("could not be applied: {err}"),
                "report this as a MESH bug; the whole namespace fell back to its defaults",
            ));
            (ShellSettings::default(), diagnostics)
        }
    }
}

/// Check the schema stamp and that other keys look like ownable namespaces.
fn validate_settings_root(root: &JsonMap<String, JsonValue>) -> Vec<SettingsDiagnostic> {
    const ROOT_KEYS: &[&str] = &[SHELL_NAMESPACE, SCHEMA_VERSION_KEY];
    let mut diagnostics = Vec::new();

    for (key, value) in root {
        match key.as_str() {
            SCHEMA_VERSION_KEY => match value.as_u64() {
                Some(version) if version > SETTINGS_SCHEMA_VERSION => {
                    diagnostics.push(SettingsDiagnostic::warning(
                        key.clone(),
                        "",
                        format!(
                            "schema version {version} is newer than this build understands \
                             ({SETTINGS_SCHEMA_VERSION})"
                        ),
                        "some values may be ignored; update MESH or check the file by hand",
                    ));
                }
                Some(_) => {}
                None => diagnostics.push(SettingsDiagnostic::error(
                    key.clone(),
                    "",
                    format!("expected a non-negative integer, found {}", describe(value)),
                    format!("set it to {SETTINGS_SCHEMA_VERSION}"),
                )),
            },
            SHELL_NAMESPACE => {}
            other if is_namespace_id(other) => {}
            other => diagnostics.push(unknown_key_diagnostic_from("", "", other, ROOT_KEYS)),
        }
    }

    diagnostics
}

/// Shaped like `@scope/name[#instance]` or a dotted interface id. Whether the
/// owner exists is `config doctor`'s question; this only avoids mistaking a
/// real namespace for a typo of `shell`.
fn is_namespace_id(key: &str) -> bool {
    let base = key.split('#').next().unwrap_or(key);
    (base.starts_with('@') && base.contains('/')) || base.contains('.')
}

fn json_type_name(value: &JsonValue) -> &'static str {
    match value {
        JsonValue::Null => "null",
        JsonValue::Bool(_) => "a boolean",
        JsonValue::Number(_) => "a number",
        JsonValue::String(_) => "a string",
        JsonValue::Array(_) => "an array",
        JsonValue::Object(_) => "an object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn store(value: JsonValue) -> SettingsStore {
        SettingsStore::from_value("/tmp/mesh-test-settings.json", value).expect("valid store")
    }

    #[test]
    fn missing_shell_namespace_yields_declared_defaults() {
        let store = store(json!({}));
        assert_eq!(store.shell().theme.active, "tokyo-night");
        assert_eq!(store.shell().tooltip.delay_ms, 300);
        assert_eq!(
            store.shell().keyboard.button_activation_keys,
            vec!["Enter".to_string(), "Space".to_string()]
        );
    }

    #[test]
    fn shell_overrides_are_sparse_and_leave_siblings_alone() {
        let store = store(json!({
            "shell": { "tooltip": { "delay_ms": 25 } }
        }));

        assert_eq!(store.shell().tooltip.delay_ms, 25);
        assert_eq!(store.shell().tooltip.position, "bottom");
        assert_eq!(store.shell().tooltip.gap, 6.0);
        assert_eq!(store.shell().theme.active, "tokyo-night");
    }

    #[test]
    fn module_namespace_returns_stored_overrides() {
        let store = store(json!({
            "@mesh/navigation-bar": { "surface": { "anchor": "bottom" } }
        }));

        assert_eq!(
            store.namespace("@mesh/navigation-bar"),
            json!({ "surface": { "anchor": "bottom" } })
        );
        assert_eq!(store.namespace("@mesh/quick-settings"), json!({}));
    }

    #[test]
    fn instance_namespace_layers_over_its_base_module() {
        let store = store(json!({
            "@mesh/navigation-bar": {
                "surface": { "anchor": "top", "layer": "top" }
            },
            "@mesh/navigation-bar#bottom": {
                "surface": { "anchor": "bottom" }
            }
        }));

        let resolved = store.namespace("@mesh/navigation-bar#bottom");
        assert_eq!(resolved["surface"]["anchor"], json!("bottom"));
        assert_eq!(resolved["surface"]["layer"], json!("top"));
    }

    #[test]
    fn setting_an_empty_namespace_removes_it() {
        let mut store = store(json!({ "@mesh/navigation-bar": { "surface": {} } }));
        store.set_namespace("@mesh/navigation-bar", json!({}));

        assert!(!store.has_namespace("@mesh/navigation-bar"));
        assert_eq!(store.namespace_names().count(), 0);
    }

    #[test]
    fn merge_namespace_keeps_unrelated_stored_keys() {
        let mut store = store(json!({
            "@mesh/navigation-bar": {
                "surface": { "anchor": "top" },
                "props": { "global": { "density": "compact" } }
            }
        }));

        store.merge_namespace(
            "@mesh/navigation-bar",
            &json!({ "surface": { "layer": "overlay" } }),
        );

        let resolved = store.namespace("@mesh/navigation-bar");
        assert_eq!(resolved["surface"]["anchor"], json!("top"));
        assert_eq!(resolved["surface"]["layer"], json!("overlay"));
        assert_eq!(resolved["props"]["global"]["density"], json!("compact"));
    }

    #[test]
    fn setting_the_shell_namespace_reresolves_shell_settings() {
        let mut store = store(json!({}));
        store.set_namespace("shell", json!({ "theme": { "active": "gruvbox-dark" } }));

        assert_eq!(store.shell().theme.active, "gruvbox-dark");
    }

    #[test]
    fn reset_namespace_restores_declared_defaults() {
        let mut store = store(json!({ "shell": { "tooltip": { "delay_ms": 25 } } }));
        store.reset_namespace("shell");

        assert_eq!(store.shell().tooltip.delay_ms, 300);
    }

    #[test]
    fn written_documents_stamp_the_schema_version_and_order_deterministically() {
        let store = store(json!({
            "@mesh/quick-settings": { "surface": { "anchor": "top" } },
            "shell": { "theme": { "active": "gruvbox-dark" } },
            "@mesh/navigation-bar": { "surface": { "anchor": "bottom" } }
        }));

        let document = store.to_value();
        let keys: Vec<&str> = document
            .as_object()
            .expect("object")
            .keys()
            .map(String::as_str)
            .collect();

        assert_eq!(
            keys,
            vec![
                "@mesh/navigation-bar",
                "@mesh/quick-settings",
                "schemaVersion",
                "shell"
            ]
        );
        assert_eq!(document["schemaVersion"], json!(SETTINGS_SCHEMA_VERSION));
    }

    #[test]
    fn a_missing_file_loads_as_an_empty_store() {
        let path = std::env::temp_dir().join(format!(
            "mesh-settings-absent-{}-{}.json",
            std::process::id(),
            line!()
        ));
        let store = SettingsStore::load_from(&path).expect("absent file is not an error");

        assert_eq!(store.shell().theme.active, "tokyo-night");
        assert_eq!(store.namespace_names().count(), 0);
    }

    #[test]
    fn save_then_load_round_trips() {
        let path = std::env::temp_dir().join(format!(
            "mesh-settings-roundtrip-{}-{}.json",
            std::process::id(),
            line!()
        ));
        let mut written = SettingsStore::from_value(&path, json!({})).unwrap();
        written.set_namespace("shell", json!({ "i18n": { "locale": "sk" } }));
        written.set_namespace(
            "@mesh/navigation-bar",
            json!({ "surface": { "exclusive_zone": 48 } }),
        );
        written.save().expect("write settings");

        let loaded = SettingsStore::load_from(&path).expect("read settings");
        std::fs::remove_file(&path).ok();

        assert_eq!(loaded.shell().i18n.locale, "sk");
        assert_eq!(loaded.shell().i18n.fallback_locale, "en");
        assert_eq!(
            loaded.namespace("@mesh/navigation-bar")["surface"]["exclusive_zone"],
            json!(48)
        );
    }

    #[test]
    fn a_non_object_document_is_a_diagnostic_not_a_panic() {
        let path = std::env::temp_dir().join(format!(
            "mesh-settings-invalid-{}-{}.json",
            std::process::id(),
            line!()
        ));
        std::fs::write(&path, "[]").unwrap();
        let result = SettingsStore::load_from(&path);
        std::fs::remove_file(&path).ok();

        assert!(matches!(result, Err(ConfigError::Validation(_))));
    }

    fn only(diagnostics: &[SettingsDiagnostic]) -> &SettingsDiagnostic {
        assert_eq!(
            diagnostics.len(),
            1,
            "expected one diagnostic: {diagnostics:#?}"
        );
        &diagnostics[0]
    }

    #[test]
    fn the_repository_settings_file_is_clean() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../..")
            .join("config/settings.json");
        let store = SettingsStore::load_from(&path).expect("load repo settings");
        assert_eq!(store.diagnostics(), &[], "repo settings.json must validate");
    }

    #[test]
    fn a_wrong_type_is_reported_and_keeps_the_default() {
        let store = store(json!({ "shell": { "tooltip": { "delay_ms": "300" } } }));

        let diagnostic = only(store.diagnostics());
        assert!(diagnostic.is_error());
        assert_eq!(diagnostic.namespace, "shell");
        assert_eq!(diagnostic.key_path, "tooltip.delay_ms");
        assert!(
            diagnostic.message.contains("non-negative integer")
                && diagnostic.message.contains("the string \"300\""),
            "message should name both the expectation and the value: {}",
            diagnostic.message
        );
        assert_eq!(store.shell().tooltip.delay_ms, 300);
    }

    #[test]
    fn a_bad_enum_value_lists_the_accepted_ones() {
        let store = store(json!({ "shell": { "tooltip": { "position": "botom" } } }));

        let diagnostic = only(store.diagnostics());
        assert!(diagnostic.is_error());
        assert_eq!(diagnostic.key_path, "tooltip.position");
        assert!(
            diagnostic.suggested_action.contains("bottom"),
            "suggestion should quote the vocabulary: {}",
            diagnostic.suggested_action
        );
        assert_eq!(store.shell().tooltip.position, "bottom");
    }

    #[test]
    fn a_tooltip_position_is_trimmed_to_its_canonical_value() {
        let store = store(json!({
            "shell": { "tooltip": { "position": " bottom " } }
        }));

        assert!(store.diagnostics().is_empty(), "{:#?}", store.diagnostics());
        assert_eq!(store.shell().tooltip.position, "bottom");
    }

    #[test]
    fn an_out_of_range_value_falls_back_without_discarding_valid_siblings() {
        let store = store(json!({
            "shell": {
                "render": { "blur": { "passes": 256 } },
                "tooltip": { "delay_ms": 25 },
                "theme": { "active": "gruvbox-dark" }
            }
        }));

        let diagnostic = only(store.diagnostics());
        assert_eq!(diagnostic.key_path, "render.blur.passes");
        assert!(diagnostic.message.contains("1 through 3"));
        assert_eq!(store.shell().render.blur.passes, 1);
        assert_eq!(store.shell().tooltip.delay_ms, 25);
        assert_eq!(store.shell().theme.active, "gruvbox-dark");
    }

    #[test]
    fn a_negative_blur_radius_falls_back_to_the_declared_default() {
        let store = store(json!({
            "shell": { "render": { "blur": { "max_radius": -1 } } }
        }));

        let diagnostic = only(store.diagnostics());
        assert_eq!(diagnostic.key_path, "render.blur.max_radius");
        assert_eq!(store.shell().render.blur.max_radius, 96.0);
    }

    #[test]
    fn an_unknown_key_near_a_known_one_suggests_it() {
        let store = store(json!({ "shell": { "tooltip": { "delay_msec": 25 } } }));

        let diagnostic = only(store.diagnostics());
        assert!(diagnostic.is_error(), "a typo is an error, not a shrug");
        assert_eq!(diagnostic.key_path, "tooltip.delay_msec");
        assert_eq!(diagnostic.suggested_action, "did you mean \"delay_ms\"?");
    }

    #[test]
    fn an_unknown_key_with_no_near_match_is_a_warning_listing_known_keys() {
        let store = store(json!({ "shell": { "tooltip": { "sparkles": true } } }));

        let diagnostic = only(store.diagnostics());
        assert!(!diagnostic.is_error());
        assert!(
            diagnostic.suggested_action.contains("delay_ms"),
            "unrecognized keys should still teach the key set: {}",
            diagnostic.suggested_action
        );
    }

    #[test]
    fn an_unknown_shell_section_is_reported_without_losing_its_siblings() {
        let store = store(json!({
            "shell": {
                "fonts": { "packs": ["@mesh/fonts-default"] },
                "theme": { "active": "gruvbox-dark" }
            }
        }));

        let diagnostic = only(store.diagnostics());
        assert_eq!(diagnostic.key_path, "fonts.packs");
        assert_eq!(store.shell().theme.active, "gruvbox-dark");
    }

    #[test]
    fn a_misspelled_namespace_is_caught_but_module_ids_are_left_alone() {
        let store = store(json!({
            "shel": { "theme": { "active": "gruvbox-dark" } },
            "@mesh/navigation-bar": { "surface": { "anchor": "bottom" } },
            "mesh.audio": { "props": { "global": { "poll": 1 } } }
        }));

        let diagnostic = only(store.diagnostics());
        assert_eq!(diagnostic.key_path, "shel");
        assert_eq!(diagnostic.suggested_action, "did you mean \"shell\"?");
    }

    #[test]
    fn free_form_shortcut_maps_validate_only_their_leaves() {
        let store = store(json!({
            "shell": {
                "keyboard": {
                    "surface_shortcuts": {
                        "@mesh/navigation-bar": {
                            "mute": { "key": "u" },
                            "raise": { "key": 7 }
                        }
                    }
                }
            }
        }));

        let diagnostic = only(store.diagnostics());
        assert_eq!(
            diagnostic.key_path,
            "keyboard.surface_shortcuts.@mesh/navigation-bar.raise.key"
        );
        let shortcuts = &store.shell().keyboard.surface_shortcuts["@mesh/navigation-bar"];
        assert_eq!(shortcuts["mute"].key.as_deref(), Some("u"));
        assert!(shortcuts["raise"].key.is_none());
    }

    #[test]
    fn a_non_object_shell_namespace_is_not_fatal() {
        let store = store(json!({ "shell": 5 }));

        let diagnostic = only(store.diagnostics());
        assert!(diagnostic.is_error());
        assert_eq!(store.shell().theme.active, "tokyo-night");
    }

    #[test]
    fn a_newer_schema_version_warns_rather_than_refusing_to_load() {
        let store = store(json!({ "schemaVersion": SETTINGS_SCHEMA_VERSION + 1 }));

        let diagnostic = only(store.diagnostics());
        assert!(!diagnostic.is_error());
        assert_eq!(diagnostic.namespace, "schemaVersion");
    }

    #[test]
    fn writing_an_invalid_value_reports_it_instead_of_failing() {
        let mut store = store(json!({}));
        store.set_namespace("shell", json!({ "theme": { "active": 7 } }));

        assert_eq!(only(store.diagnostics()).key_path, "theme.active");
        assert_eq!(store.shell().theme.active, "tokyo-night");
    }

    #[test]
    fn new_diagnostics_only_reports_what_changed() {
        let before = store(json!({
            "shell": { "tooltip": { "delay_ms": "300", "gap": "wide" } }
        }));
        let after = store(json!({
            "shell": { "tooltip": { "delay_ms": 300, "gap": "wide" } }
        }));

        assert_eq!(before.diagnostics().len(), 2);
        assert!(
            crate::validate::new_settings_diagnostics(before.diagnostics(), after.diagnostics())
                .is_empty()
        );
    }

    #[test]
    fn merge_json_replaces_arrays_wholesale() {
        let mut base = json!({ "icons": { "packs": ["a", "b"] } });
        merge_json(&mut base, &json!({ "icons": { "packs": ["c"] } }));

        assert_eq!(base, json!({ "icons": { "packs": ["c"] } }));
    }
}
