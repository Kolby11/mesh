use super::{InstalledModuleNode, ModuleGraphDiagnostic};
use mesh_core_theme::{
    DEFAULT_MAX_THEME_SOURCE_BYTES, SEMANTIC_TOKEN_CONTRACT_VERSION, ThemeSourceHandle,
    canonical_token_name, css_token_usage, is_private_token, is_standard_token,
    missing_semantic_tokens, token_is_declared_locally,
};
use std::collections::HashMap;
use std::path::Path;

pub(super) fn diagnose(
    modules: &HashMap<String, InstalledModuleNode>,
    diagnostics: &mut Vec<ModuleGraphDiagnostic>,
) {
    for module in modules.values().filter(|module| module.enabled) {
        if let Some(theme) = &module.manifest.mesh.theme {
            for name in &theme.tokens_used {
                if !is_standard_token(name)
                    && !is_private_token(name)
                    && !theme.tokens.contains_key(&canonical_token_name(name))
                {
                    record(
                        module,
                        "unknown_semantic_token",
                        "manifest",
                        format!(
                            "token '{name}' in mesh.theme.tokens_used is outside semantic token contract {SEMANTIC_TOKEN_CONTRACT_VERSION}"
                        ),
                        diagnostics,
                    );
                }
            }
        }
        let root = module.manifest_path.parent().unwrap_or(Path::new("."));
        for theme in &module.manifest.mesh.contributes.themes {
            let declared_version = theme.token_contract.as_deref();
            match declared_version {
                None => record(
                    module,
                    "missing_token_contract_version",
                    &theme.id,
                    format!(
                        "theme '{}' should declare tokenContract: \"{SEMANTIC_TOKEN_CONTRACT_VERSION}\"",
                        theme.id
                    ),
                    diagnostics,
                ),
                Some(version) if version != SEMANTIC_TOKEN_CONTRACT_VERSION => record(
                    module,
                    "unsupported_token_contract_version",
                    &theme.id,
                    format!(
                        "theme '{}' declares unknown token contract '{version}'; this runtime knows {SEMANTIC_TOKEN_CONTRACT_VERSION}",
                        theme.id
                    ),
                    diagnostics,
                ),
                _ => {}
            }
            for (mode, path) in &theme.modes {
                let Ok(handle) = ThemeSourceHandle::new(root, path) else {
                    continue;
                };
                let Ok(source) = handle.read_utf8_bounded(DEFAULT_MAX_THEME_SOURCE_BYTES) else {
                    continue;
                };
                let usage = css_token_usage(&source);
                for read in usage.references {
                    if !is_standard_token(&read.name)
                        && !is_private_token(&read.name)
                        && !token_is_declared_locally(&read.name, &usage.declarations)
                    {
                        record(
                            module,
                            "unknown_semantic_token",
                            &format!("{}:{mode}:{}", theme.id, read.span.start),
                            format!(
                                "theme '{}' mode '{mode}' reads '{}' outside semantic token contract {SEMANTIC_TOKEN_CONTRACT_VERSION} in {path}",
                                theme.id, read.name
                            ),
                            diagnostics,
                        );
                    }
                }
                if declared_version == Some(SEMANTIC_TOKEN_CONTRACT_VERSION) {
                    if let Ok(missing) = missing_semantic_tokens(&source) {
                        if !missing.is_empty() {
                            record(
                                module,
                                "incomplete_token_contract",
                                &format!("{}:{mode}", theme.id),
                                format!(
                                    "theme '{}' mode '{mode}' omits semantic token contract {SEMANTIC_TOKEN_CONTRACT_VERSION} values: {}",
                                    theme.id,
                                    missing.join(", ")
                                ),
                                diagnostics,
                            );
                        }
                    }
                }
            }
        }
    }
}

fn record(
    module: &InstalledModuleNode,
    status: &str,
    contribution: &str,
    message: String,
    diagnostics: &mut Vec<ModuleGraphDiagnostic>,
) {
    diagnostics.push(ModuleGraphDiagnostic {
        module_id: module.id.clone(),
        contribution_id: Some(format!("{}:theme:{contribution}", module.id)),
        status: status.into(),
        message,
    });
}
