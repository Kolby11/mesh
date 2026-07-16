use crate::contract::{InterfaceContract, parse_contract_version, parse_version_req};
use semver::Version;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

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
    pub base_module: Option<String>,
    pub provider_module: String,
    pub backend_name: String,
    pub priority: u32,
}

/// Result of resolving an interface lookup request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceResolution {
    pub requested: String,
    pub requested_version: Option<String>,
    pub contract: Option<Arc<InterfaceContract>>,
    pub provider: Option<InterfaceProvider>,
}

/// Dynamic registry of named interfaces, their contract metadata, and providers.
#[derive(Debug, Default)]
pub struct InterfaceRegistry {
    contracts: RwLock<HashMap<String, Vec<Arc<InterfaceContract>>>>,
    providers: RwLock<HashMap<String, Vec<InterfaceProvider>>>,
}

impl InterfaceRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_contract(&self, contract: InterfaceContract) {
        let mut contracts = self.contracts.write().unwrap();
        let entry = contracts.entry(contract.interface.clone()).or_default();
        entry.retain(|existing| existing.version != contract.version);
        entry.push(Arc::new(contract));
        entry.sort_by(|a, b| b.version.cmp(&a.version));
    }

    pub fn register(&self, provider: InterfaceProvider) {
        let mut providers = self.providers.write().unwrap();
        register_provider_in_map(&mut providers, provider);
    }

    pub fn resolve(&self, requested: &str, requested_version: Option<&str>) -> InterfaceResolution {
        let canonical = canonical_interface_name(requested);
        let contract = self
            .contracts
            .read()
            .unwrap()
            .get(&canonical)
            .and_then(|contracts| {
                contracts
                    .iter()
                    .find(|contract| version_matches_contract(requested_version, &contract.version))
                    .cloned()
            });
        let provider = self
            .providers
            .read()
            .unwrap()
            .get(&canonical)
            .and_then(|providers| {
                providers
                    .iter()
                    .find(|provider| {
                        version_matches_provider(
                            requested_version,
                            provider.version.as_deref(),
                            contract.as_deref(),
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
            .map(|contracts| {
                contracts
                    .iter()
                    .map(|contract| (**contract).clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn list_interfaces(&self) -> Vec<String> {
        let contracts = self.contracts.read().unwrap();
        let providers = self.providers.read().unwrap();
        let mut names = Vec::with_capacity(contracts.len() + providers.len());
        names.extend(contracts.keys().cloned());
        names.extend(providers.keys().cloned());
        names.sort();
        names.dedup();
        names
    }

    pub fn has(&self, requested: &str) -> bool {
        let canonical = canonical_interface_name(requested);
        self.contracts.read().unwrap().contains_key(&canonical)
            || self.providers.read().unwrap().contains_key(&canonical)
    }

    pub fn catalog(&self) -> InterfaceCatalog {
        InterfaceCatalog {
            contracts: self
                .contracts
                .read()
                .unwrap()
                .iter()
                .map(|(name, contracts)| {
                    (
                        name.clone(),
                        contracts
                            .iter()
                            .map(|contract| (**contract).clone())
                            .collect(),
                    )
                })
                .collect(),
            providers: self.providers.read().unwrap().clone(),
        }
    }
}

/// Normalize docs-era interface naming.
pub fn canonical_interface_name(name: &str) -> String {
    canonical_interface_name_cow(name).into_owned()
}

/// Normalize docs-era interface naming while reusing already-owned canonical
/// names. This avoids allocating on runtime paths that receive `mesh.*`
/// service names from prior normalization.
pub fn canonical_interface_name_owned(name: String) -> String {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    if trimmed.contains('.') {
        if trimmed.len() == name.len() {
            name
        } else {
            trimmed.to_owned()
        }
    } else {
        format!("mesh.{trimmed}")
    }
}

/// Borrow an already-canonical interface name and allocate only for short
/// aliases that need the `mesh.` prefix.
pub fn canonical_interface_name_cow(name: &str) -> Cow<'_, str> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Cow::Borrowed("");
    }

    if trimmed.contains('.') {
        Cow::Borrowed(trimmed)
    } else {
        Cow::Owned(format!("mesh.{trimmed}"))
    }
}

/// Convert a canonical interface name such as `mesh.audio` into the short
/// service state key/capability segment used by runtime APIs.
pub fn service_name_from_interface(interface: &str) -> String {
    service_name_from_interface_cow(interface).into_owned()
}

/// Borrow the short service segment from an interface name.
pub fn service_name_from_interface_cow(interface: &str) -> Cow<'_, str> {
    Cow::Borrowed(interface.strip_prefix("mesh.").unwrap_or(interface))
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
                .map(Arc::new)
        });
        let provider = self.providers.get(&canonical).and_then(|providers| {
            providers
                .iter()
                .find(|provider| {
                    version_matches_provider(
                        requested_version,
                        provider.version.as_deref(),
                        contract.as_deref(),
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
        !(existing.provider_module == provider.provider_module
            && existing.version == provider.version)
    });
    entry.push(provider);
    entry.sort_by(|a, b| {
        b.priority
            .cmp(&a.priority)
            .then_with(|| a.provider_module.cmp(&b.provider_module))
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
    use crate::contract::{
        ContractCapabilities, InterfaceArgument, InterfaceEvent, InterfaceMethod,
    };

    fn test_contract(interface: &str, method_count: usize) -> InterfaceContract {
        InterfaceContract {
            interface: interface.into(),
            version: Version::parse("1.0.0").unwrap(),
            state_fields: vec![],
            methods: (0..method_count)
                .map(|index| InterfaceMethod {
                    name: format!("method_{index}"),
                    args: Vec::new(),
                    returns: None,
                    coalesce: false,
                    optimistic: None,
                })
                .collect(),
            events: vec![],
            types: HashMap::new(),
            capabilities: ContractCapabilities::default(),
        }
    }

    #[test]
    fn canonicalizes_short_names() {
        assert_eq!(canonical_interface_name("audio"), "mesh.audio");
        assert_eq!(canonical_interface_name("mesh.audio"), "mesh.audio");
        assert_eq!(canonical_interface_name("alice.thermal"), "alice.thermal");
    }

    #[test]
    fn owned_canonicalization_preserves_normalization_semantics() {
        assert_eq!(
            canonical_interface_name_owned("mesh.audio".to_string()),
            "mesh.audio"
        );
        assert_eq!(
            canonical_interface_name_owned(" audio ".to_string()),
            "mesh.audio"
        );
        assert_eq!(
            canonical_interface_name_owned(" alice.thermal ".to_string()),
            "alice.thermal"
        );
        assert_eq!(canonical_interface_name_owned("   ".to_string()), "");
    }

    #[test]
    fn resolves_highest_priority_provider_for_contract() {
        let registry = InterfaceRegistry::new();
        registry.register_contract(InterfaceContract {
            interface: "mesh.audio".into(),
            version: Version::parse("1.0.0").unwrap(),
            state_fields: vec![],
            methods: vec![InterfaceMethod {
                name: "volume_up".into(),
                args: Vec::new(),
                returns: None,
                coalesce: false,
                optimistic: None,
            }],
            events: vec![InterfaceEvent {
                name: "VolumeChanged".into(),
                payload: vec![
                    InterfaceArgument {
                        name: "device_id".into(),
                        arg_type: "string".into(),
                    },
                    InterfaceArgument {
                        name: "level".into(),
                        arg_type: "float".into(),
                    },
                ],
            }],
            types: HashMap::new(),
            capabilities: ContractCapabilities::default(),
        });
        registry.register(InterfaceProvider {
            interface: "mesh.audio".into(),
            version: Some("1.0".into()),
            base_module: Some("@mesh/audio-interface".into()),
            provider_module: "@mesh/pulseaudio-audio".into(),
            backend_name: "PulseAudio".into(),
            priority: 50,
        });
        registry.register(InterfaceProvider {
            interface: "mesh.audio".into(),
            version: Some("1.0".into()),
            base_module: Some("@mesh/audio-interface".into()),
            provider_module: "@mesh/pipewire-audio".into(),
            backend_name: "PipeWire".into(),
            priority: 100,
        });

        let resolved = registry.resolve("audio", Some(">=1.0"));
        assert_eq!(resolved.contract.unwrap().version.to_string(), "1.0.0");
        assert_eq!(
            resolved.provider.unwrap().provider_module,
            "@mesh/pipewire-audio"
        );
    }

    #[test]
    fn repeated_registry_resolution_shares_contract_storage() {
        let registry = InterfaceRegistry::new();
        registry.register_contract(test_contract("mesh.audio", 8));

        let first = registry.resolve("audio", None).contract.unwrap();
        let second = registry.resolve("audio", None).contract.unwrap();

        assert!(Arc::ptr_eq(&first, &second));
    }

    #[test]
    #[ignore = "release-only performance benchmark"]
    fn benchmark_direct_registry_resolution_against_catalog_snapshot() {
        use std::hint::black_box;
        use std::time::Instant;

        let registry = InterfaceRegistry::new();
        for index in 0..64 {
            let interface = format!("mesh.service_{index}");
            registry.register_contract(test_contract(&interface, 32));
            registry.register(InterfaceProvider {
                interface,
                version: Some("1.0".into()),
                base_module: None,
                provider_module: format!("@mesh/provider_{index}"),
                backend_name: format!("Provider {index}"),
                priority: 100,
            });
        }

        let iterations = 10_000;
        let started = Instant::now();
        for _ in 0..iterations {
            black_box(registry.catalog().resolve("mesh.service_31", None));
        }
        let snapshot_elapsed = started.elapsed();

        let started = Instant::now();
        for _ in 0..iterations {
            black_box(registry.resolve("mesh.service_31", None));
        }
        let direct_elapsed = started.elapsed();

        eprintln!(
            "interface resolution over {iterations} iterations: catalog snapshot {snapshot_elapsed:?}, direct registry {direct_elapsed:?}"
        );
    }

    #[test]
    #[ignore = "release-only performance benchmark"]
    fn benchmark_shared_catalog_clone_against_deep_clone() {
        use std::hint::black_box;
        use std::time::Instant;

        let mut catalog = InterfaceCatalog::default();
        for index in 0..64 {
            let interface = format!("mesh.service_{index}");
            catalog.register_contract(test_contract(&interface, 32));
            catalog.register_provider(InterfaceProvider {
                interface,
                version: Some("1.0".into()),
                base_module: None,
                provider_module: format!("@mesh/provider_{index}"),
                backend_name: format!("Provider {index}"),
                priority: 100,
            });
        }
        let shared = Arc::new(catalog.clone());
        let iterations = 10_000;

        let started = Instant::now();
        for _ in 0..iterations {
            black_box(catalog.clone());
        }
        let deep_clone_elapsed = started.elapsed();

        let started = Instant::now();
        for _ in 0..iterations {
            black_box(Arc::clone(&shared));
        }
        let shared_clone_elapsed = started.elapsed();

        eprintln!(
            "interface catalog clone over {iterations} iterations: deep {deep_clone_elapsed:?}, shared {shared_clone_elapsed:?}"
        );
    }

    #[test]
    fn preserves_provider_base_interface_metadata_in_catalog() {
        let mut catalog = InterfaceCatalog::default();
        catalog.register_provider(InterfaceProvider {
            interface: "mesh.network".into(),
            version: Some("1.0".into()),
            base_module: Some("@mesh/network-interface".into()),
            provider_module: "@mesh/networkmanager".into(),
            backend_name: "NetworkManager".into(),
            priority: 100,
        });

        let resolved = catalog.resolve("network", Some(">=1.0"));
        assert_eq!(
            resolved.provider.unwrap().base_module.as_deref(),
            Some("@mesh/network-interface")
        );
    }

    #[test]
    fn service_name_strips_mesh_prefix_only() {
        assert_eq!(service_name_from_interface("mesh.audio"), "audio");
        assert_eq!(service_name_from_interface("audio"), "audio");
        assert_eq!(
            service_name_from_interface("alice.thermal"),
            "alice.thermal"
        );
    }

    #[test]
    fn falls_back_to_contract_version_when_provider_omits_one() {
        let registry = InterfaceRegistry::new();
        registry.register_contract(InterfaceContract {
            interface: "alice.thermal".into(),
            version: Version::parse("1.0.0").unwrap(),
            state_fields: Vec::new(),
            methods: Vec::new(),
            events: Vec::new(),
            types: HashMap::new(),
            capabilities: ContractCapabilities::default(),
        });
        registry.register(InterfaceProvider {
            interface: "alice.thermal".into(),
            version: None,
            base_module: None,
            provider_module: "@alice/lmsensors".into(),
            backend_name: "lm-sensors".into(),
            priority: 100,
        });

        let resolved = registry.resolve("alice.thermal", Some(">=1.0"));
        assert_eq!(
            resolved.provider.unwrap().provider_module,
            "@alice/lmsensors"
        );
    }

    // cargo test -p mesh-core-service --release -- canonical_interface_borrowing_beats_owned_clone --ignored --nocapture
    #[test]
    #[ignore = "release-only canonical interface name microbenchmark"]
    fn canonical_interface_borrowing_beats_owned_clone() {
        use std::hint::black_box;
        use std::time::Instant;

        let iterations = 2_000_000usize;
        let canonical = "mesh.audio";

        let owned_started = Instant::now();
        let mut owned_total = 0usize;
        for _ in 0..iterations {
            let name = canonical_interface_name(black_box(canonical));
            owned_total = owned_total.wrapping_add(name.len());
        }
        let owned_time = owned_started.elapsed();

        let borrowed_started = Instant::now();
        let mut borrowed_total = 0usize;
        for _ in 0..iterations {
            let name = canonical_interface_name_cow(black_box(canonical));
            borrowed_total = borrowed_total.wrapping_add(name.len());
        }
        let borrowed_time = borrowed_started.elapsed();

        eprintln!(
            "canonical interface: owned {owned_time:?}; borrowed {borrowed_time:?}; ratio {:.1}x; totals={owned_total}/{borrowed_total}",
            owned_time.as_secs_f64() / borrowed_time.as_secs_f64()
        );
        assert_eq!(owned_total, borrowed_total);
        assert!(borrowed_time < owned_time);
    }

    // cargo test -p mesh-core-service --release -- owned_canonical_interface_reuses_runtime_names --ignored --nocapture
    #[test]
    #[ignore = "release-only owned canonical interface microbenchmark"]
    fn owned_canonical_interface_reuses_runtime_names() {
        use std::hint::black_box;
        use std::time::Instant;

        let iterations = 2_000_000usize;

        let old_started = Instant::now();
        let mut old_total = 0usize;
        for _ in 0..iterations {
            let service = black_box("mesh.audio".to_string());
            let interface = canonical_interface_name(black_box(&service));
            old_total = old_total.wrapping_add(interface.len());
        }
        let old_time = old_started.elapsed();

        let new_started = Instant::now();
        let mut new_total = 0usize;
        for _ in 0..iterations {
            let service = black_box("mesh.audio".to_string());
            let interface = canonical_interface_name_owned(black_box(service));
            new_total = new_total.wrapping_add(interface.len());
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "owned canonical interface over {iterations} canonical names: borrowed-to-owned {old_time:?}; owned-reuse {new_time:?}; ratio {:.1}x; totals={old_total}/{new_total}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_total, new_total);
        assert!(new_time < old_time);
    }

    // cargo test -p mesh-core-service --release -- service_name_borrowing_beats_owned_clone --ignored --nocapture
    #[test]
    #[ignore = "release-only service name projection microbenchmark"]
    fn service_name_borrowing_beats_owned_clone() {
        use std::hint::black_box;
        use std::time::Instant;

        let iterations = 2_000_000usize;
        let interface = "mesh.audio";

        let owned_started = Instant::now();
        let mut owned_total = 0usize;
        for _ in 0..iterations {
            let name = service_name_from_interface(black_box(interface));
            owned_total = owned_total.wrapping_add(name.len());
        }
        let owned_time = owned_started.elapsed();

        let borrowed_started = Instant::now();
        let mut borrowed_total = 0usize;
        for _ in 0..iterations {
            let name = service_name_from_interface_cow(black_box(interface));
            borrowed_total = borrowed_total.wrapping_add(name.len());
        }
        let borrowed_time = borrowed_started.elapsed();

        eprintln!(
            "service name projection: owned {owned_time:?}; borrowed {borrowed_time:?}; ratio {:.1}x; totals={owned_total}/{borrowed_total}",
            owned_time.as_secs_f64() / borrowed_time.as_secs_f64()
        );
        assert_eq!(owned_total, borrowed_total);
        assert!(borrowed_time < owned_time);
    }
}
