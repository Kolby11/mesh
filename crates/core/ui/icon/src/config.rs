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
    pub root: PathBuf,
    #[serde(default = "default_hicolor")]
    pub theme: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct IconProfile {
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

    pub fn builtin_material(root: PathBuf) -> Result<Self> {
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
            ("settings", vec!["settings"]),
            ("weather-clear-night", vec!["star", "warning"]),
            ("weather-clear", vec!["star", "warning"]),
            ("battery-empty", vec!["warning", "battery-80"]),
            ("battery-caution", vec!["warning", "battery-80"]),
            ("battery-low", vec!["battery-80", "warning"]),
            ("battery-good", vec!["battery-80", "battery-full"]),
            ("battery-full", vec!["battery-full"]),
            ("battery-80", vec!["battery-80"]),
            ("close", vec!["close"]),
            ("star", vec!["star"]),
            ("warning", vec!["warning"]),
            ("wifi", vec!["wifi"]),
            ("volume", vec!["volume"]),
            ("volume-off", vec!["volume-off"]),
        ] {
            profile.icons.insert(
                name.into(),
                assets
                    .into_iter()
                    .map(|asset_name| IconCandidate {
                        pack_id: "material".into(),
                        asset_name: asset_name.into(),
                        multicolor: false,
                    })
                    .collect(),
            );
        }

        let config = Self {
            active_profile: "material".into(),
            packs: vec![IconPackRoot {
                id: "material".into(),
                root,
                theme: "hicolor".into(),
            }],
            profiles: HashMap::from([("material".into(), profile)]),
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
            if pack.root.as_os_str().is_empty() {
                bail!("pack '{}' root must not be empty", pack.id);
            }
        }

        for (profile_id, profile) in &self.profiles {
            if profile_id.trim().is_empty() {
                bail!("profile id must not be empty");
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
