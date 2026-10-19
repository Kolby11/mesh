use super::{InstalledModuleNode, ModuleGraphDiagnostic};

use mesh_core_theme::{
    DEFAULT_MAX_THEME_SOURCE_BYTES, SEMANTIC_TOKEN_CONTRACT_VERSION, ThemeSourceHandle, TokenValue,
    canonical_token_name, css_token_usage, is_private_token, is_standard_token,
    is_supported_token_contract, missing_semantic_tokens, module_token_name, theme_module_targets,
    token_is_declared_locally,
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

/// Enabled modules this module uses whose `mesh.theme.tokens` form a shared
/// token family, sorted by id.
pub(super) fn token_families(
    modules: &HashMap<String, InstalledModuleNode>,
    module: &InstalledModuleNode,
) -> Vec<String> {
    let mut families = module
        .manifest
        .mesh
        .uses
        .modules
        .keys()
        .filter(|id| *id != &module.id)
        .filter(|id| {
            modules.get(*id).is_some_and(|family| {
                family.enabled
                    && family
                        .manifest
                        .mesh
                        .theme
                        .as_ref()
                        .is_some_and(|theme| !theme.tokens.is_empty())
            })
        })
        .cloned()
        .collect::<Vec<_>>();
    families.sort();
    families
}

/// Dotted token names a module may read beyond the semantic contract: its
/// own declarations and those of the families it uses.
pub(super) fn visible_module_tokens(
    modules: &HashMap<String, InstalledModuleNode>,
    module: &InstalledModuleNode,
) -> BTreeSet<String> {
    let declared = |module: &InstalledModuleNode| {
        module
            .manifest
            .mesh
            .theme
            .as_ref()
            .map(|theme| {
                theme
                    .tokens
                    .keys()
                    .map(|name| module_token_name(name))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };
    let mut visible = declared(module).into_iter().collect::<BTreeSet<_>>();
    for family in token_families(modules, module) {
        visible.extend(declared(&modules[&family]));
    }
    visible
}

pub(super) fn diagnose(
    modules: &HashMap<String, InstalledModuleNode>,
    diagnostics: &mut Vec<ModuleGraphDiagnostic>,
) {
    for module in modules.values().filter(|module| module.enabled) {
        if let Some(theme) = &module.manifest.mesh.theme {
            let visible = visible_module_tokens(modules, module);
            for name in &theme.tokens_used {
                if !is_standard_token(name)
                    && !is_private_token(name)
                    && !visible.contains(&canonical_token_name(name))
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
            let mut declared = theme.tokens.iter().collect::<Vec<_>>();
            declared.sort_by(|left, right| left.0.cmp(right.0));
            for (token, value) in declared {
                let TokenValue::String(value) = value else {
                    continue;
                };
                for read in css_token_usage(value).references {
                    if !is_standard_token(&read.name)
                        && !visible.contains(&canonical_token_name(&read.name))
                    {
                        record(
                            module,
                            "token_default_outside_contract",
                            &format!("token:{token}"),
                            format!(
                                "module token '{token}' defaults through '{}', which is neither a semantic contract {SEMANTIC_TOKEN_CONTRACT_VERSION} role nor a token this module or its families declare; themes that do not tune it leave it unresolved",
                                read.name
                            ),
                            diagnostics,
                        );
                    }
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
                Some(version) if !is_supported_token_contract(version) => record(
                    module,
                    "unsupported_token_contract_version",
                    &theme.id,
                    format!(
                        "theme '{}' declares unknown token contract '{version}'; this runtime knows up to {SEMANTIC_TOKEN_CONTRACT_VERSION}",
                        theme.id
                    ),
                    diagnostics,
                ),
                _ => {}
            }
            let mut modes = theme.modes.iter().collect::<Vec<_>>();
            modes.sort();
            for (mode, path) in modes {
                let Some(source) = read_theme_source(root, path) else {
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
                if let Some(version) =
                    declared_version.filter(|version| is_supported_token_contract(version))
                {
                    if let Ok(missing) = missing_semantic_tokens(&source, version) {
                        if !missing.is_empty() {
                            record(
                                module,
                                "incomplete_token_contract",
                                &format!("{}:{mode}", theme.id),
                                format!(
                                    "theme '{}' mode '{mode}' omits semantic token contract {version} values: {}",
                                    theme.id,
                                    missing.join(", ")
                                ),
                                diagnostics,
                            );
                        }
                    }
                }
                let Ok(targets) = theme_module_targets(&source) else {
                    continue;
                };
                for target in targets {
                    for selector in &target.internal_selectors {
                        record(
                            module,
                            "theme_styles_component_internals",
                            &format!("{}:{mode}:{}:{selector}", theme.id, target.module_id),
                            format!(
                                "theme '{}' mode '{mode}' styles '{selector}' inside {}; classes and ids are that module's internals and may change when it updates, so prefer its declared tokens or element rules",
                                theme.id, target.module_id
                            ),
                            diagnostics,
                        );
                    }
                    let Some(owner) = modules.get(&target.module_id).filter(|owner| owner.enabled)
                    else {
                        continue;
                    };
                    let declared = visible_module_tokens(modules, owner);
                    for token in target.tokens.difference(&declared) {
                        record(
                            module,
                            "theme_sets_undeclared_module_token",
                            &format!("{}:{mode}:{}:{token}", theme.id, target.module_id),
                            format!(
                                "theme '{}' mode '{mode}' sets '{token}' for {}, which does not declare it in mesh.theme.tokens; the value has no reader",
                                theme.id, target.module_id
                            ),
                            diagnostics,
                        );
                    }
                }
            }
        }
    }
}

fn read_theme_source(root: &Path, path: &str) -> Option<String> {
    ThemeSourceHandle::new(root, path)
        .ok()?
        .read_utf8_bounded(DEFAULT_MAX_THEME_SOURCE_BYTES)
        .ok()
}

/// How completely one theme tunes the installed modules that expose styling
/// hooks. Every module follows a theme through the semantic contract; this
/// reports where a theme goes further and where it reaches into internals.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ThemeCoverage {
    pub theme: String,
    pub token_contract: Option<String>,
    /// Modules or families whose declared tokens the theme sets.
    pub tuned: Vec<ThemeCoverageEntry>,
    /// Modules or families declaring tokens the theme leaves at their defaults.
    pub defaults: Vec<String>,
    /// Modules that declare they ignore the active theme.
    pub independent: Vec<String>,
    /// Theme selectors that style another module's classes or ids.
    pub internal_selectors: Vec<ThemeCoverageSelector>,
    /// `@module` targets that are not installed and enabled.
    pub absent_targets: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ThemeCoverageEntry {
    pub module: String,
    pub tokens_set: usize,
    pub tokens_declared: usize,
    pub elements: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ThemeCoverageSelector {
    pub module: String,
    pub selector: String,
}

pub(super) fn theme_coverage(
    modules: &HashMap<String, InstalledModuleNode>,
    descriptor: &mesh_core_theme::ThemePackDescriptor,
) -> ThemeCoverage {
    let owner = modules.get(&descriptor.owner_module);
    let token_contract = owner.and_then(|owner| {
        owner
            .manifest
            .mesh
            .contributes
            .themes
            .iter()
            .find(|theme| theme.id == descriptor.local_id)
            .and_then(|theme| theme.token_contract.clone())
    });
    let mut targeted = BTreeMap::<String, (BTreeSet<String>, BTreeSet<String>)>::new();
    let mut internal = BTreeSet::new();
    for mode in descriptor.modes.values() {
        let Ok(source) = mode
            .source
            .read_utf8_bounded(DEFAULT_MAX_THEME_SOURCE_BYTES)
        else {
            continue;
        };
        let Ok(targets) = theme_module_targets(&source) else {
            continue;
        };
        for target in targets {
            for selector in &target.internal_selectors {
                internal.insert((target.module_id.clone(), selector.clone()));
            }
            let entry = targeted.entry(target.module_id).or_default();
            entry.0.extend(target.tokens);
            entry.1.extend(target.elements);
        }
    }

    let mut coverage = ThemeCoverage {
        theme: descriptor.id.clone(),
        token_contract,
        ..ThemeCoverage::default()
    };
    let mut enabled = modules
        .values()
        .filter(|module| module.enabled)
        .collect::<Vec<_>>();
    enabled.sort_by(|left, right| left.id.cmp(&right.id));
    for module in enabled {
        let Some(theme) = &module.manifest.mesh.theme else {
            continue;
        };
        if theme.independent {
            coverage.independent.push(module.id.clone());
            continue;
        }
        let declared = theme
            .tokens
            .keys()
            .map(|name| module_token_name(name))
            .collect::<BTreeSet<_>>();
        match targeted.get(&module.id) {
            Some((tokens, elements)) => coverage.tuned.push(ThemeCoverageEntry {
                module: module.id.clone(),
                tokens_set: tokens.intersection(&declared).count(),
                tokens_declared: declared.len(),
                elements: elements.iter().cloned().collect(),
            }),
            None if !declared.is_empty() => coverage.defaults.push(module.id.clone()),
            None => {}
        }
    }
    for (module_id, (_, elements)) in &targeted {
        let installed = modules.get(module_id).filter(|module| module.enabled);
        match installed {
            None => coverage.absent_targets.push(module_id.clone()),
            Some(module) if module.manifest.mesh.theme.is_none() => {
                coverage.tuned.push(ThemeCoverageEntry {
                    module: module_id.clone(),
                    tokens_set: 0,
                    tokens_declared: 0,
                    elements: elements.iter().cloned().collect(),
                })
            }
            Some(_) => {}
        }
    }
    coverage
        .tuned
        .sort_by(|left, right| left.module.cmp(&right.module));
    coverage.internal_selectors = internal
        .into_iter()
        .map(|(module, selector)| ThemeCoverageSelector { module, selector })
        .collect();
    coverage
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
