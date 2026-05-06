pub(super) fn log_message(module_id: &str, level: &str, message: &str) {
    match level.to_ascii_lowercase().as_str() {
        "info" => tracing::info!(module_id = module_id, "{message}"),
        "warn" | "warning" => tracing::warn!(module_id = module_id, "{message}"),
        "error" => tracing::error!(module_id = module_id, "{message}"),
        "debug" => tracing::debug!(module_id = module_id, "{message}"),
        _ => tracing::warn!(
            module_id = module_id,
            "unknown log level `{level}`: {message}"
        ),
    }
}
