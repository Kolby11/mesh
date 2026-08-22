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

use crate::validate::{
    SettingsDiagnostic, describe, unknown_key_diagnostic_from, validate_json_schema,
    validate_object,
};
use crate::{ConfigError, SHELL_SETTINGS_FIELDS, ShellSettings, mesh_home_path};
use serde_json::{Map as JsonMap, Value as JsonValue};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub const SETTINGS_SCHEMA_VERSION: u64 = 1;

/// Namespace holding core shell preferences (theme, locale, icons, keyboard,
/// tooltip, sounds). Every other top-level key is a module id
/// (`@scope/name`, optionally `#instance`) or an interface id (`mesh.audio`).
pub const SHELL_NAMESPACE: &str = "shell";

const SCHEMA_VERSION_KEY: &str = "schemaVersion";
const REVISION_KEY: &str = "revision";
static TEMP_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// A schema registered by the owner of one settings namespace.
#[derive(Debug, Clone, PartialEq)]
pub struct SettingsNamespaceSchema {
    pub namespace: String,
    pub owner: String,
    pub schema: JsonValue,
}

impl SettingsNamespaceSchema {
    pub fn new(
        namespace: impl Into<String>,
        owner: impl Into<String>,
        schema: JsonValue,
    ) -> Result<Self, SettingsSchemaError> {
        let candidate = Self {
            namespace: namespace.into(),
            owner: owner.into(),
            schema,
        };
        candidate.validate()?;
        Ok(candidate)
    }

    fn validate(&self) -> Result<(), SettingsSchemaError> {
        if !is_namespace_id(&self.namespace) || self.namespace.contains('#') {
            return Err(SettingsSchemaError::InvalidNamespace {
                namespace: self.namespace.clone(),
            });
        }
        if self.owner.trim().is_empty() {
            return Err(SettingsSchemaError::EmptyOwner {
                namespace: self.namespace.clone(),
            });
        }
        if self.owner != self.namespace {
            return Err(SettingsSchemaError::OwnerMismatch {
                namespace: self.namespace.clone(),
                owner: self.owner.clone(),
            });
        }
        if self
            .schema
            .get("type")
            .and_then(JsonValue::as_str)
            .is_some_and(|kind| kind != "object")
        {
            return Err(SettingsSchemaError::InvalidSchema {
                path: "type".into(),
                message: "a namespace schema must describe an object".into(),
            });
        }
        validate_schema_definition(&self.schema, "")
    }

    fn normalized(mut self) -> Self {
        // Manifest contributions historically used a bare field map while
        // component props use an explicit object schema.
        if self
            .schema
            .as_object()
            .is_some_and(|schema| !schema.contains_key("type"))
        {
            self.schema = serde_json::json!({
                "type": "object",
                "properties": self.schema,
            });
        }
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SettingsSchemaError {
    #[error("settings namespace '{namespace}' is invalid")]
    InvalidNamespace { namespace: String },
    #[error("settings namespace '{namespace}' has no owner")]
    EmptyOwner { namespace: String },
    #[error("owner '{owner}' cannot register settings namespace '{namespace}'")]
    OwnerMismatch { namespace: String, owner: String },
    #[error("settings namespace '{namespace}' has more than one owner")]
    DuplicateNamespace { namespace: String },
    #[error("invalid settings schema at '{path}': {message}")]
    InvalidSchema { path: String, message: String },
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SettingsSchemaRegistry {
    schemas: BTreeMap<String, SettingsNamespaceSchema>,
}

impl SettingsSchemaRegistry {
    pub fn get(&self, namespace: &str) -> Option<&SettingsNamespaceSchema> {
        self.schemas.get(namespace)
    }

    pub fn iter(&self) -> impl Iterator<Item = &SettingsNamespaceSchema> {
        self.schemas.values()
    }

    fn register(&mut self, schema: SettingsNamespaceSchema) -> Result<(), SettingsSchemaError> {
        let schema = schema.normalized();
        schema.validate()?;
        if self.schemas.contains_key(&schema.namespace) {
            return Err(SettingsSchemaError::DuplicateNamespace {
                namespace: schema.namespace,
            });
        }
        self.schemas.insert(schema.namespace.clone(), schema);
        Ok(())
    }
}

/// Every user-owned setting in the shell, loaded from one file.
#[derive(Debug, Clone)]
pub struct SettingsStore {
    path: PathBuf,
    root: JsonMap<String, JsonValue>,
    revision: u64,
    schemas: SettingsSchemaRegistry,
    validated_root: JsonMap<String, JsonValue>,
    shell: ShellSettings,
    document_diagnostics: Vec<SettingsDiagnostic>,
    diagnostics: Vec<SettingsDiagnostic>,
}

impl Default for SettingsStore {
    fn default() -> Self {
        Self {
            path: default_settings_path(),
            root: JsonMap::new(),
            revision: 0,
            schemas: SettingsSchemaRegistry::default(),
            validated_root: JsonMap::new(),
            shell: ShellSettings::default(),
            document_diagnostics: Vec::new(),
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
        let (root, diagnostics) = if path.exists() {
            let content = std::fs::read_to_string(path)?;
            match serde_json::from_str::<JsonValue>(&content)? {
                JsonValue::Object(map) => (map, Vec::new()),
                other => (
                    JsonMap::new(),
                    vec![non_object_document_diagnostic(
                        &format!("{} must contain", path.display()),
                        &other,
                    )],
                ),
            }
        } else {
            (JsonMap::new(), Vec::new())
        };

        let revision = document_revision(&root);
        let mut store = Self {
            path: path.to_path_buf(),
            root,
            revision,
            schemas: SettingsSchemaRegistry::default(),
            validated_root: JsonMap::new(),
            shell: ShellSettings::default(),
            document_diagnostics: diagnostics,
            diagnostics: Vec::new(),
        };
        store.rebuild_validation();
        Ok(store)
    }

    /// Build a store from an already-parsed document.
    pub fn from_value(path: impl Into<PathBuf>, value: JsonValue) -> Result<Self, ConfigError> {
        let path = path.into();
        let (root, diagnostics) = match value {
            JsonValue::Object(map) => (map, Vec::new()),
            other => (
                JsonMap::new(),
                vec![non_object_document_diagnostic("settings must be", &other)],
            ),
        };
        let revision = document_revision(&root);
        let mut store = Self {
            path,
            root,
            revision,
            schemas: SettingsSchemaRegistry::default(),
            validated_root: JsonMap::new(),
            shell: ShellSettings::default(),
            document_diagnostics: diagnostics,
            diagnostics: Vec::new(),
        };
        store.rebuild_validation();
        Ok(store)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Monotonic revision of the durable settings document. A missing or
    /// legacy document starts at revision zero.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Everything rejected while resolving the shell namespace, file top
    /// level, and owner-registered namespaces.
    pub fn diagnostics(&self) -> &[SettingsDiagnostic] {
        &self.diagnostics
    }

    /// Core shell preferences with declared defaults already applied.
    pub fn shell(&self) -> &ShellSettings {
        &self.shell
    }

    /// Runtime-facing overrides for one namespace, or `{}`. Invalid values in
    /// a registered schema are omitted; the raw document remains available
    /// through [`Self::to_value`]. For an instance key
    /// (`@mesh/navigation-bar#top`) the bare module namespace is the base and
    /// the instance object layers over it.
    pub fn namespace(&self, name: &str) -> JsonValue {
        let mut resolved = match name.split_once('#') {
            Some((base, _)) => self.resolved_stored(base),
            None => JsonValue::Object(JsonMap::new()),
        };
        merge_json(&mut resolved, &self.resolved_stored(name));
        resolved
    }

    /// Owner-registered schemas currently used to validate module settings.
    pub fn schema_registry(&self) -> &SettingsSchemaRegistry {
        &self.schemas
    }

    /// Register a batch of namespace schemas as one state transition.
    ///
    /// Validation and duplicate-owner checks happen against a staged
    /// registry. A failure leaves the existing registry and runtime-facing
    /// settings projection untouched.
    pub fn register_namespace_schemas_transactionally<I>(
        &mut self,
        schemas: I,
    ) -> Result<(), SettingsSchemaError>
    where
        I: IntoIterator<Item = SettingsNamespaceSchema>,
    {
        let mut candidate = self.schemas.clone();
        for schema in schemas {
            candidate.register(schema)?;
        }
        self.commit_schema_registry(candidate);
        Ok(())
    }

    /// Replace the complete owner snapshot atomically. This is used when a
    /// candidate installed graph is prepared or committed.
    pub fn replace_namespace_schemas_transactionally<I>(
        &mut self,
        schemas: I,
    ) -> Result<(), SettingsSchemaError>
    where
        I: IntoIterator<Item = SettingsNamespaceSchema>,
    {
        let mut candidate = SettingsSchemaRegistry::default();
        for schema in schemas {
            candidate.register(schema)?;
        }
        self.commit_schema_registry(candidate);
        Ok(())
    }

    fn commit_schema_registry(&mut self, candidate: SettingsSchemaRegistry) {
        let mut staged = self.clone();
        staged.schemas = candidate;
        staged.rebuild_validation();
        self.schemas = staged.schemas;
        self.validated_root = staged.validated_root;
        self.shell = staged.shell;
        self.diagnostics = staged.diagnostics;
    }

    pub fn register_namespace_schema(
        &mut self,
        schema: SettingsNamespaceSchema,
    ) -> Result<(), SettingsSchemaError> {
        self.register_namespace_schemas_transactionally([schema])
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
    /// and surfaces in diagnostics, while readers receive only the validated
    /// projection.
    pub fn set_namespace(&mut self, name: &str, value: JsonValue) {
        let is_empty = value.as_object().is_some_and(JsonMap::is_empty);
        if value.is_null() || is_empty {
            self.root.remove(name);
        } else {
            self.root.insert(name.to_string(), value);
        }
        self.rebuild_validation();
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
        if self.revision > 0 {
            root.insert(REVISION_KEY.to_string(), JsonValue::from(self.revision));
        }
        JsonValue::Object(root)
    }

    /// Write the store through a unique owner-only temporary file, syncing the
    /// file before the atomic rename and the parent directory afterwards.
    ///
    /// A failure before the rename leaves the previous settings file intact.
    pub fn save(&self) -> Result<(), ConfigError> {
        let parent = settings_parent(&self.path);
        ensure_settings_directory(parent)?;
        let mut content = serde_json::to_string_pretty(&self.to_value())?;
        content.push('\n');
        let (temporary, mut file) = create_settings_temporary(&self.path, parent)?;
        let result = (|| {
            let file_result = (|| {
                file.write_all(content.as_bytes())?;
                file.sync_all()
            })();
            drop(file);
            file_result?;
            fs::rename(&temporary, &self.path)?;
            sync_settings_directory(parent)
        })();
        if let Err(error) = result {
            let _ = fs::remove_file(&temporary);
            return Err(error.into());
        }
        Ok(())
    }

    /// Persist this candidate only if the file still has `expected_revision`.
    ///
    /// Callers prepare and validate a complete candidate first, then use this
    /// method as the durable commit boundary. The in-memory store advances to
    /// the committed revision only after the atomic replace succeeds.
    pub fn save_if_revision(&mut self, expected_revision: u64) -> Result<(), ConfigError> {
        self.check_revision(expected_revision)?;

        let next_revision = expected_revision.saturating_add(1);
        let mut committed = self.clone();
        committed.revision = next_revision;
        committed.save()?;
        self.revision = next_revision;
        Ok(())
    }

    /// Check both the candidate and its backing document without writing it.
    /// This lets a profile-scoped transaction ensure that shared fallback
    /// settings did not change while its profile candidate was prepared.
    pub fn check_revision(&self, expected_revision: u64) -> Result<(), ConfigError> {
        if self.revision != expected_revision {
            return Err(ConfigError::RevisionConflict {
                expected: expected_revision,
                actual: self.revision,
            });
        }

        let actual_revision = Self::load_from(&self.path)?.revision;
        if actual_revision != expected_revision {
            return Err(ConfigError::RevisionConflict {
                expected: expected_revision,
                actual: actual_revision,
            });
        }
        Ok(())
    }

    fn stored(&self, name: &str) -> JsonValue {
        self.root
            .get(name)
            .cloned()
            .unwrap_or_else(|| JsonValue::Object(JsonMap::new()))
    }

    fn resolved_stored(&self, name: &str) -> JsonValue {
        if self.schemas.get(name).is_some() {
            return self
                .validated_root
                .get(name)
                .cloned()
                .unwrap_or_else(|| JsonValue::Object(JsonMap::new()));
        }
        self.stored(name)
    }

    fn rebuild_validation(&mut self) {
        let (shell, shell_diagnostics) = resolve_shell_settings(&self.root);
        let mut diagnostics = self.document_diagnostics.clone();
        diagnostics.extend(shell_diagnostics);
        let mut validated_root = JsonMap::new();
        for (name, value) in &self.root {
            let base = name.split('#').next().unwrap_or(name);
            let Some(schema) = self.schemas.get(base) else {
                continue;
            };
            let validated = validate_json_schema(name, "", &schema.schema, value, &mut diagnostics);
            if !validated.is_null() {
                validated_root.insert(name.clone(), validated);
            }
        }
        self.shell = shell;
        self.diagnostics = diagnostics;
        self.validated_root = validated_root;
    }
}

fn settings_parent(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

fn document_revision(root: &JsonMap<String, JsonValue>) -> u64 {
    root.get(REVISION_KEY)
        .and_then(JsonValue::as_u64)
        .unwrap_or_default()
}

fn ensure_settings_directory(parent: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;

        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)?;
    }
    #[cfg(not(unix))]
    {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn create_settings_temporary(path: &Path, parent: &Path) -> io::Result<(PathBuf, File)> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("settings.json");

    for _ in 0..128 {
        let sequence = TEMP_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temporary = parent.join(format!(
            ".{file_name}.tmp-{}-{sequence}",
            std::process::id()
        ));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;

            options.mode(0o600);
        }
        match options.open(&temporary) {
            Ok(file) => return Ok((temporary, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }

    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        format!(
            "could not allocate a unique temporary settings file in {}",
            parent.display()
        ),
    ))
}

fn sync_settings_directory(parent: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        File::open(parent)?.sync_all()
    }
    #[cfg(not(unix))]
    {
        let _ = parent;
        Ok(())
    }
}

fn validate_schema_definition(schema: &JsonValue, path: &str) -> Result<(), SettingsSchemaError> {
    let Some(schema) = schema.as_object() else {
        return Err(SettingsSchemaError::InvalidSchema {
            path: path.to_string(),
            message: "schema must be an object".into(),
        });
    };
    if let Some(kind) = schema.get("type") {
        let Some(kind) = kind.as_str() else {
            return Err(SettingsSchemaError::InvalidSchema {
                path: join_schema_path(path, "type"),
                message: "type must be a string".into(),
            });
        };
        if !matches!(
            kind,
            "any"
                | "object"
                | "array"
                | "string"
                | "str"
                | "size"
                | "duration"
                | "color"
                | "enum"
                | "boolean"
                | "bool"
                | "integer"
                | "int"
                | "number"
                | "float"
        ) {
            return Err(SettingsSchemaError::InvalidSchema {
                path: join_schema_path(path, "type"),
                message: format!("unsupported type '{kind}'"),
            });
        }
    }
    if let Some(properties) = schema.get("properties") {
        let Some(properties) = properties.as_object() else {
            return Err(SettingsSchemaError::InvalidSchema {
                path: join_schema_path(path, "properties"),
                message: "properties must be an object".into(),
            });
        };
        for (key, child) in properties {
            validate_schema_definition(child, &join_schema_path(path, key))?;
        }
    }
    if let Some(items) = schema.get("items") {
        validate_schema_definition(items, &join_schema_path(path, "items"))?;
    }
    if let Some(additional) = schema.get("additionalProperties") {
        if !additional.is_boolean() && !additional.is_object() {
            return Err(SettingsSchemaError::InvalidSchema {
                path: join_schema_path(path, "additionalProperties"),
                message: "additionalProperties must be a boolean or schema object".into(),
            });
        }
        if additional.is_object() {
            validate_schema_definition(
                additional,
                &join_schema_path(path, "additionalProperties"),
            )?;
        }
    }
    for bound in ["minimum", "maximum"] {
        if let Some(value) = schema.get(bound)
            && !value.is_number()
        {
            return Err(SettingsSchemaError::InvalidSchema {
                path: join_schema_path(path, bound),
                message: "numeric bounds must be numbers".into(),
            });
        }
    }
    if let Some(enumeration) = schema.get("enum")
        && !enumeration.is_array()
    {
        return Err(SettingsSchemaError::InvalidSchema {
            path: join_schema_path(path, "enum"),
            message: "enum must be an array".into(),
        });
    }
    Ok(())
}

fn join_schema_path(prefix: &str, key: &str) -> String {
    if prefix.is_empty() {
        key.to_string()
    } else {
        format!("{prefix}.{key}")
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
    const ROOT_KEYS: &[&str] = &[SHELL_NAMESPACE, SCHEMA_VERSION_KEY, REVISION_KEY];
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

fn non_object_document_diagnostic(source: &str, value: &JsonValue) -> SettingsDiagnostic {
    SettingsDiagnostic::error(
        "",
        "",
        format!("{source} a JSON object, found {}", json_type_name(value)),
        "replace the document with a JSON object",
    )
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
    fn owner_schema_filters_runtime_values_but_keeps_raw_orphans_for_repair() {
        let mut store = store(json!({
            "@mesh/test": {
                "enabled": true,
                "limit": 0,
                "unknown": "kept"
            },
            "@mesh/test#alternate": { "enabled": false }
        }));
        let schema = SettingsNamespaceSchema::new(
            "@mesh/test",
            "@mesh/test",
            json!({
                "type": "object",
                "properties": {
                    "enabled": { "type": "boolean" },
                    "limit": { "type": "integer", "minimum": 1 }
                },
                "additionalProperties": false
            }),
        )
        .unwrap();

        store
            .replace_namespace_schemas_transactionally([schema])
            .unwrap();

        assert_eq!(store.namespace("@mesh/test"), json!({ "enabled": true }));
        assert_eq!(
            store.namespace("@mesh/test#alternate"),
            json!({ "enabled": false })
        );
        assert_eq!(store.to_value()["@mesh/test"]["unknown"], json!("kept"));
        assert_eq!(
            store
                .diagnostics()
                .iter()
                .filter(|diagnostic| diagnostic.namespace == "@mesh/test")
                .count(),
            2
        );
    }

    #[test]
    fn schema_registration_is_atomic_across_duplicate_owners() {
        let mut store = store(json!({
            "@mesh/test": { "enabled": "wrong" }
        }));
        let first = SettingsNamespaceSchema::new(
            "@mesh/test",
            "@mesh/test",
            json!({ "enabled": { "type": "boolean" } }),
        )
        .unwrap();
        let duplicate = SettingsNamespaceSchema::new(
            "@mesh/test",
            "@mesh/test",
            json!({ "enabled": { "type": "string" } }),
        )
        .unwrap();

        let error = store
            .replace_namespace_schemas_transactionally([first, duplicate])
            .unwrap_err();

        assert!(matches!(
            error,
            SettingsSchemaError::DuplicateNamespace { .. }
        ));
        assert!(store.schema_registry().iter().next().is_none());
        assert_eq!(store.namespace("@mesh/test")["enabled"], json!("wrong"));
        assert!(store.diagnostics().is_empty());
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
    fn revision_checked_save_rejects_a_stale_settings_candidate() {
        let root = std::env::temp_dir().join(format!(
            "mesh-settings-revision-{}-{}",
            std::process::id(),
            line!()
        ));
        let path = root.join("settings.json");

        let mut first = SettingsStore::from_value(&path, json!({})).unwrap();
        first.set_namespace("shell", json!({ "i18n": { "locale": "sk" } }));
        first.save_if_revision(0).expect("initial revision commits");
        assert_eq!(first.revision(), 1);

        let stale = SettingsStore::load_from(&path).unwrap();
        let mut current = stale.clone();
        current.set_namespace("shell", json!({ "i18n": { "locale": "cs" } }));
        current
            .save_if_revision(1)
            .expect("current revision commits");

        let mut stale = stale;
        stale.set_namespace("shell", json!({ "i18n": { "locale": "de" } }));
        let error = stale
            .save_if_revision(1)
            .expect_err("stale locale writes must not overwrite a newer choice");
        assert!(matches!(
            error,
            ConfigError::RevisionConflict {
                expected: 1,
                actual: 2
            }
        ));

        let loaded = SettingsStore::load_from(&path).unwrap();
        assert_eq!(loaded.revision(), 2);
        assert_eq!(loaded.shell().i18n.locale, "cs");
        std::fs::remove_dir_all(root).ok();
    }

    #[cfg(unix)]
    #[test]
    fn save_uses_owner_only_file_and_directory_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let root = std::env::temp_dir().join(format!(
            "mesh-settings-permissions-{}-{}",
            std::process::id(),
            line!()
        ));
        let parent = root.join("config");
        let path = parent.join("settings.json");
        let store = SettingsStore::from_value(&path, json!({})).unwrap();

        store.save().expect("write settings");

        assert_eq!(
            std::fs::metadata(&parent).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::read_dir(&parent)
                .unwrap()
                .filter_map(Result::ok)
                .count(),
            1,
            "the unique temporary file must be removed by the rename"
        );

        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn concurrent_saves_keep_complete_documents_and_clean_temporary_files() {
        let root = std::env::temp_dir().join(format!(
            "mesh-settings-concurrent-{}-{}",
            std::process::id(),
            line!()
        ));
        let path = root.join("settings.json");
        let shared_path = std::sync::Arc::new(path.clone());

        let workers = (0..4)
            .map(|worker| {
                let path = shared_path.clone();
                std::thread::spawn(move || {
                    let mut store = SettingsStore::from_value(&*path, json!({})).unwrap();
                    store.set_namespace(
                        "shell",
                        json!({
                            "i18n": { "locale": format!("worker-{worker}") }
                        }),
                    );
                    for _ in 0..8 {
                        store.save().unwrap();
                    }
                })
            })
            .collect::<Vec<_>>();
        for worker in workers {
            worker
                .join()
                .expect("concurrent settings writer must not panic");
        }

        let loaded = SettingsStore::load_from(&path).expect("last atomic document is valid JSON");
        assert!(loaded.shell().i18n.locale.starts_with("worker-"));
        assert_eq!(
            std::fs::read_dir(&root)
                .unwrap()
                .filter_map(Result::ok)
                .count(),
            1,
            "unique temporary files must not remain after concurrent saves"
        );

        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn a_non_object_document_loads_defaults_and_retains_a_root_diagnostic() {
        let path = std::env::temp_dir().join(format!(
            "mesh-settings-invalid-{}-{}.json",
            std::process::id(),
            line!()
        ));
        std::fs::write(&path, "[]").unwrap();
        let store = SettingsStore::load_from(&path).expect("non-object documents are recoverable");
        std::fs::remove_file(&path).ok();

        assert_eq!(store.shell().theme.active, "tokyo-night");
        assert_eq!(store.namespace_names().count(), 0);
        let diagnostic = only(store.diagnostics());
        assert!(diagnostic.is_error());
        assert_eq!(diagnostic.namespace, "");
        assert_eq!(diagnostic.key_path, "");
        assert!(diagnostic.message.contains("must contain a JSON object"));
        assert_eq!(
            diagnostic.suggested_action,
            "replace the document with a JSON object"
        );
    }

    #[test]
    fn a_non_object_value_builds_a_default_store_with_a_root_diagnostic() {
        let store = SettingsStore::from_value("/tmp/mesh-test-settings.json", json!(null))
            .expect("non-object values are recoverable");

        assert_eq!(store.shell().theme.active, "tokyo-night");
        let diagnostic = only(store.diagnostics());
        assert_eq!(diagnostic.namespace, "");
        assert_eq!(diagnostic.key_path, "");
        assert!(diagnostic.message.contains("found null"));
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
        assert_eq!(diagnostic.location(), "shel");
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
