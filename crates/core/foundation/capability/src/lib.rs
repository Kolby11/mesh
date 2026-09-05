//! Capability-based permissions. Modules declare required and optional
//! capabilities in their manifest; core grants or denies them at load time.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fmt;

/// A dotted capability id: `shell.widget`, `service.battery.read`, …
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct Capability(String);

impl Capability {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn id(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Capability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum PrivilegeLevel {
    /// Read-only access to services, theme, locale.
    Standard,
    /// Meaningful system interaction; confirmed at install.
    Elevated,
    /// Sensitive access; explicit opt-in with a warning.
    High,
}

impl fmt::Display for PrivilegeLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Standard => write!(f, "standard"),
            Self::Elevated => write!(f, "elevated"),
            Self::High => write!(f, "high"),
        }
    }
}

/// The host capabilities understood by this MESH build.
///
/// Service read/control capabilities are listed explicitly because interface
/// contracts are data and their consumer capabilities must be reviewed before
/// they become runnable. Provider host powers are also explicit; executable
/// access uses the structured `exec.argv:<program>:<json-args>` form rather
/// than basename-derived grants.
#[derive(Debug, Clone, Copy, Default)]
pub struct CapabilityCatalog;

impl CapabilityCatalog {
    pub const fn builtin() -> Self {
        Self
    }

    fn privilege_level(&self, id: &str) -> Option<PrivilegeLevel> {
        Some(match id {
            "shell.surface"
            | "shell.widget"
            | "theme.read"
            | "locale.read"
            | "service.audio.read"
            | "service.brightness.read"
            | "service.bluetooth.read"
            | "service.composition.read"
            | "service.debug.read"
            | "service.device.read"
            | "service.media.read"
            | "service.network.read"
            | "service.packages.read"
            | "service.power.read"
            | "service.settings.read"
            | "service.wm.read" => PrivilegeLevel::Standard,
            "exec.launch-app"
            | "shell.clipboard.write"
            | "shell.notification"
            | "fs.write"
            | "dbus.session"
            | "net.http"
            | "service.audio.control"
            | "service.brightness.control"
            | "service.bluetooth.control"
            | "service.composition.control"
            | "service.debug.control"
            | "service.media.control"
            | "service.network.control"
            | "service.packages.control"
            | "service.settings.control"
            | "service.theme.control"
            | "service.wm.control"
            | "service.notifications.post"
            | "service.notifications.manage" => PrivilegeLevel::Elevated,
            "exec.command" | "shell.screenshot" | "dbus.system" | "net.socket" | "locale.write" => {
                PrivilegeLevel::High
            }
            value if value.starts_with("exec.argv:") && valid_exec_argv_capability(value) => {
                PrivilegeLevel::High
            }
            _ => return None,
        })
    }

    pub fn validate(&self, id: &str) -> Result<PrivilegeLevel, CapabilityPolicyError> {
        self.privilege_level(id)
            .ok_or_else(|| CapabilityPolicyError::UnknownCapability {
                module_id: String::new(),
                capability: id.to_string(),
            })
    }

    /// Build one runtime capability proof after checking that the id belongs
    /// to the closed host catalog.
    pub fn capability(&self, id: &str) -> Result<Capability, CapabilityPolicyError> {
        self.validate(id)?;
        Ok(Capability::new(id))
    }

    /// Build a runtime capability proof set from ids validated by this closed
    /// catalog. Activation normally uses `EffectiveCapabilities` instead;
    /// this path is for host-owned requests that are not module activations.
    pub fn capability_set<I, S>(&self, ids: I) -> Result<CapabilitySet, CapabilityPolicyError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let ids = ids.into_iter().map(Into::into).collect::<Vec<_>>();
        for id in &ids {
            self.validate(id)?;
        }
        Ok(CapabilitySet::from_validated_ids(ids))
    }
}

fn valid_exec_argv_capability(value: &str) -> bool {
    let Some(specification) = value.strip_prefix("exec.argv:") else {
        return false;
    };
    let Some((program, arguments)) = specification.split_once(':') else {
        return false;
    };
    if program.is_empty() || program.contains('\0') || arguments.is_empty() {
        return false;
    }
    arguments == "*" || serde_json::from_str::<Vec<String>>(arguments).is_ok()
}

/// The immutable result of resolving a module's declarations against user
/// approvals. Only this value should cross the activation boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveCapabilities {
    granted: BTreeSet<String>,
}

impl EffectiveCapabilities {
    pub fn granted_ids(&self) -> impl Iterator<Item = &str> {
        self.granted.iter().map(String::as_str)
    }

    /// Adapt the immutable policy result to the runtime's capability proof
    /// container. The runtime receives only the resolved grant set.
    pub fn into_capability_set(&self) -> CapabilitySet {
        CapabilitySet::from_validated_ids(self.granted.iter().cloned())
    }
}

/// Persisted decisions and the catalog-backed resolver used at activation.
#[derive(Debug, Clone, Default)]
pub struct CapabilityPolicy {
    catalog: CapabilityCatalog,
    approvals: BTreeMap<String, BTreeSet<String>>,
}

impl CapabilityPolicy {
    pub fn from_approvals<I>(approvals: I) -> Self
    where
        I: IntoIterator<Item = (String, Vec<String>)>,
    {
        let approvals = approvals
            .into_iter()
            .map(|(module_id, capabilities)| {
                (module_id, capabilities.into_iter().collect::<BTreeSet<_>>())
            })
            .collect();
        Self {
            catalog: CapabilityCatalog::builtin(),
            approvals,
        }
    }

    pub fn resolve(
        &self,
        module_id: &str,
        required: &[String],
        optional: &[String],
    ) -> Result<EffectiveCapabilities, CapabilityPolicyError> {
        let required = normalize_declarations(&self.catalog, module_id, required)?;
        let optional = normalize_declarations(&self.catalog, module_id, optional)?;
        let approved = self.approvals.get(module_id);

        let missing_required = required
            .iter()
            .filter(|capability| !approved.is_some_and(|set| set.contains(*capability)))
            .cloned()
            .collect::<Vec<_>>();
        if !missing_required.is_empty() {
            return Err(CapabilityPolicyError::MissingRequiredApproval {
                module_id: module_id.to_string(),
                capabilities: missing_required,
            });
        }

        let mut granted = required.clone();
        granted.extend(
            optional
                .iter()
                .filter(|capability| approved.is_some_and(|set| set.contains(*capability)))
                .cloned(),
        );
        Ok(EffectiveCapabilities { granted })
    }
}

fn normalize_declarations(
    catalog: &CapabilityCatalog,
    module_id: &str,
    declarations: &[String],
) -> Result<BTreeSet<String>, CapabilityPolicyError> {
    let mut normalized = BTreeSet::new();
    for capability in declarations {
        if capability.trim() != capability || capability.is_empty() {
            return Err(CapabilityPolicyError::InvalidCapability {
                module_id: module_id.to_string(),
                capability: capability.clone(),
            });
        }
        catalog
            .validate(capability)
            .map_err(|error| error.with_module(module_id))?;
        normalized.insert(capability.clone());
    }
    Ok(normalized)
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CapabilityPolicyError {
    #[error("module '{module_id}' requests unknown capability '{capability}'")]
    UnknownCapability {
        module_id: String,
        capability: String,
    },
    #[error("module '{module_id}' declares malformed capability '{capability}'")]
    InvalidCapability {
        module_id: String,
        capability: String,
    },
    #[error("module '{module_id}' is missing approval for required capabilities: {capabilities:?}")]
    MissingRequiredApproval {
        module_id: String,
        capabilities: Vec<String>,
    },
}

impl CapabilityPolicyError {
    fn with_module(self, module_id: &str) -> Self {
        match self {
            Self::UnknownCapability { capability, .. } => Self::UnknownCapability {
                module_id: module_id.to_string(),
                capability,
            },
            other => other,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CapabilitySet {
    granted: HashSet<Capability>,
}

impl CapabilitySet {
    /// Construct a proof set for an already-resolved grant list.
    ///
    /// Production activation must use `EffectiveCapabilities::into_capability_set`
    /// and host-owned requests must use `CapabilityCatalog::capability_set`.
    /// This immutable adapter remains public for runtime fixtures and external
    /// integrations that already receive resolved grants.
    pub fn from_ids<I>(ids: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<String>,
    {
        Self::from_validated_ids(ids.into_iter().map(Into::into))
    }

    fn from_validated_ids<I>(ids: I) -> Self
    where
        I: IntoIterator<Item = String>,
    {
        Self {
            granted: ids.into_iter().map(Capability::new).collect(),
        }
    }

    pub fn is_granted(&self, capability: &Capability) -> bool {
        self.granted.contains(capability)
    }

    pub fn granted(&self) -> &HashSet<Capability> {
        &self.granted
    }
}

impl Default for CapabilitySet {
    fn default() -> Self {
        Self {
            granted: HashSet::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug, Default)]
    struct AllocationCounters {
        count: u64,
        allocated_bytes: u64,
        live_bytes: u64,
    }

    mod test_alloc {
        use super::AllocationCounters;
        use std::alloc::{GlobalAlloc, Layout, System};
        use std::cell::{Cell, RefCell};

        thread_local! {
            static TRACKING: Cell<bool> = const { Cell::new(false) };
            static COUNTERS: RefCell<AllocationCounters> =
                const { RefCell::new(AllocationCounters { count: 0, allocated_bytes: 0, live_bytes: 0 }) };
        }

        pub(super) fn begin() {
            TRACKING.with(|tracking| tracking.set(false));
            COUNTERS.with(|counters| counters.replace(AllocationCounters::default()));
            TRACKING.with(|tracking| tracking.set(true));
        }

        pub(super) fn end() -> AllocationCounters {
            TRACKING.with(|tracking| tracking.set(false));
            COUNTERS.with(|counters| *counters.borrow())
        }

        pub(super) fn reset_activity() {
            COUNTERS.with(|counters| {
                let mut counters = counters.borrow_mut();
                counters.count = 0;
                counters.allocated_bytes = 0;
            });
        }

        pub(super) fn snapshot() -> AllocationCounters {
            COUNTERS.with(|counters| *counters.borrow())
        }

        fn record(update: impl FnOnce(&mut AllocationCounters)) {
            TRACKING.with(|tracking| {
                if tracking.get() {
                    COUNTERS.with(|counters| update(&mut counters.borrow_mut()));
                }
            });
        }

        pub(super) struct CountingAllocator;

        unsafe impl GlobalAlloc for CountingAllocator {
            unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
                let pointer = unsafe { System.alloc(layout) };
                if !pointer.is_null() {
                    record(|counters| {
                        counters.count += 1;
                        counters.allocated_bytes += layout.size() as u64;
                        counters.live_bytes += layout.size() as u64;
                    });
                }
                pointer
            }

            unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
                let pointer = unsafe { System.alloc_zeroed(layout) };
                if !pointer.is_null() {
                    record(|counters| {
                        counters.count += 1;
                        counters.allocated_bytes += layout.size() as u64;
                        counters.live_bytes += layout.size() as u64;
                    });
                }
                pointer
            }

            unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
                unsafe { System.dealloc(pointer, layout) };
                record(|counters| {
                    counters.live_bytes = counters.live_bytes.saturating_sub(layout.size() as u64);
                });
            }

            unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
                let new_pointer = unsafe { System.realloc(pointer, layout, new_size) };
                if !new_pointer.is_null() {
                    record(|counters| {
                        counters.count += 1;
                        counters.allocated_bytes += new_size as u64;
                        counters.live_bytes = counters
                            .live_bytes
                            .saturating_sub(layout.size() as u64)
                            .saturating_add(new_size as u64);
                    });
                }
                new_pointer
            }
        }
    }

    #[global_allocator]
    static TEST_ALLOCATOR: test_alloc::CountingAllocator = test_alloc::CountingAllocator;

    #[test]
    fn unknown_capabilities_fail_closed() {
        assert_eq!(
            CapabilityCatalog::builtin().validate("exec.wpctl"),
            Err(CapabilityPolicyError::UnknownCapability {
                module_id: String::new(),
                capability: "exec.wpctl".into(),
            })
        );
        assert_eq!(
            CapabilityCatalog::builtin().validate("exec.argv:wpctl:[\"get-volume\"]"),
            Ok(PrivilegeLevel::High)
        );
        assert_eq!(
            CapabilityCatalog::builtin().validate("service.unknown.read"),
            Err(CapabilityPolicyError::UnknownCapability {
                module_id: String::new(),
                capability: "service.unknown.read".into(),
            })
        );
    }

    #[test]
    fn required_capabilities_need_approval_and_optional_default_to_denied() {
        let policy =
            CapabilityPolicy::from_approvals([("@mesh/test".into(), vec!["theme.read".into()])]);
        let required = vec!["theme.read".into()];
        let optional = vec!["locale.read".into()];
        let effective = policy.resolve("@mesh/test", &required, &optional).unwrap();
        assert!(effective.granted_ids().any(|id| id == "theme.read"));
        assert!(!effective.granted_ids().any(|id| id == "locale.read"));
        assert_eq!(
            policy.resolve("@mesh/missing", &required, &[]),
            Err(CapabilityPolicyError::MissingRequiredApproval {
                module_id: "@mesh/missing".into(),
                capabilities: vec!["theme.read".into()],
            })
        );
    }

    #[test]
    fn optional_approval_is_included_in_effective_grants() {
        let policy = CapabilityPolicy::from_approvals([(
            "@mesh/test".into(),
            vec!["theme.read".into(), "locale.read".into()],
        )]);
        let effective = policy
            .resolve(
                "@mesh/test",
                &["theme.read".into()],
                &["locale.read".into()],
            )
            .unwrap();
        assert!(effective.granted_ids().any(|id| id == "locale.read"));
        assert_eq!(effective.into_capability_set().granted().len(), 2);
    }

    #[test]
    fn catalog_capability_set_rejects_unknown_ids() {
        let catalog = CapabilityCatalog::builtin();
        assert!(
            catalog
                .capability_set(["service.audio.control"])
                .unwrap()
                .is_granted(&Capability::new("service.audio.control"))
        );
        assert!(matches!(
            catalog.capability_set(["service.unknown.control"]),
            Err(CapabilityPolicyError::UnknownCapability {
                module_id,
                capability
            }) if module_id.is_empty() && capability == "service.unknown.control"
        ));
    }

    #[test]
    fn shipped_debug_capabilities_have_explicit_catalog_privileges() {
        let catalog = CapabilityCatalog::builtin();
        assert_eq!(
            catalog.validate("service.debug.read"),
            Ok(PrivilegeLevel::Standard)
        );
        assert_eq!(
            catalog.validate("service.debug.control"),
            Ok(PrivilegeLevel::Elevated)
        );

        let policy = CapabilityPolicy::from_approvals([(
            "@mesh/debug-inspector".into(),
            vec![
                "locale.read".into(),
                "service.debug.control".into(),
                "service.debug.read".into(),
                "shell.surface".into(),
            ],
        )]);
        let effective = policy
            .resolve(
                "@mesh/debug-inspector",
                &[
                    "shell.surface".into(),
                    "service.debug.read".into(),
                    "service.debug.control".into(),
                    "locale.read".into(),
                ],
                &[],
            )
            .expect("the shipped debug inspector approval should resolve");
        assert_eq!(
            effective.granted_ids().collect::<Vec<_>>(),
            vec![
                "locale.read",
                "service.debug.control",
                "service.debug.read",
                "shell.surface"
            ]
        );
    }

    // cargo test -p mesh-core-capability --release -- capability_activation_release_benchmark --ignored --nocapture
    #[test]
    #[ignore = "release-only capability activation allocation benchmark"]
    fn capability_activation_release_benchmark() {
        const MODULE_COUNT: usize = 500;
        const GRANTS_PER_MODULE: usize = 10;
        const ROOT_COUNT: usize = 4;
        const RESTART_COUNT: usize = 50;
        const WARMUP_COUNT: usize = 3;
        const REQUIRED_GRANTS: usize = 5;
        const GRANTS: [&str; GRANTS_PER_MODULE] = [
            "shell.surface",
            "shell.widget",
            "theme.read",
            "locale.read",
            "service.audio.read",
            "service.brightness.read",
            "service.bluetooth.read",
            "service.composition.read",
            "service.debug.read",
            "service.device.read",
        ];

        let mut samples = Vec::with_capacity(RESTART_COUNT);
        let mut allocations = Vec::with_capacity(RESTART_COUNT);
        test_alloc::begin();
        let fixture = capability_activation_benchmark_fixture(
            MODULE_COUNT,
            &GRANTS,
            REQUIRED_GRANTS,
            ROOT_COUNT,
        );
        let fixture_live_bytes = test_alloc::snapshot().live_bytes;
        let mut last_activation = None;

        for _ in 0..WARMUP_COUNT {
            drop(last_activation.take());
            last_activation = Some(activate_capabilities(&fixture));
        }
        drop(last_activation.take());
        test_alloc::reset_activity();

        for _ in 0..RESTART_COUNT {
            drop(last_activation.take());
            test_alloc::reset_activity();
            let started = std::time::Instant::now();
            let activation = activate_capabilities(&fixture);
            let elapsed = started.elapsed().as_nanos();
            last_activation = Some(activation);
            let counters = test_alloc::snapshot();
            samples.push(elapsed);
            allocations.push(counters);
        }

        let final_counters = test_alloc::end();
        samples.sort_unstable();

        let min_ns = samples[0];
        let median_ns = samples[samples.len() / 2];
        let p95_ns = samples[(samples.len() * 95) / 100];
        let max_ns = *samples.last().expect("benchmark has samples");
        let min_allocations = allocations.iter().map(|item| item.count).min().unwrap();
        let max_allocations = allocations.iter().map(|item| item.count).max().unwrap();
        let min_allocated_bytes = allocations
            .iter()
            .map(|item| item.allocated_bytes)
            .min()
            .unwrap();
        let max_allocated_bytes = allocations
            .iter()
            .map(|item| item.allocated_bytes)
            .max()
            .unwrap();
        let min_retained_bytes = allocations
            .iter()
            .map(|item| item.live_bytes.saturating_sub(fixture_live_bytes))
            .min()
            .unwrap();
        let max_retained_bytes = allocations
            .iter()
            .map(|item| item.live_bytes.saturating_sub(fixture_live_bytes))
            .max()
            .unwrap();
        let activation = last_activation
            .as_ref()
            .expect("benchmark should retain its final activation");

        assert_eq!(activation.effective.len(), MODULE_COUNT);
        assert_eq!(
            activation
                .effective
                .values()
                .map(|effective| effective.granted_ids().count())
                .sum::<usize>(),
            MODULE_COUNT * GRANTS_PER_MODULE
        );
        assert_eq!(activation.roots.len(), ROOT_COUNT);
        assert!(
            activation
                .roots
                .iter()
                .all(|capabilities| capabilities.granted().len() == GRANTS_PER_MODULE)
        );

        eprintln!(
            "MESH_PERF metric=capability_activation modules={MODULE_COUNT} grants_per_module={GRANTS_PER_MODULE} roots={ROOT_COUNT} restarts={RESTART_COUNT} warmups={WARMUP_COUNT} latency_ns={min_ns}..{max_ns} median_ns={median_ns} p95_ns={p95_ns} allocations={min_allocations}..{max_allocations} allocated_bytes={min_allocated_bytes}..{max_allocated_bytes} retained_bytes={min_retained_bytes}..{max_retained_bytes} fixture_retained_bytes={} final_retained_bytes={}",
            fixture_live_bytes,
            final_counters.live_bytes.saturating_sub(fixture_live_bytes)
        );
    }

    struct CapabilityBenchmarkModule {
        id: String,
        required: Vec<String>,
        optional: Vec<String>,
    }

    struct CapabilityBenchmarkFixture {
        policy: CapabilityPolicy,
        modules: Vec<CapabilityBenchmarkModule>,
        root_modules: Vec<usize>,
    }

    struct CapabilityActivation {
        effective: std::collections::HashMap<String, EffectiveCapabilities>,
        roots: Vec<CapabilitySet>,
    }

    fn capability_activation_benchmark_fixture(
        module_count: usize,
        grants: &[&str],
        required_count: usize,
        root_count: usize,
    ) -> CapabilityBenchmarkFixture {
        let modules = (0..module_count)
            .map(|index| CapabilityBenchmarkModule {
                id: format!("@mesh/benchmark-{index:03}"),
                required: grants[..required_count]
                    .iter()
                    .map(|grant| (*grant).to_string())
                    .collect(),
                optional: grants[required_count..]
                    .iter()
                    .map(|grant| (*grant).to_string())
                    .collect(),
            })
            .collect::<Vec<_>>();
        let approvals = modules
            .iter()
            .map(|module| {
                (
                    module.id.clone(),
                    grants.iter().map(|grant| (*grant).to_string()).collect(),
                )
            })
            .collect::<Vec<_>>();

        CapabilityBenchmarkFixture {
            policy: CapabilityPolicy::from_approvals(approvals),
            modules,
            root_modules: (0..root_count).collect(),
        }
    }

    fn activate_capabilities(fixture: &CapabilityBenchmarkFixture) -> CapabilityActivation {
        let mut effective = std::collections::HashMap::with_capacity(fixture.modules.len());
        for module in &fixture.modules {
            let resolved = fixture
                .policy
                .resolve(&module.id, &module.required, &module.optional)
                .expect("benchmark declarations should be approved");
            effective.insert(module.id.clone(), resolved);
        }

        let roots = fixture
            .root_modules
            .iter()
            .map(|&module_index| effective[&fixture.modules[module_index].id].into_capability_set())
            .collect();
        CapabilityActivation { effective, roots }
    }
}
