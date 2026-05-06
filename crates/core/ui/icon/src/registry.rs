use crate::config::{IconConfig, IconPackRoot};
use crate::xdg;
use anyhow::{Result, bail};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IconResolution {
    Found {
        semantic_name: String,
        candidate: String,
        path: PathBuf,
        multicolor: bool,
    },
    Missing {
        semantic_name: String,
        tried: Vec<String>,
    },
}

#[derive(Debug, Clone)]
pub struct IconRegistry {
    config: IconConfig,
    generation: u64,
    cache: HashMap<(u64, String, u32), IconResolution>,
}

impl IconRegistry {
    pub fn from_config(config: IconConfig) -> Result<Self> {
        config.validate()?;
        Ok(Self {
            config,
            generation: 0,
            cache: HashMap::new(),
        })
    }

    pub fn set_config(&mut self, config: IconConfig) -> Result<()> {
        config.validate()?;
        self.config = config;
        self.generation = self.generation.saturating_add(1);
        self.cache.clear();
        Ok(())
    }

    /// Register an additional icon pack on top of the current config.
    /// Returns `Ok(false)` if a pack with the same id is already registered;
    /// callers can treat that as a no-op. Bumps generation and invalidates
    /// the resolution cache when a new pack is added.
    pub fn register_pack(&mut self, pack: IconPackRoot) -> Result<bool> {
        if pack.id.trim().is_empty() {
            bail!("pack id must not be empty");
        }
        if self.config.pack(&pack.id).is_some() {
            return Ok(false);
        }
        if let Some(root) = &pack.root
            && root.as_os_str().is_empty()
        {
            bail!("pack '{}' root must not be empty", pack.id);
        }
        if pack.theme.trim().is_empty() {
            bail!("pack '{}' theme must not be empty", pack.id);
        }
        self.config.packs.push(pack);
        self.generation = self.generation.saturating_add(1);
        self.cache.clear();
        Ok(true)
    }

    pub fn resolve(&mut self, semantic_name: &str, size: u32) -> IconResolution {
        let key = (self.generation, semantic_name.to_string(), size);
        if let Some(cached) = self.cache.get(&key) {
            return cached.clone();
        }

        let resolution = self.resolve_uncached(semantic_name, size);
        self.cache.insert(key, resolution.clone());
        resolution
    }

    fn resolve_uncached(&self, semantic_name: &str, size: u32) -> IconResolution {
        let mut tried = Vec::new();
        let Some(candidates) = self.config.active_profile().icons.get(semantic_name) else {
            return IconResolution::Missing {
                semantic_name: semantic_name.into(),
                tried,
            };
        };

        for candidate in candidates {
            let mapping = candidate.as_mapping();
            tried.push(mapping.clone());
            let Some(pack) = self.config.pack(&candidate.pack_id) else {
                continue;
            };
            if let Some(path) = xdg::find_icon_in_pack(pack, &candidate.asset_name, size) {
                return IconResolution::Found {
                    semantic_name: semantic_name.into(),
                    candidate: mapping,
                    path,
                    multicolor: candidate.multicolor,
                };
            }
        }

        IconResolution::Missing {
            semantic_name: semantic_name.into(),
            tried,
        }
    }
}

impl IconResolution {
    pub fn path(&self) -> Option<PathBuf> {
        match self {
            Self::Found { path, .. } => Some(path.clone()),
            Self::Missing { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_xdg_config() -> IconConfig {
        IconConfig::from_toml_str(
            r#"
active_profile = "system"

[[packs]]
id = "system"
theme = "hicolor"

[profiles.system.icons]
nothing = ["system:nothing"]
"#,
        )
        .unwrap()
    }

    #[test]
    fn register_pack_adds_new_pack_and_invalidates_cache() {
        let mut registry = IconRegistry::from_config(empty_xdg_config()).unwrap();
        // Force a cached resolution before registering — must be flushed.
        let _ = registry.resolve("nothing", 24);
        let before_gen = registry.generation;

        let added = registry
            .register_pack(IconPackRoot {
                id: "@mesh/extra".into(),
                root: Some(PathBuf::from("/tmp/mesh-icons")),
                theme: "hicolor".into(),
                kind: crate::IconPackKind::Xdg,
            })
            .unwrap();

        assert!(added);
        assert!(registry.config.pack("@mesh/extra").is_some());
        assert!(registry.generation > before_gen);
        assert!(registry.cache.is_empty());
    }

    #[test]
    fn register_pack_is_idempotent_on_duplicate_id() {
        let mut registry = IconRegistry::from_config(empty_xdg_config()).unwrap();
        let added_again = registry
            .register_pack(IconPackRoot {
                id: "system".into(),
                root: None,
                theme: "hicolor".into(),
                kind: crate::IconPackKind::Xdg,
            })
            .unwrap();
        assert!(!added_again, "duplicate id should be a no-op");
    }

    #[test]
    fn register_pack_rejects_empty_id() {
        let mut registry = IconRegistry::from_config(empty_xdg_config()).unwrap();
        let err = registry
            .register_pack(IconPackRoot {
                id: "  ".into(),
                root: None,
                theme: "hicolor".into(),
                kind: crate::IconPackKind::Xdg,
            })
            .unwrap_err();
        assert!(err.to_string().contains("pack id"));
    }
}
