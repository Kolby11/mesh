use anyhow::{Result, bail};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize)]
pub struct IconConfig {
    pub active_profile: String,
    #[serde(default)]
    pub packs: Vec<IconPackRoot>,
    #[serde(default)]
    pub profiles: HashMap<String, IconProfile>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IconPackRoot {
    pub id: String,
    #[serde(default)]
    pub root: Option<PathBuf>,
    #[serde(default = "default_hicolor")]
    pub theme: String,
    #[serde(default)]
    pub kind: IconPackKind,
}

/// What kind of icon source backs this pack.
///
/// `Xdg` packs follow the freedesktop icon-theme spec (files laid out as
/// `<root>/<theme>/<size>/<category>/<name>.svg`). `Font` packs are a font
/// file plus a `name -> codepoint` mapping table; resolution returns a
/// glyph reference instead of a file path. Font rendering still needs to
/// land in the painter — for now the data model is wired so authors can
/// declare font packs and the registry will track them.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IconPackKind {
    #[default]
    Xdg,
    Font {
        /// Path to the font file, relative to the pack root.
        font_file: String,
        /// Path to the JSON glyph map (`{ "asset-name": "", ... }`)
        /// relative to the pack root.
        glyph_map: String,
    },
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct IconProfile {
    /// Optional discipline hint: when set, `validate()` warns if any
    /// candidate list's first entry is not from this pack. Encourages
    /// "one profile = one visual style"; cross-pack fallbacks remain
    /// allowed for missing-icon handling.
    #[serde(default)]
    pub primary_pack: Option<String>,
    #[serde(default)]
    pub icons: HashMap<String, Vec<IconCandidate>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IconCandidate {
    pub pack_id: String,
    pub asset_name: String,
    pub multicolor: bool,
}

fn default_hicolor() -> String {
    "hicolor".into()
}

impl IconConfig {
    pub fn from_toml_str(input: &str) -> Result<Self> {
        let config: Self = toml::from_str(input)?;
        config.validate()?;
        Ok(config)
    }

    /// Build the default icon config. Resolves semantic names through the
    /// installed XDG icon themes only — no bundled assets. Each candidate
    /// list tries the canonical XDG name plus its `-symbolic` and `-panel`
    /// variants where applicable, with sensible fallbacks (e.g. volume
    /// icons fall back to `volume`/`volume-off`).
    pub fn builtin_xdg() -> Result<Self> {
        let mut profile = IconProfile::default();
        for (name, assets) in [
            ("audio-volume-high", vec!["audio-volume-high", "volume"]),
            ("audio-volume-medium", vec!["audio-volume-medium", "volume"]),
            ("audio-volume-low", vec!["audio-volume-low", "volume"]),
            (
                "audio-volume-muted",
                vec!["audio-volume-muted", "volume-off"],
            ),
            ("network-wireless", vec!["network-wireless", "wifi"]),
            ("bluetooth", vec!["bluetooth"]),
            ("settings", vec!["preferences-system", "settings"]),
            (
                "weather-clear-night",
                vec!["weather-clear-night", "weather-clear"],
            ),
            ("weather-clear", vec!["weather-clear"]),
            ("battery-empty", vec!["battery-empty", "battery-low"]),
            ("battery-caution", vec!["battery-caution", "battery-low"]),
            ("battery-low", vec!["battery-low"]),
            ("battery-good", vec!["battery-good", "battery"]),
            ("battery-full", vec!["battery-full", "battery"]),
            ("close", vec!["window-close", "close"]),
            ("star", vec!["starred", "star"]),
            ("warning", vec!["dialog-warning", "warning"]),
            ("wifi", vec!["network-wireless", "wifi"]),
            ("volume", vec!["audio-volume-high", "volume"]),
            ("volume-off", vec!["audio-volume-muted", "volume-off"]),
        ] {
            profile.icons.insert(
                name.into(),
                assets
                    .into_iter()
                    .flat_map(|asset_name| {
                        [
                            IconCandidate {
                                pack_id: "system".into(),
                                asset_name: asset_name.into(),
                                multicolor: false,
                            },
                            IconCandidate {
                                pack_id: "system".into(),
                                asset_name: format!("{asset_name}-symbolic"),
                                multicolor: false,
                            },
                        ]
                    })
                    .collect(),
            );
        }

        let config = Self {
            active_profile: "system".into(),
            packs: vec![IconPackRoot {
                id: "system".into(),
                root: None,
                theme: "hicolor".into(),
                kind: IconPackKind::Xdg,
            }],
            profiles: HashMap::from([("system".into(), profile)]),
        };
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        if self.active_profile.trim().is_empty() {
            bail!("active_profile must not be empty");
        }
        if !self.profiles.contains_key(&self.active_profile) {
            bail!("active_profile '{}' does not exist", self.active_profile);
        }

        let mut pack_ids = HashSet::new();
        for pack in &self.packs {
            if pack.id.trim().is_empty() {
                bail!("pack id must not be empty");
            }
            if !pack_ids.insert(pack.id.clone()) {
                bail!("duplicate pack id '{}'", pack.id);
            }
            if let Some(root) = &pack.root
                && root.as_os_str().is_empty()
            {
                bail!("pack '{}' root must not be empty", pack.id);
            }
            if pack.theme.trim().is_empty() {
                bail!("pack '{}' theme must not be empty", pack.id);
            }
            if let IconPackKind::Font {
                font_file,
                glyph_map,
            } = &pack.kind
            {
                if font_file.trim().is_empty() {
                    bail!("font pack '{}' font_file must not be empty", pack.id);
                }
                if glyph_map.trim().is_empty() {
                    bail!("font pack '{}' glyph_map must not be empty", pack.id);
                }
                if pack.root.is_none() {
                    bail!(
                        "font pack '{}' must declare a root containing the font file",
                        pack.id
                    );
                }
            }
        }

        for (profile_id, profile) in &self.profiles {
            if profile_id.trim().is_empty() {
                bail!("profile id must not be empty");
            }
            if let Some(primary) = profile.primary_pack.as_deref()
                && !pack_ids.contains(primary)
            {
                bail!(
                    "profile '{}' primary_pack '{}' references unknown pack",
                    profile_id,
                    primary
                );
            }
            for (semantic_name, candidates) in &profile.icons {
                if semantic_name.trim().is_empty() {
                    bail!("semantic icon name must not be empty");
                }
                if candidates.is_empty() {
                    bail!("semantic icon '{}' has no candidates", semantic_name);
                }
                for candidate in candidates {
                    if !pack_ids.contains(&candidate.pack_id) {
                        bail!(
                            "candidate '{}' references unknown pack '{}'",
                            candidate.as_mapping(),
                            candidate.pack_id
                        );
                    }
                    if candidate.asset_name.trim().is_empty() {
                        bail!("candidate asset name must not be empty");
                    }
                }
                // Soft consistency check: warn if the *first* candidate isn't
                // from the profile's declared primary pack. Cross-pack
                // fallbacks remain allowed; this just nudges authors toward
                // a single visual style as the default resolution path.
                if let Some(primary) = profile.primary_pack.as_deref()
                    && let Some(first) = candidates.first()
                    && first.pack_id != primary
                {
                    tracing::warn!(
                        "profile '{}' icon '{}' resolves to '{}' before primary pack '{}'; \
                         consider listing a '{}:*' candidate first to keep the look consistent",
                        profile_id,
                        semantic_name,
                        first.as_mapping(),
                        primary,
                        primary,
                    );
                }
            }
        }

        Ok(())
    }

    pub fn pack(&self, pack_id: &str) -> Option<&IconPackRoot> {
        self.packs.iter().find(|pack| pack.id == pack_id)
    }

    pub fn active_profile(&self) -> &IconProfile {
        self.profiles
            .get(&self.active_profile)
            .expect("IconConfig is validated")
    }
}

impl IconCandidate {
    pub fn as_mapping(&self) -> String {
        format!("{}:{}", self.pack_id, self.asset_name)
    }
}

impl<'de> Deserialize<'de> for IconCandidate {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        let (candidate, multicolor) = raw
            .strip_suffix("?multicolor")
            .map(|candidate| (candidate, true))
            .unwrap_or((raw.as_str(), false));
        let (pack_id, asset_name) = candidate.split_once(':').ok_or_else(|| {
            serde::de::Error::custom("icon candidate must use pack_id:asset_name format")
        })?;
        if pack_id.is_empty() || asset_name.is_empty() {
            return Err(serde::de::Error::custom(
                "icon candidate pack_id and asset_name must be non-empty",
            ));
        }
        Ok(Self {
            pack_id: pack_id.into(),
            asset_name: asset_name.into(),
            multicolor,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_config_parses_active_profile_packs_and_ordered_fallbacks() {
        let config = IconConfig::from_toml_str(
            r#"
active_profile = "rounded"

[[packs]]
id = "material"
root = "crates/core/ui/icon/assets/material"
theme = "hicolor"

[profiles.rounded.icons]
audio-volume-muted = ["material:audio-volume-muted", "material:volume-off"]
"#,
        )
        .unwrap();

        assert_eq!(config.active_profile, "rounded");
        assert_eq!(config.packs[0].id, "material");
        let candidates = &config.profiles["rounded"].icons["audio-volume-muted"];
        assert_eq!(candidates[0].as_mapping(), "material:audio-volume-muted");
        assert_eq!(candidates[1].as_mapping(), "material:volume-off");
    }

    #[test]
    fn icon_config_rejects_missing_active_profile() {
        let err = IconConfig::from_toml_str(
            r#"
active_profile = "missing"

[[packs]]
id = "material"
root = "crates/core/ui/icon/assets/material"

[profiles.rounded.icons]
audio-volume-muted = ["material:audio-volume-muted"]
"#,
        )
        .unwrap_err();

        assert!(err.to_string().contains("active_profile"));
    }

    #[test]
    fn icon_config_rejects_duplicate_pack_ids() {
        let err = IconConfig::from_toml_str(
            r#"
active_profile = "rounded"

[[packs]]
id = "material"
root = "one"

[[packs]]
id = "material"
root = "two"

[profiles.rounded.icons]
audio-volume-muted = ["material:audio-volume-muted"]
"#,
        )
        .unwrap_err();

        assert!(err.to_string().contains("duplicate pack id"));
    }

    #[test]
    fn icon_config_rejects_unknown_candidate_pack() {
        let err = IconConfig::from_toml_str(
            r#"
active_profile = "rounded"

[[packs]]
id = "material"
root = "crates/core/ui/icon/assets/material"

[profiles.rounded.icons]
audio-volume-muted = ["lucide:volume-x"]
"#,
        )
        .unwrap_err();

        assert!(err.to_string().contains("unknown pack"));
    }
}
