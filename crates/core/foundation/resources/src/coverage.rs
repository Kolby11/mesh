use crate::{ResourceExplanationSnapshot, ResourcePackExplanation};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One semantic name that a frontend expects the active icon chain to cover.
/// The advisor treats `required` as severity only; it never changes the
/// chain or blocks a resource candidate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceSemanticNeed {
    pub module_id: String,
    pub name: String,
    pub required: bool,
}

/// One script that a role is expected to render. Script names use the same
/// pack-declared vocabulary as `font_pack.covers` and face `coverage`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceFontScriptNeed {
    pub module_id: String,
    pub role: String,
    pub script: String,
    pub required: bool,
}

/// Inputs for a coverage preview. This is deliberately separate from the
/// committed snapshot: callers can inspect a proposed locale/content set
/// before asking the normal resource coordinator to prepare a candidate.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceCoverageRequest {
    pub semantic_names: Vec<ResourceSemanticNeed>,
    pub font_scripts: Vec<ResourceFontScriptNeed>,
}

impl ResourceCoverageRequest {
    /// Seed an advisor request from the semantic icon requirements already
    /// recorded in an effective snapshot. Font script needs are content- or
    /// locale-owned, so callers add them explicitly with
    /// [`Self::add_font_script`].
    pub fn from_snapshot(snapshot: &ResourceExplanationSnapshot) -> Self {
        let mut semantic_names = BTreeMap::<(String, String), bool>::new();
        for resolution in &snapshot.icons.resolutions {
            let key = (
                resolution.module_id.clone(),
                resolution.semantic_name.clone(),
            );
            semantic_names
                .entry(key)
                .and_modify(|required| *required |= resolution.required)
                .or_insert(resolution.required);
        }
        Self {
            semantic_names: semantic_names
                .into_iter()
                .map(|((module_id, name), required)| ResourceSemanticNeed {
                    module_id,
                    name,
                    required,
                })
                .collect(),
            font_scripts: Vec::new(),
        }
    }

    pub fn add_font_script(
        &mut self,
        module_id: impl Into<String>,
        role: impl Into<String>,
        script: impl Into<String>,
        required: bool,
    ) {
        self.font_scripts.push(ResourceFontScriptNeed {
            module_id: module_id.into(),
            role: role.into(),
            script: script.into(),
            required,
        });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceCoverageKind {
    Icons,
    Fonts,
}

/// A semantic name that is not covered by the requested frontend chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceSemanticGap {
    pub module_id: String,
    pub name: String,
    pub required: bool,
    pub status: String,
    pub current_chain: Vec<String>,
    pub tried: Vec<String>,
    pub candidate_packs: Vec<String>,
}

/// A role whose selected face does not declare coverage for the requested
/// script. `unverified` means the active role resolves to a host family (or
/// no role pack) and no pack-level declaration can prove coverage either way.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceFontScriptGap {
    pub module_id: String,
    pub role: String,
    pub script: String,
    pub required: bool,
    pub status: String,
    pub current_chain: Vec<String>,
    pub selected_pack: Option<String>,
    pub candidate_packs: Vec<String>,
}

/// An explicit, unapplied alternative chain. The advisor never writes this
/// chain back to settings or the live registry; doing so remains the caller's
/// normal prepare/commit decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceChainSuggestion {
    pub resource: ResourceCoverageKind,
    pub module_id: String,
    pub current_chain: Vec<String>,
    pub suggested_chain: Vec<String>,
    pub added_packs: Vec<String>,
    pub reordered_packs: Vec<String>,
    pub reason: String,
    pub requires_explicit_apply: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceCoverageAdvice {
    pub semantic_gaps: Vec<ResourceSemanticGap>,
    pub font_script_gaps: Vec<ResourceFontScriptGap>,
    pub suggestions: Vec<ResourceChainSuggestion>,
}

impl ResourceCoverageAdvice {
    pub fn has_gaps(&self) -> bool {
        !self.semantic_gaps.is_empty() || !self.font_script_gaps.is_empty()
    }
}

/// Read-only coverage analysis over one effective resource snapshot.
pub struct ResourceCoverageAdvisor<'a> {
    snapshot: &'a ResourceExplanationSnapshot,
}

impl<'a> ResourceCoverageAdvisor<'a> {
    pub fn new(snapshot: &'a ResourceExplanationSnapshot) -> Self {
        Self { snapshot }
    }

    pub fn advise(&self, request: &ResourceCoverageRequest) -> ResourceCoverageAdvice {
        let mut advice = ResourceCoverageAdvice::default();
        self.advise_semantic_names(request, &mut advice);
        self.advise_font_scripts(request, &mut advice);
        advice
    }

    fn advise_semantic_names(
        &self,
        request: &ResourceCoverageRequest,
        advice: &mut ResourceCoverageAdvice,
    ) {
        let icon_packs = ordered_packs(&self.snapshot.icons);
        for need in dedup_semantic_needs(&request.semantic_names) {
            let current_chain = effective_chain(
                &self.snapshot.frontends,
                &need.module_id,
                &self.snapshot.icons,
                true,
            );
            let resolution = self.snapshot.icons.resolutions.iter().find(|resolution| {
                resolution.module_id == need.module_id && resolution.semantic_name == need.name
            });
            let metadata_covered = current_chain.iter().any(|chain_id| {
                pack_for_chain_id(&icon_packs, chain_id)
                    .is_some_and(|pack| semantic_pack_covers(pack, &need.name))
            });
            if resolution.is_some_and(|resolution| resolution.status == "found")
                || (resolution.is_none() && metadata_covered)
            {
                continue;
            }

            let candidate_packs = icon_packs
                .iter()
                .filter(|pack| semantic_pack_covers(pack, &need.name))
                .map(|pack| chain_identifier(pack, &current_chain, ResourceCoverageKind::Icons))
                .collect::<Vec<_>>();
            advice.semantic_gaps.push(ResourceSemanticGap {
                module_id: need.module_id.clone(),
                name: need.name.clone(),
                required: need.required,
                status: resolution
                    .map(|resolution| resolution.status.clone())
                    .unwrap_or_else(|| "unresolved".into()),
                current_chain: current_chain.clone(),
                tried: resolution
                    .map(|resolution| resolution.tried.clone())
                    .unwrap_or_default(),
                candidate_packs: candidate_packs.clone(),
            });

            let mut suggested_chain = current_chain.clone();
            let mut added_packs = Vec::new();
            for candidate in &candidate_packs {
                if !suggested_chain.contains(candidate) {
                    suggested_chain.push(candidate.clone());
                    added_packs.push(candidate.clone());
                }
            }
            if suggested_chain != current_chain {
                advice.suggestions.push(ResourceChainSuggestion {
                    resource: ResourceCoverageKind::Icons,
                    module_id: need.module_id.clone(),
                    current_chain,
                    suggested_chain,
                    added_packs,
                    reordered_packs: Vec::new(),
                    reason: format!(
                        "icon '{}' is missing; append a pack that advertises the semantic name",
                        need.name
                    ),
                    requires_explicit_apply: true,
                });
            }
        }
    }

    fn advise_font_scripts(
        &self,
        request: &ResourceCoverageRequest,
        advice: &mut ResourceCoverageAdvice,
    ) {
        let font_packs = ordered_packs(&self.snapshot.fonts);
        for need in dedup_font_script_needs(&request.font_scripts) {
            let current_chain = effective_chain(
                &self.snapshot.frontends,
                &need.module_id,
                &self.snapshot.fonts,
                false,
            );
            let selected = current_chain.iter().find_map(|chain_id| {
                let pack = pack_for_chain_id(&font_packs, chain_id)?;
                pack.mappings
                    .iter()
                    .any(|mapping| mapping.semantic_name == need.role)
                    .then_some((chain_id, pack))
            });
            if selected.is_some_and(|(_, pack)| pack_covers_script(pack, &need.script)) {
                continue;
            }

            let candidate_packs = font_packs
                .iter()
                .filter(|pack| {
                    pack.mappings
                        .iter()
                        .any(|mapping| mapping.semantic_name == need.role)
                        && pack_covers_script(pack, &need.script)
                })
                .map(|pack| chain_identifier(pack, &current_chain, ResourceCoverageKind::Fonts))
                .collect::<Vec<_>>();
            advice.font_script_gaps.push(ResourceFontScriptGap {
                module_id: need.module_id.clone(),
                role: need.role.clone(),
                script: need.script.clone(),
                required: need.required,
                status: if selected.is_some() {
                    "uncovered".into()
                } else {
                    "unverified".into()
                },
                current_chain: current_chain.clone(),
                selected_pack: selected.map(|(chain_id, _)| chain_id.clone()),
                candidate_packs: candidate_packs.clone(),
            });

            let Some(candidate) = candidate_packs.first() else {
                continue;
            };
            let Some(candidate_pack) = pack_for_chain_id(&font_packs, candidate) else {
                continue;
            };
            let mut suggested_chain = current_chain.clone();
            let mut added_packs = Vec::new();
            if suggested_chain.contains(candidate) {
                suggested_chain.retain(|chain_id| chain_id != candidate);
            } else {
                added_packs.push(candidate.clone());
            }
            let insert_at = suggested_chain
                .iter()
                .position(|chain_id| {
                    pack_for_chain_id(&font_packs, chain_id).is_some_and(|pack| {
                        pack.mappings
                            .iter()
                            .any(|mapping| mapping.semantic_name == need.role)
                    })
                })
                .unwrap_or(suggested_chain.len());
            suggested_chain.insert(insert_at, candidate.clone());
            if suggested_chain == current_chain {
                continue;
            }
            let reordered_packs = current_chain
                .iter()
                .filter(|chain_id| {
                    suggested_chain
                        .iter()
                        .position(|candidate_id| candidate_id == *chain_id)
                        != current_chain
                            .iter()
                            .position(|candidate_id| candidate_id == *chain_id)
                })
                .cloned()
                .collect();
            advice.suggestions.push(ResourceChainSuggestion {
                resource: ResourceCoverageKind::Fonts,
                module_id: need.module_id.clone(),
                current_chain,
                suggested_chain,
                added_packs,
                reordered_packs,
                reason: format!(
                    "font role '{}' has no declared '{}' coverage; place '{}' before the current role winner",
                    need.role,
                    need.script,
                    chain_identifier(candidate_pack, &[], ResourceCoverageKind::Fonts)
                ),
                requires_explicit_apply: true,
            });
        }
    }
}

impl ResourceExplanationSnapshot {
    pub fn advise_coverage(&self, request: &ResourceCoverageRequest) -> ResourceCoverageAdvice {
        ResourceCoverageAdvisor::new(self).advise(request)
    }
}

fn dedup_semantic_needs(needs: &[ResourceSemanticNeed]) -> Vec<ResourceSemanticNeed> {
    let mut deduped = BTreeMap::<(String, String), bool>::new();
    for need in needs {
        deduped
            .entry((need.module_id.clone(), need.name.clone()))
            .and_modify(|required| *required |= need.required)
            .or_insert(need.required);
    }
    deduped
        .into_iter()
        .map(|((module_id, name), required)| ResourceSemanticNeed {
            module_id,
            name,
            required,
        })
        .collect()
}

fn dedup_font_script_needs(needs: &[ResourceFontScriptNeed]) -> Vec<ResourceFontScriptNeed> {
    let mut deduped = BTreeMap::<(String, String, String), bool>::new();
    for need in needs {
        deduped
            .entry((
                need.module_id.clone(),
                need.role.clone(),
                need.script.clone(),
            ))
            .and_modify(|required| *required |= need.required)
            .or_insert(need.required);
    }
    deduped
        .into_iter()
        .map(
            |((module_id, role, script), required)| ResourceFontScriptNeed {
                module_id,
                role,
                script,
                required,
            },
        )
        .collect()
}

fn ordered_packs(chain: &crate::ResourceChainExplanation) -> Vec<&ResourcePackExplanation> {
    let mut packs = chain.chain.iter().collect::<Vec<_>>();
    packs.sort_by_key(|pack| pack.chain_position);
    packs
}

fn effective_chain(
    frontends: &[crate::ResourceFrontendExplanation],
    module_id: &str,
    chain: &crate::ResourceChainExplanation,
    icons: bool,
) -> Vec<String> {
    if let Some(frontend) = frontends
        .iter()
        .find(|frontend| frontend.module_id == module_id)
    {
        return if icons {
            frontend.icon_chain.clone()
        } else {
            frontend.font_chain.clone()
        };
    }
    ordered_packs(chain)
        .into_iter()
        .map(|pack| {
            if icons {
                pack.module_id.clone()
            } else {
                pack.pack_id.clone()
            }
        })
        .collect()
}

fn pack_for_chain_id<'a>(
    packs: &'a [&ResourcePackExplanation],
    chain_id: &str,
) -> Option<&'a ResourcePackExplanation> {
    packs
        .iter()
        .copied()
        .find(|pack| pack.module_id == chain_id || pack.pack_id == chain_id)
}

fn chain_identifier(
    pack: &ResourcePackExplanation,
    current_chain: &[String],
    resource: ResourceCoverageKind,
) -> String {
    if current_chain.iter().any(|id| id == &pack.module_id) {
        return pack.module_id.clone();
    }
    if current_chain.iter().any(|id| id == &pack.pack_id) {
        return pack.pack_id.clone();
    }
    match resource {
        ResourceCoverageKind::Icons => pack.module_id.clone(),
        ResourceCoverageKind::Fonts => pack.pack_id.clone(),
    }
}

fn semantic_pack_covers(pack: &ResourcePackExplanation, name: &str) -> bool {
    semantic_fallback_names(name).iter().any(|candidate| {
        pack.mappings
            .iter()
            .any(|mapping| mapping.semantic_name == *candidate)
    })
}

fn semantic_fallback_names(name: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut candidate = name;
    loop {
        names.push(candidate.to_owned());
        let Some((prefix, _)) = candidate.rsplit_once('-') else {
            break;
        };
        candidate = prefix;
    }
    names
}

fn pack_covers_script(pack: &ResourcePackExplanation, script: &str) -> bool {
    pack.script_coverage
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(script))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ResourceChainExplanation, ResourceFrontendExplanation, ResourceMappingExplanation,
        ResourceResolutionExplanation,
    };

    fn pack(
        module_id: &str,
        pack_id: &str,
        position: usize,
        mappings: &[&str],
        scripts: &[&str],
    ) -> ResourcePackExplanation {
        ResourcePackExplanation {
            module_id: module_id.into(),
            pack_id: pack_id.into(),
            chain_position: position,
            status: "selected".into(),
            assets: Vec::new(),
            mappings: mappings
                .iter()
                .map(|name| ResourceMappingExplanation {
                    semantic_name: (*name).into(),
                    target: (*name).into(),
                    ..Default::default()
                })
                .collect(),
            script_coverage: scripts.iter().map(|script| (*script).into()).collect(),
        }
    }

    #[test]
    fn icon_gap_suggests_append_and_keeps_declared_order() {
        let mut snapshot = ResourceExplanationSnapshot::default();
        snapshot.icons = ResourceChainExplanation {
            chain: vec![
                pack("@mesh/icons-base", "base", 0, &["settings"], &[]),
                pack("@mesh/icons-extra", "extra", 1, &["network-wireless"], &[]),
            ],
            ..Default::default()
        };
        snapshot.frontends = vec![ResourceFrontendExplanation {
            module_id: "@mesh/panel".into(),
            icon_chain: vec!["@mesh/icons-base".into()],
            ..Default::default()
        }];
        snapshot
            .icons
            .resolutions
            .push(ResourceResolutionExplanation {
                module_id: "@mesh/panel".into(),
                semantic_name: "network-wireless-signal-weak".into(),
                status: "missing".into(),
                tried: vec!["@mesh/icons-base".into()],
                ..Default::default()
            });

        let request = ResourceCoverageRequest {
            semantic_names: vec![ResourceSemanticNeed {
                module_id: "@mesh/panel".into(),
                name: "network-wireless-signal-weak".into(),
                required: true,
            }],
            ..Default::default()
        };
        let advice = snapshot.advise_coverage(&request);

        assert_eq!(
            advice.semantic_gaps[0].candidate_packs,
            ["@mesh/icons-extra"]
        );
        assert_eq!(advice.suggestions[0].current_chain, ["@mesh/icons-base"]);
        assert_eq!(
            advice.suggestions[0].suggested_chain,
            ["@mesh/icons-base", "@mesh/icons-extra"]
        );
        assert!(advice.suggestions[0].requires_explicit_apply);
        assert_eq!(snapshot.frontends[0].icon_chain, ["@mesh/icons-base"]);
    }

    #[test]
    fn font_gap_suggests_explicit_reorder_for_first_wins_resolution() {
        let mut snapshot = ResourceExplanationSnapshot::default();
        snapshot.fonts = ResourceChainExplanation {
            chain: vec![
                pack("@mesh/fonts-latin", "latin", 0, &["body"], &["latin"]),
                pack(
                    "@mesh/fonts-cyrillic",
                    "cyrillic",
                    1,
                    &["body"],
                    &["cyrillic"],
                ),
            ],
            ..Default::default()
        };
        snapshot.frontends = vec![ResourceFrontendExplanation {
            module_id: "@mesh/panel".into(),
            font_chain: vec!["latin".into(), "cyrillic".into()],
            ..Default::default()
        }];

        let mut request = ResourceCoverageRequest::default();
        request.add_font_script("@mesh/panel", "body", "cyrillic", true);
        let advice = snapshot.advise_coverage(&request);

        assert_eq!(advice.font_script_gaps[0].status, "uncovered");
        assert_eq!(
            advice.font_script_gaps[0].selected_pack.as_deref(),
            Some("latin")
        );
        assert_eq!(advice.font_script_gaps[0].candidate_packs, ["cyrillic"]);
        assert_eq!(advice.suggestions[0].current_chain, ["latin", "cyrillic"]);
        assert_eq!(advice.suggestions[0].suggested_chain, ["cyrillic", "latin"]);
        assert_eq!(advice.suggestions[0].reordered_packs, ["latin", "cyrillic"]);
        assert!(advice.suggestions[0].requires_explicit_apply);
        assert_eq!(snapshot.frontends[0].font_chain, ["latin", "cyrillic"]);
    }
}
