use crate::config::IconConfig;
use crate::xdg;
use anyhow::Result;
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
