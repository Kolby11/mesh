use crate::contract::{InterfaceContract, parse_contract_version, parse_version_req};
use semver::Version;
use std::collections::HashMap;
use std::sync::RwLock;

#[derive(Debug, Clone, Default)]
pub struct InterfaceCatalog {
    pub contracts: HashMap<String, Vec<InterfaceContract>>,
    pub providers: HashMap<String, Vec<InterfaceProvider>>,
}

/// Metadata about an available interface provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceProvider {
    pub interface: String,
    pub version: Option<String>,
    pub provider_plugin: String,
    pub backend_name: String,
    pub priority: u32,
}

/// Result of resolving an interface lookup request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceResolution {
    pub requested: String,
    pub requested_version: Option<String>,
    pub contract: Option<InterfaceContract>,
    pub provider: Option<InterfaceProvider>,
}

/// Dynamic registry of named interfaces, their contract metadata, and providers.
#[derive(Debug, Default)]
pub struct InterfaceRegistry {
    contracts: RwLock<HashMap<String, Vec<InterfaceContract>>>,
    providers: RwLock<HashMap<String, Vec<InterfaceProvider>>>,
}

impl InterfaceRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_contract(&self, contract: InterfaceContract) {
        let mut contracts = self.contracts.write().unwrap();
        register_contract_in_map(&mut contracts, contract);
    }

    pub fn register(&self, provider: InterfaceProvider) {
        let mut providers = self.providers.write().unwrap();
        register_provider_in_map(&mut providers, provider);
    }

    pub fn resolve(&self, requested: &str, requested_version: Option<&str>) -> InterfaceResolution {
        self.catalog().resolve(requested, requested_version)
    }

    pub fn providers_for(&self, requested: &str) -> Vec<InterfaceProvider> {
        let canonical = canonical_interface_name(requested);
        self.providers
            .read()
            .unwrap()
            .get(&canonical)
            .cloned()
            .unwrap_or_default()
    }

    pub fn contracts_for(&self, requested: &str) -> Vec<InterfaceContract> {
        let canonical = canonical_interface_name(requested);
        self.contracts
            .read()
            .unwrap()
            .get(&canonical)
            .cloned()
            .unwrap_or_default()
    }

    pub fn list_interfaces(&self) -> Vec<String> {
        self.catalog().list_interfaces()
    }

    pub fn has(&self, requested: &str) -> bool {
        self.catalog().has(requested)
    }

    pub fn catalog(&self) -> InterfaceCatalog {
        InterfaceCatalog {
            contracts: self.contracts.read().unwrap().clone(),
            providers: self.providers.read().unwrap().clone(),
        }
    }
}

/// Normalize docs-era interface naming.
pub fn canonical_interface_name(name: &str) -> String {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    if trimmed.contains('.') {
        trimmed.to_string()
    } else {
        format!("mesh.{trimmed}")
    }
}

impl InterfaceCatalog {
    pub fn register_contract(&mut self, contract: InterfaceContract) {
        register_contract_in_map(&mut self.contracts, contract);
    }

    pub fn register_provider(&mut self, provider: InterfaceProvider) {
        register_provider_in_map(&mut self.providers, provider);
    }

    pub fn resolve(&self, requested: &str, requested_version: Option<&str>) -> InterfaceResolution {
        let canonical = canonical_interface_name(requested);
        let contract = self.contracts.get(&canonical).and_then(|contracts| {
            contracts
                .iter()
                .find(|contract| version_matches_contract(requested_version, &contract.version))
                .cloned()
        });
        let provider = self.providers.get(&canonical).and_then(|providers| {
            providers
                .iter()
                .find(|provider| {
                    version_matches_provider(
                        requested_version,
                        provider.version.as_deref(),
                        contract.as_ref(),
                    )
                })
                .cloned()
        });

        InterfaceResolution {
            requested: canonical,
            requested_version: requested_version.map(ToOwned::to_owned),
            contract,
            provider,
        }
    }

    pub fn list_interfaces(&self) -> Vec<String> {
        let mut names = Vec::new();
        names.extend(self.contracts.keys().cloned());
        names.extend(self.providers.keys().cloned());
        names.sort();
        names.dedup();
        names
    }

    pub fn has(&self, requested: &str) -> bool {
        let canonical = canonical_interface_name(requested);
        self.contracts.contains_key(&canonical) || self.providers.contains_key(&canonical)
    }
}

fn register_contract_in_map(
    contracts: &mut HashMap<String, Vec<InterfaceContract>>,
    contract: InterfaceContract,
) {
    let entry = contracts.entry(contract.interface.clone()).or_default();
    entry.retain(|existing| existing.version != contract.version);
    entry.push(contract);
    entry.sort_by(|a, b| b.version.cmp(&a.version));
}

fn register_provider_in_map(
    providers: &mut HashMap<String, Vec<InterfaceProvider>>,
    provider: InterfaceProvider,
) {
    let entry = providers.entry(provider.interface.clone()).or_default();
    entry.retain(|existing| {
        !(existing.provider_plugin == provider.provider_plugin
            && existing.version == provider.version)
    });
    entry.push(provider);
    entry.sort_by(|a, b| {
        b.priority
            .cmp(&a.priority)
            .then_with(|| a.provider_plugin.cmp(&b.provider_plugin))
    });
}

fn version_matches_contract(requested: Option<&str>, contract_version: &Version) -> bool {
    let Some(requested) = requested.map(str::trim).filter(|value| !value.is_empty()) else {
        return true;
    };

    parse_version_req(requested)
        .map(|req| req.matches(contract_version))
        .unwrap_or(false)
}

fn version_matches_provider(
    requested: Option<&str>,
    provider_version: Option<&str>,
    contract: Option<&InterfaceContract>,
) -> bool {
    let Some(requested) = requested.map(str::trim).filter(|value| !value.is_empty()) else {
        return true;
    };

    if let Some(provider_version) = provider_version.and_then(parse_contract_version) {
        return parse_version_req(requested)
            .map(|req| req.matches(&provider_version))
            .unwrap_or(false);
    }

    if let Some(contract) = contract {
        return version_matches_contract(Some(requested), &contract.version);
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{ContractCapabilities, InterfaceEvent, InterfaceMethod};
    use std::path::PathBuf;

    #[test]
    fn canonicalizes_short_names() {
        assert_eq!(canonical_interface_name("audio"), "mesh.audio");
        assert_eq!(canonical_interface_name("mesh.audio"), "mesh.audio");
        assert_eq!(canonical_interface_name("alice.thermal"), "alice.thermal");
    }

    #[test]
    fn resolves_highest_priority_provider_for_contract() {
        let registry = InterfaceRegistry::new();
        registry.register_contract(InterfaceContract {
            interface: "mesh.audio".into(),
            version: Version::parse("1.0.0").unwrap(),
            file_path: PathBuf::from("interface.toml"),
            methods: vec![InterfaceMethod {
                name: "default_output".into(),
                args: Vec::new(),
                returns: Some("Device?".into()),
            }],
            events: vec![InterfaceEvent {
                name: "VolumeChanged".into(),
                payload: Some("{ device_id: string, level: float }".into()),
            }],
            types: HashMap::new(),
            capabilities: ContractCapabilities::default(),
        });
        registry.register(InterfaceProvider {
            interface: "mesh.audio".into(),
            version: Some("1.0".into()),
            provider_plugin: "@mesh/pulseaudio-audio".into(),
            backend_name: "PulseAudio".into(),
            priority: 50,
        });
        registry.register(InterfaceProvider {
            interface: "mesh.audio".into(),
            version: Some("1.0".into()),
            provider_plugin: "@mesh/pipewire-audio".into(),
            backend_name: "PipeWire".into(),
            priority: 100,
        });

        let resolved = registry.resolve("audio", Some(">=1.0"));
        assert_eq!(resolved.contract.unwrap().version.to_string(), "1.0.0");
        assert_eq!(
            resolved.provider.unwrap().provider_plugin,
            "@mesh/pipewire-audio"
        );
    }

    #[test]
    fn falls_back_to_contract_version_when_provider_omits_one() {
        let registry = InterfaceRegistry::new();
        registry.register_contract(InterfaceContract {
            interface: "alice.thermal".into(),
            version: Version::parse("1.0.0").unwrap(),
            file_path: PathBuf::from("interface.toml"),
            methods: Vec::new(),
            events: Vec::new(),
            types: HashMap::new(),
            capabilities: ContractCapabilities::default(),
        });
        registry.register(InterfaceProvider {
            interface: "alice.thermal".into(),
            version: None,
            provider_plugin: "@alice/lmsensors".into(),
            backend_name: "lm-sensors".into(),
            priority: 100,
        });

        let resolved = registry.resolve("alice.thermal", Some(">=1.0"));
        assert_eq!(
            resolved.provider.unwrap().provider_plugin,
            "@alice/lmsensors"
        );
    }
}
