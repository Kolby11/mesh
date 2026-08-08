//! Composition modules — an installable, versionable shell composition.
//!
//! A profile alone is a config file: no version, no dependencies, no lock
//! entry, no capability review. A **composition module** is a profile that is
//! also a module, so a whole shell family can be installed, published, pinned,
//! updated, rolled back, and forked with the machinery modules already have.
//!
//! Layering, most general to most specific:
//!
//! ```text
//! extends chain (base → derived)  ⊕  composition.compose  ⊕  profile deltas
//! ```
//!
//! A composition **binds**; it never **owns**. It selects a provider, it does
//! not contain one — backends are effectively machine singletons while
//! compositions are swappable, and durable service data stays shared.

use super::{
    ModuleKind, ModuleManifest, ModuleManifestError, ProfileResources, ProfileRootInstance,
    ShellProfile,
};
use crate::manifest::SurfaceLayoutSection;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

/// The `mesh.compose` block: what a composition selects.
///
/// Structurally the same decisions a profile holds, plus extension point
/// overrides, so one merge function serves both layers.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompositionSpec {
    #[serde(default)]
    pub roots: BTreeMap<String, ProfileRootInstance>,
    #[serde(default)]
    pub background_services: BTreeSet<String>,
    #[serde(default)]
    pub providers: BTreeMap<String, String>,
    #[serde(default)]
    pub resources: ProfileResources,
    /// Per-extension-point overrides: how this composition rearranges the UI
    /// its members contribute, without editing any member.
    #[serde(default)]
    pub slots: BTreeMap<String, SlotOverride>,
    /// Sparse ordered component placements for named customizable slots,
    /// keyed by root instance and then component-local slot name.
    #[serde(default)]
    pub node_slots: BTreeMap<String, BTreeMap<String, NodeSlotOverride>>,
    #[serde(default)]
    pub settings: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NodeSlotOverride {
    #[serde(default)]
    pub nodes: Vec<ComponentPlacement>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComponentPlacement {
    pub id: String,
    #[serde(rename = "use")]
    pub contribution: String,
    #[serde(default)]
    pub props: serde_json::Map<String, serde_json::Value>,
}

/// A composition's authority over one extension point.
///
/// This is what makes a shell family a *family*: it can replace `@mesh/audio`'s
/// settings page with its own, hide a page it does not want, and fix the order
/// — all without touching the audio module.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SlotOverride {
    /// Contributing module id → replacement module id.
    #[serde(default)]
    pub replace: BTreeMap<String, String>,
    /// Contributing module ids whose contributions are not rendered.
    #[serde(default)]
    pub suppress: BTreeSet<String>,
    /// Explicit render order by contributing module id. Modules not listed keep
    /// their declared order after the listed ones.
    #[serde(default)]
    pub order: Vec<String>,
}

impl SlotOverride {
    fn merge_from(&mut self, other: &Self) {
        for (from, to) in &other.replace {
            self.replace.insert(from.clone(), to.clone());
        }
        self.suppress.extend(other.suppress.iter().cloned());
        if !other.order.is_empty() {
            self.order = other.order.clone();
        }
    }
}

/// A composition resolved through its `extends` chain and the user's profile.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EffectiveComposition {
    pub spec: CompositionSpec,
    /// The composition module this was resolved from, if any. A profile with no
    /// `from` is a hand-built composition — the pre-composition behavior, still
    /// fully supported.
    pub source_module: Option<String>,
    /// User overrides that no longer match anything the composition declares.
    ///
    /// Retained rather than dropped: discarding them would lose the user's work
    /// on every upstream rename. `mesh profile prune` clears them on request.
    pub orphaned_overrides: Vec<String>,
}

impl EffectiveComposition {
    /// Project back into a [`ShellProfile`] so the existing activation closure,
    /// root-graph application, and transactional switch work unchanged.
    pub fn to_profile(&self) -> ShellProfile {
        ShellProfile {
            schema_version: super::PROFILE_SCHEMA_VERSION,
            from: self.source_module.as_ref().map(|module| CompositionRef {
                module: module.clone(),
                version: None,
            }),
            roots: self.spec.roots.clone(),
            background_services: self.spec.background_services.clone(),
            providers: self.spec.providers.clone(),
            resources: self.spec.resources.clone(),
            node_slots: self.spec.node_slots.clone(),
            settings: self.spec.settings.clone(),
        }
    }
}

/// Identifies the composition module a profile instantiates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompositionRef {
    pub module: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// Resolve `composition (+ its extends chain) ⊕ profile deltas`.
///
/// `manifests` is the installed set; only composition modules are consulted.
pub fn resolve_composition<'a>(
    profile: &ShellProfile,
    manifests: impl IntoIterator<Item = &'a ModuleManifest>,
) -> Result<EffectiveComposition, ModuleManifestError> {
    let compositions: HashMap<&str, &ModuleManifest> = manifests
        .into_iter()
        .filter(|manifest| manifest.mesh.kind == ModuleKind::Composition)
        .map(|manifest| (manifest.name.as_str(), manifest))
        .collect();

    let mut spec = CompositionSpec::default();
    let source_module = profile.from.as_ref().map(|from| from.module.clone());

    if let Some(from) = &profile.from {
        let chain = composition_chain(&from.module, &compositions)?;
        // Base first, so a derived composition wins where they disagree.
        for module_id in chain.iter().rev() {
            let manifest = compositions[module_id.as_str()];
            if let Some(compose) = &manifest.mesh.compose {
                merge_spec(&mut spec, compose);
            }
        }
    }

    let mut orphaned_overrides = Vec::new();
    let profile_spec = profile.as_composition_spec();
    for instance_id in profile_spec.roots.keys() {
        if profile.from.is_some() && !spec.roots.contains_key(instance_id) {
            orphaned_overrides.push(instance_id.clone());
        }
    }
    for instance_id in profile_spec.node_slots.keys() {
        if profile.from.is_some() && !spec.roots.contains_key(instance_id) {
            orphaned_overrides.push(format!("nodeSlots.{instance_id}"));
        }
    }
    merge_spec(&mut spec, &profile_spec);

    Ok(EffectiveComposition {
        spec,
        source_module,
        orphaned_overrides,
    })
}

/// The `extends` chain from `module_id` upward, most derived first.
fn composition_chain(
    module_id: &str,
    compositions: &HashMap<&str, &ModuleManifest>,
) -> Result<Vec<String>, ModuleManifestError> {
    let mut chain = Vec::new();
    let mut seen = HashSet::new();
    let mut current = module_id.to_string();
    loop {
        let manifest = compositions.get(current.as_str()).ok_or_else(|| {
            ModuleManifestError::Validation(format!(
                "composition {current} is not an installed composition module"
            ))
        })?;
        if !seen.insert(current.clone()) {
            chain.push(current.clone());
            return Err(ModuleManifestError::Validation(format!(
                "composition extends cycle: {}",
                chain.join(" → ")
            )));
        }
        chain.push(current.clone());
        match &manifest.mesh.extends {
            Some(parent) => current = parent.clone(),
            None => break,
        }
    }
    Ok(chain)
}

/// Layer `overlay` onto `base`. More specific wins, per field.
fn merge_spec(base: &mut CompositionSpec, overlay: &CompositionSpec) {
    for (instance_id, instance) in &overlay.roots {
        match base.roots.get_mut(instance_id) {
            Some(existing) => merge_root(existing, instance),
            None => {
                base.roots.insert(instance_id.clone(), instance.clone());
            }
        }
    }
    base.background_services
        .extend(overlay.background_services.iter().cloned());
    for (interface, provider) in &overlay.providers {
        base.providers.insert(interface.clone(), provider.clone());
    }

    if overlay.resources.theme.is_some() {
        base.resources.theme = overlay.resources.theme.clone();
    }
    // Ordered chains replace rather than merge: an icon or font chain is an
    // ordered fallback list, and interleaving two orderings has no meaning.
    if !overlay.resources.icons.is_empty() {
        base.resources.icons = overlay.resources.icons.clone();
    }
    if !overlay.resources.fonts.is_empty() {
        base.resources.fonts = overlay.resources.fonts.clone();
    }
    if !overlay.resources.languages.is_empty() {
        base.resources.languages = overlay.resources.languages.clone();
    }

    for (point, over) in &overlay.slots {
        base.slots
            .entry(point.clone())
            .or_default()
            .merge_from(over);
    }
    for (instance, slots) in &overlay.node_slots {
        let target = base.node_slots.entry(instance.clone()).or_default();
        for (slot, over) in slots {
            target.insert(slot.clone(), over.clone());
        }
    }

    for (namespace, value) in &overlay.settings {
        match base.settings.get_mut(namespace) {
            Some(existing) => merge_json(existing, value),
            None => {
                base.settings.insert(namespace.clone(), value.clone());
            }
        }
    }
}

/// A root instance layers per field, so a user changing one surface field does
/// not discard the composition's other placement decisions.
fn merge_root(base: &mut ProfileRootInstance, overlay: &ProfileRootInstance) {
    if !overlay.module.is_empty() {
        base.module = overlay.module.clone();
    }
    if overlay.entrypoint != "main" {
        base.entrypoint = overlay.entrypoint.clone();
    }
    base.active = overlay.active;
    if let Some(surface) = &overlay.surface {
        base.surface = Some(match base.surface.take() {
            Some(existing) => merge_surface(existing, surface),
            None => surface.clone(),
        });
    }
}

macro_rules! merge_surface_fields {
    ($base:expr, $overlay:expr, $($field:ident),+ $(,)?) => {
        $(if $overlay.$field.is_some() {
            $base.$field = $overlay.$field.clone();
        })+
    };
}

fn merge_surface(
    mut base: SurfaceLayoutSection,
    overlay: &SurfaceLayoutSection,
) -> SurfaceLayoutSection {
    merge_surface_fields!(
        base,
        overlay,
        role,
        promotable,
        anchor,
        layer,
        exclusive_zone,
        keyboard_mode,
        margins,
        visible_on_start,
        blur,
        title,
        app_id,
        resizable,
        decorations,
    );
    base
}

fn merge_json(base: &mut serde_json::Value, overlay: &serde_json::Value) {
    match (base, overlay) {
        (serde_json::Value::Object(base), serde_json::Value::Object(overlay)) => {
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

/// Apply a composition's extension point overrides to the graph's resolved
/// contributions for one host.
///
/// Precedence, most specific first: composition override, module-provided
/// contribution, then whatever the host falls back to.
pub fn apply_slot_override<T>(
    contributions: &mut Vec<T>,
    over: &SlotOverride,
    source_module_id: impl Fn(&T) -> String,
    set_source_module_id: impl Fn(&mut T, String),
) {
    contributions.retain(|contribution| !over.suppress.contains(&source_module_id(contribution)));
    for contribution in contributions.iter_mut() {
        if let Some(replacement) = over.replace.get(&source_module_id(contribution)) {
            set_source_module_id(contribution, replacement.clone());
        }
    }
    if over.order.is_empty() {
        return;
    }
    let rank = |contribution: &T| {
        over.order
            .iter()
            .position(|module_id| *module_id == source_module_id(contribution))
            .unwrap_or(usize::MAX)
    };
    contributions.sort_by_key(rank);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(json: &str) -> ModuleManifest {
        ModuleManifest::from_json_str(json).unwrap()
    }

    fn composition(name: &str, extends: Option<&str>, compose: &str) -> ModuleManifest {
        let extends = extends
            .map(|parent| format!(r#""extends":"{parent}","#))
            .unwrap_or_default();
        manifest(&format!(
            r#"{{"name":"{name}","version":"1.0.0","mesh":{{"apiVersion":"0.1",
               "kind":"composition",{extends}"compose":{compose}}}}}"#
        ))
    }

    fn profile(json: &str) -> ShellProfile {
        ShellProfile::from_json_str(json).unwrap()
    }

    #[test]
    fn a_profile_without_provenance_stays_a_hand_built_composition() {
        let resolved = resolve_composition(
            &profile(r#"{"schemaVersion":3,"roots":{"@me/panel#default":{"module":"@me/panel"}}}"#),
            [],
        )
        .unwrap();
        assert!(resolved.source_module.is_none());
        assert_eq!(resolved.spec.roots.len(), 1);
    }

    #[test]
    fn a_derived_composition_wins_over_its_base() {
        let base = composition(
            "@mesh/base",
            None,
            r#"{"providers":{"mesh.audio":"@mesh/pipewire"},
                "resources":{"theme":"@mesh/dark","icons":["@mesh/a","@mesh/b"]}}"#,
        );
        let derived = composition(
            "@alice/desk",
            Some("@mesh/base"),
            r#"{"providers":{"mesh.audio":"@mesh/pulseaudio"},
                "resources":{"icons":["@alice/icons"]}}"#,
        );

        let resolved = resolve_composition(
            &profile(r#"{"schemaVersion":3,"from":{"module":"@alice/desk"}}"#),
            [&base, &derived],
        )
        .unwrap();

        assert_eq!(resolved.spec.providers["mesh.audio"], "@mesh/pulseaudio");
        // Scalar selections inherit; ordered chains replace wholesale.
        assert_eq!(resolved.spec.resources.theme.as_deref(), Some("@mesh/dark"));
        assert_eq!(resolved.spec.resources.icons, vec!["@alice/icons"]);
    }

    #[test]
    fn user_deltas_beat_the_composition() {
        let desk = composition(
            "@alice/desk",
            None,
            r#"{"roots":{"@mesh/panel#top":{"module":"@mesh/panel",
                 "surface":{"anchor":"top","exclusive_zone":56}}},
                "settings":{"shell":{"i18n":{"locale":"en-US"},"theme":{"mode":"dark"}}}}"#,
        );
        let resolved = resolve_composition(
            &profile(
                r#"{"schemaVersion":3,"from":{"module":"@alice/desk"},
                    "roots":{"@mesh/panel#top":{"module":"@mesh/panel","active":false,
                      "surface":{"anchor":"bottom"}}},
                    "settings":{"shell":{"i18n":{"locale":"sk-SK"}}}}"#,
            ),
            [&desk],
        )
        .unwrap();

        let root = &resolved.spec.roots["@mesh/panel#top"];
        assert!(!root.active);
        let surface = root.surface.as_ref().unwrap();
        assert_eq!(surface.anchor.as_deref(), Some("bottom"));
        // A per-field surface merge keeps the composition's other placement.
        assert_eq!(surface.exclusive_zone, Some(56));
        // Settings merge per key, not per section.
        assert_eq!(resolved.spec.settings["shell"]["i18n"]["locale"], "sk-SK");
        assert_eq!(resolved.spec.settings["shell"]["theme"]["mode"], "dark");
    }

    #[test]
    fn an_override_for_a_root_the_composition_dropped_is_retained_and_reported() {
        let desk = composition("@alice/desk", None, r#"{"roots":{}}"#);
        let resolved = resolve_composition(
            &profile(
                r#"{"schemaVersion":3,"from":{"module":"@alice/desk"},
                    "roots":{"@mesh/gone#default":{"module":"@mesh/gone"}}}"#,
            ),
            [&desk],
        )
        .unwrap();

        assert_eq!(resolved.orphaned_overrides, vec!["@mesh/gone#default"]);
        assert!(resolved.spec.roots.contains_key("@mesh/gone#default"));
    }

    #[test]
    fn an_extends_cycle_is_rejected() {
        let a = composition("@me/a", Some("@me/b"), "{}");
        let b = composition("@me/b", Some("@me/a"), "{}");
        let error = resolve_composition(
            &profile(r#"{"schemaVersion":3,"from":{"module":"@me/a"}}"#),
            [&a, &b],
        )
        .unwrap_err();
        assert!(format!("{error}").contains("extends cycle"));
    }

    #[test]
    fn slot_overrides_layer_through_the_extends_chain() {
        let base = composition(
            "@mesh/base",
            None,
            r#"{"slots":{"mesh.settings.page":{"suppress":["@mesh/noisy"],
                 "order":["@mesh/a","@mesh/b"]}}}"#,
        );
        let derived = composition(
            "@alice/desk",
            Some("@mesh/base"),
            r#"{"slots":{"mesh.settings.page":{"replace":{"@mesh/audio":"@alice/audio-page"},
                 "suppress":["@mesh/other"]}}}"#,
        );
        let resolved = resolve_composition(
            &profile(r#"{"schemaVersion":3,"from":{"module":"@alice/desk"}}"#),
            [&base, &derived],
        )
        .unwrap();

        let over = &resolved.spec.slots["mesh.settings.page"];
        assert_eq!(over.replace["@mesh/audio"], "@alice/audio-page");
        assert!(over.suppress.contains("@mesh/noisy"));
        assert!(over.suppress.contains("@mesh/other"));
        assert_eq!(over.order, vec!["@mesh/a", "@mesh/b"]);
    }

    #[test]
    fn node_slot_lists_replace_wholesale_and_an_empty_list_is_explicit() {
        let base = composition(
            "@mesh/base",
            None,
            r#"{"roots":{"@mesh/panel#top":{"module":"@mesh/panel"}},
                "nodeSlots":{"@mesh/panel#top":{"start":{"nodes":[
                  {"id":"clock","use":"@mesh/items:clock","props":{}}
                ]}}}}"#,
        );
        let resolved = resolve_composition(
            &profile(
                r#"{"schemaVersion":3,"from":{"module":"@mesh/base"},
                    "nodeSlots":{"@mesh/panel#top":{"start":{"nodes":[]}}}}"#,
            ),
            [&base],
        )
        .unwrap();
        assert!(
            resolved.spec.node_slots["@mesh/panel#top"]["start"]
                .nodes
                .is_empty()
        );
    }

    #[test]
    fn applying_a_slot_override_suppresses_replaces_and_reorders() {
        let mut contributions = vec![
            "@mesh/audio".to_string(),
            "@mesh/noisy".to_string(),
            "@mesh/network".to_string(),
        ];
        let over = SlotOverride {
            replace: BTreeMap::from([("@mesh/audio".to_string(), "@alice/audio".to_string())]),
            suppress: BTreeSet::from(["@mesh/noisy".to_string()]),
            order: vec!["@mesh/network".to_string(), "@alice/audio".to_string()],
        };
        apply_slot_override(
            &mut contributions,
            &over,
            |entry| entry.clone(),
            |entry, id| *entry = id,
        );
        assert_eq!(contributions, vec!["@mesh/network", "@alice/audio"]);
    }

    /// The Stage 2 gate: composing through an installed composition module and
    /// writing the same decisions by hand produce the same running shell.
    #[test]
    fn a_composition_backed_profile_matches_the_equivalent_hand_written_one() {
        let compose = r#"{"roots":{"@mesh/panel#top":{"module":"@mesh/panel",
              "surface":{"anchor":"top"}}},
             "providers":{"mesh.audio":"@mesh/pipewire"},
             "resources":{"icons":["@mesh/icons-default"]},
             "settings":{"shell":{"i18n":{"locale":"en-US"}}}}"#;
        let desk = composition("@mesh/desk", None, compose);

        let from_composition = resolve_composition(
            &profile(r#"{"schemaVersion":3,"from":{"module":"@mesh/desk"}}"#),
            [&desk],
        )
        .unwrap();
        let hand_written = resolve_composition(
            &profile(&format!(
                r#"{{"schemaVersion":3,{}}}"#,
                &compose[1..compose.len() - 1]
            )),
            [&desk],
        )
        .unwrap();

        assert_eq!(from_composition.spec, hand_written.spec);
        // Only the provenance differs, which is what makes one updatable.
        assert_eq!(
            from_composition.source_module.as_deref(),
            Some("@mesh/desk")
        );
        assert!(hand_written.source_module.is_none());
    }

    #[test]
    fn a_composition_may_not_request_capabilities() {
        let error = ModuleManifest::from_json_str(
            r#"{"name":"@me/desk","version":"1","mesh":{"apiVersion":"0.1","kind":"composition",
                "uses":{"capabilities":["exec.command"]}}}"#,
        )
        .unwrap_err();
        assert!(format!("{error}").contains("capabilit"));
    }

    #[test]
    fn a_composition_declares_no_surface_and_no_entry() {
        for field in [
            r#""entry":"src/main.mesh""#,
            r#""surface":{"anchor":"top"}"#,
        ] {
            assert!(
                ModuleManifest::from_json_str(&format!(
                    r#"{{"name":"@me/desk","version":"1","mesh":{{"apiVersion":"0.1",
                       "kind":"composition",{field}}}}}"#
                ))
                .is_err(),
                "composition accepted {field}"
            );
        }
    }
}
