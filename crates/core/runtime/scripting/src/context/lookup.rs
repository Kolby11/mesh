use super::{ScriptDiagnostic, ScriptError};
use mesh_core_service::{InterfaceCatalog, InterfaceResolution};
use mlua::Value as LuaValue;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

pub(super) fn record_lookup_diagnostic_lua(
    diagnostics: &Arc<Mutex<Vec<ScriptDiagnostic>>>,
    pending_side_channels: &Arc<AtomicBool>,
    module_id: &str,
    interface: &str,
    requested_version: Option<&str>,
    reason: &str,
    err: ScriptError,
) -> mlua::Error {
    record_lookup_diagnostic(
        diagnostics,
        pending_side_channels,
        module_id,
        interface,
        requested_version,
        reason,
    );
    mlua::Error::external(err)
}

pub(super) fn record_lookup_diagnostic(
    diagnostics: &Arc<Mutex<Vec<ScriptDiagnostic>>>,
    pending_side_channels: &Arc<AtomicBool>,
    module_id: &str,
    interface: &str,
    requested_version: Option<&str>,
    reason: &str,
) {
    tracing::error!(
        module_id,
        interface,
        requested_version = requested_version.unwrap_or(""),
        reason,
        "service interface lookup failed"
    );
    pending_side_channels.store(true, Ordering::Release);
    diagnostics.lock().unwrap().push(ScriptDiagnostic {
        module_id: module_id.to_string(),
        interface: interface.to_string(),
        requested_version: requested_version.map(ToOwned::to_owned),
        reason: reason.to_string(),
    });
}

pub(super) fn lookup_failure_reason(
    catalog: &InterfaceCatalog,
    resolution: &InterfaceResolution,
) -> String {
    let has_contracts = catalog
        .contracts
        .get(&resolution.requested)
        .is_some_and(|contracts| !contracts.is_empty());
    let has_providers = catalog
        .providers
        .get(&resolution.requested)
        .is_some_and(|providers| !providers.is_empty());

    match (
        resolution.contract.is_some(),
        resolution.provider.is_some(),
        resolution.requested_version.as_deref(),
        has_contracts,
        has_providers,
    ) {
        (false, false, Some(version), true, _) | (false, false, Some(version), _, true) => {
            format!(
                "requested version {version} did not match available interface contracts or providers"
            )
        }
        (false, true, _, _, _) => "missing contract".to_string(),
        (true, false, _, _, _) => "missing provider".to_string(),
        (false, false, _, false, false) => "missing contract and provider".to_string(),
        (false, false, _, false, true) => "missing contract".to_string(),
        (false, false, _, true, false) => "missing provider".to_string(),
        _ => "interface lookup failed".to_string(),
    }
}

pub(super) fn interface_error_message(interface: &str, requested_version: Option<&str>) -> String {
    format!(
        "{}{}",
        interface,
        requested_version
            .map(|value| format!(" ({value})"))
            .unwrap_or_default()
    )
}

pub(super) fn map_lua_error(err: mlua::Error) -> ScriptError {
    extract_script_error(&err).unwrap_or_else(|| ScriptError::LuaError(err.to_string()))
}

fn extract_script_error(err: &mlua::Error) -> Option<ScriptError> {
    match err {
        mlua::Error::CallbackError { cause, .. } => extract_script_error(cause),
        mlua::Error::ExternalError(err) => err.downcast_ref::<ScriptError>().cloned(),
        _ => None,
    }
}

pub(super) fn lua_err(err: mlua::Error) -> ScriptError {
    ScriptError::LuaError(err.to_string())
}

pub(super) fn lua_value_to_string(value: LuaValue) -> String {
    match value {
        LuaValue::Nil => "nil".to_string(),
        LuaValue::Boolean(v) => v.to_string(),
        LuaValue::Integer(v) => v.to_string(),
        LuaValue::Number(v) => v.to_string(),
        LuaValue::String(v) => v.to_string_lossy(),
        other => format!("{other:?}"),
    }
}
