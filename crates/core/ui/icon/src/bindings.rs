use crate::registry::SupportedAxes;
use std::collections::HashMap;
use std::path::PathBuf;

/// One loaded icon-pack module's bindings, registered with the icon
/// registry. Pure mapping — no assets shipped here. The mapping target
/// strings (`<asset-pack>/<asset-name>`) are resolved at icon-render time
/// against fonts declared in `font_aliases` (looked up via fontconfig)
/// or against installed XDG icon themes by name.
#[derive(Debug, Clone)]
pub struct IconPackBindings {
    /// Short alias used in pack-qualified syntax (`<pack-id>/<name>`).
    pub pack_id: String,
    /// Full module id (e.g. `@mesh/icons-material-rounded`).
    pub module_id: String,
    /// Logical name → asset reference (`<asset-pack>/<asset-name>`).
    pub mappings: HashMap<String, String>,
    /// Variable-font axes the underlying assets expose.
    pub axes: SupportedAxes,
    /// Font aliases declared in `mesh.icon_pack.requires.fonts`.
    /// Keyed by alias; the right-hand `FontAsset` carries fontconfig
    /// family name and an optional codepoints map path.
    pub font_aliases: HashMap<String, FontAsset>,
}

#[derive(Debug, Clone)]
pub struct FontAsset {
    pub family: String,
    pub glyph_map_path: Option<PathBuf>,
    pub resolved_font_path: Option<PathBuf>,
}

/// Per-frontend icon resolution context, registered with the icon
/// registry when a frontend module loads. The registry combines this
/// with the loaded `IconPackBindings` set to produce the effective
/// resolution chain at lookup time.
#[derive(Debug, Clone, Default)]
pub struct FrontendIconBindings {
    /// Icon-pack module ids the frontend declares as dependencies, in
    /// declaration order (first preference wins for duplicate logical
    /// names).
    pub declared_pack_chain: Vec<String>,
    /// Author-side per-icon overrides from the frontend's `package.json`
    /// `icons.overrides`. Format: logical name → `<pack-id>/<asset-name>`
    /// or absolute path.
    pub author_overrides: HashMap<String, String>,
    /// User-side override for `declared_pack_chain` (from shell
    /// settings `modules.<id>.icons.use_packs`). When `Some`, this list
    /// **replaces** the declared chain.
    pub user_pack_chain: Option<Vec<String>>,
    /// User-side per-icon overrides (shell settings
    /// `modules.<id>.icons.overrides`). Highest priority of any
    /// resolution path.
    pub user_overrides: HashMap<String, String>,
    /// Frontend opted out of the shell-default pack.
    pub ignore_shell_default_frontend: bool,
    /// User opted this module out of the shell-default pack.
    pub ignore_shell_default_user: bool,
}

impl FrontendIconBindings {
    /// Compute the effective ordered icon-pack chain for this frontend,
    /// taking into account user override of the declared chain and the
    /// shell default. The result is a list of module ids; the registry
    /// looks each up to find its `IconPackBindings`.
    pub fn effective_chain(&self, shell_default: Option<&str>) -> Vec<String> {
        let mut chain = Vec::new();
        let suppress_default = self.ignore_shell_default_frontend || self.ignore_shell_default_user;
        if !suppress_default
            && let Some(default_id) = shell_default
            && !default_id.is_empty()
        {
            chain.push(default_id.to_string());
        }
        let source = self
            .user_pack_chain
            .as_deref()
            .unwrap_or(&self.declared_pack_chain);
        for id in source {
            if !chain.iter().any(|existing| existing == id) {
                chain.push(id.clone());
            }
        }
        chain
    }
}

/// Parse a target string of the form `<pack>/<name>`. Returns
/// `Some((pack, name))` on success and `None` for malformed values
/// (empty halves) or values that lack a slash. Targets that lack a
/// slash are treated by the caller as bare absolute paths or
/// hicolor-relative names.
pub fn parse_target(target: &str) -> Option<(&str, &str)> {
    let (pack, name) = target.split_once('/')?;
    if pack.is_empty() || name.is_empty() {
        return None;
    }
    Some((pack, name))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack_chain(packs: &[&str]) -> FrontendIconBindings {
        FrontendIconBindings {
            declared_pack_chain: packs.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn effective_chain_prepends_shell_default() {
        let bindings = pack_chain(&["@mesh/icons-author"]);
        let chain = bindings.effective_chain(Some("@mesh/icons-default"));
        assert_eq!(chain, vec!["@mesh/icons-default", "@mesh/icons-author"]);
    }

    #[test]
    fn effective_chain_skips_default_when_frontend_opts_out() {
        let mut bindings = pack_chain(&["@mesh/icons-author"]);
        bindings.ignore_shell_default_frontend = true;
        let chain = bindings.effective_chain(Some("@mesh/icons-default"));
        assert_eq!(chain, vec!["@mesh/icons-author"]);
    }

    #[test]
    fn effective_chain_user_override_replaces_declared() {
        let mut bindings = pack_chain(&["@mesh/icons-author"]);
        bindings.user_pack_chain = Some(vec!["@mesh/icons-user".into()]);
        let chain = bindings.effective_chain(Some("@mesh/icons-default"));
        assert_eq!(chain, vec!["@mesh/icons-default", "@mesh/icons-user"]);
    }

    #[test]
    fn effective_chain_dedups_when_default_already_in_chain() {
        let bindings = pack_chain(&["@mesh/icons-default", "@mesh/icons-extra"]);
        let chain = bindings.effective_chain(Some("@mesh/icons-default"));
        assert_eq!(chain, vec!["@mesh/icons-default", "@mesh/icons-extra"]);
    }

    #[test]
    fn parse_target_splits_pack_and_name() {
        assert_eq!(parse_target("lucide/home"), Some(("lucide", "home")));
        assert_eq!(
            parse_target("hicolor/audio-volume-high"),
            Some(("hicolor", "audio-volume-high"))
        );
    }

    #[test]
    fn parse_target_rejects_malformed() {
        assert_eq!(parse_target("home"), None);
        assert_eq!(parse_target("/home"), None);
        assert_eq!(parse_target("lucide/"), None);
    }
}
