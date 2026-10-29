/// Build the single locale state shape shared by core service updates and
/// component-local service delivery. Optional persistence fields are added by
/// the shell coordinator once the durable transaction has committed.
pub fn locale_service_payload(
    locale: &mesh_core_locale::LocaleSnapshot,
    policy: Option<&str>,
    durable_revision: Option<&str>,
) -> serde_json::Value {
    let selection = locale.selection();
    let mut payload = serde_json::json!({
        "locale": selection.active(),
        "current": selection.active(),
        "chain": selection.chain(),
        "direction": selection.direction().as_str(),
        "revision": selection.revision().to_string(),
    });
    if let Some(policy) = policy {
        payload["policy"] = serde_json::json!(policy);
    }
    if let Some(durable_revision) = durable_revision {
        payload["durable_revision"] = serde_json::json!(durable_revision);
    }
    payload
}

/// Resolve a manifest-localized field against exactly the module named by the
/// field's owner. All shell metadata consumers use this same conversion so a
/// debug label and a live descriptor cannot silently select different
/// catalogs.
pub fn resolve_manifest_text(
    locale: &mesh_core_locale::LocaleSnapshot,
    module_id: &str,
    field_path: &str,
    text: &mesh_core_module::LocalizedText,
) -> mesh_core_locale::LocalizedTextResolution {
    let resolution = match text {
        mesh_core_module::LocalizedText::Literal(value) => {
            mesh_core_locale::LocalizedTextResolution::literal(
                module_id,
                value,
                locale.catalog_snapshot().revision(),
            )
        }
        mesh_core_module::LocalizedText::Translation { key, fallback } => locale
            .module_translator(module_id)
            .resolve(key, Some(fallback)),
    };
    resolution.with_field_path(field_path)
}
