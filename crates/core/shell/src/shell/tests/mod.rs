mod backend_lifecycle;
mod benchmarks;
mod child_surface;
mod commands;
mod common;
mod debug;
mod discovery;
mod graph;
mod input;
mod popover;
mod profiling;
mod request_drain;
mod scheduler;
mod service_contract;
mod service_delivery;
mod service_state;
mod surface_layout;
mod theme;
mod window_role;

use super::types;
use super::{
    BackendLaunchCandidate, BackendRuntimeSlot, BackendRuntimeStatus, ComponentInput, CoreRequest,
    InterfaceProvider, KeyModifiers, ResolvedServiceCatalogHandle, ServiceCommandMsg, ServiceEvent,
    ServiceInterfaceEventSubscription, ServiceObservationSummary, Shell, ShellCompositionMode,
    TabFocusTarget, backend_launch_candidates_from_graph, blur_quality_from_settings,
    component_key_pressed_input, component_key_released_input,
    discovery::{
        discover_shell_module_manifest_dirs, load_shell_module_manifests,
        load_shell_module_manifests_serial, startup_composition,
    },
    ipc::parse_ipc_command,
    service::{
        apply_service_update, script_events_to_requests, seed_service_state,
        service_name_from_interface,
    },
    shell_global_shortcut_request,
    surface_layout::{load_active_theme, resolve_frontend_module_settings},
};
use mesh_core_debug::{
    ComponentInvalidationCounts, DisplayBatchBarrierSnapshot, ProfilingBackendStage,
    ProfilingInvalidationSnapshot, ProfilingStage, RepaintPolicySnapshot,
    RetainedInvalidationCounts, RetainedPaintSnapshot, TextCacheSnapshot,
};
use mesh_core_elements::{LayoutRect, VariableStore, WidgetNode};
use mesh_core_interaction::measure_content_size;
use mesh_core_module::ModuleInstance;
use mesh_core_module::manifest::{
    CapabilitiesSection, CompatibilitySection, DependenciesSection, EntrypointsSection,
    ExportsSection, Manifest, ManifestSource, ModuleSection, ModuleType, ProvidedInterface,
};
use mesh_core_module::package::{
    InstalledModuleGraph, LoadedModuleManifest, ModuleManifest, ModuleManifestSource,
    RootModuleGraphManifest,
};
use mesh_core_scripting::{PublishedEvent, ScriptState};
use mesh_core_service::{
    ContractCapabilities, InterfaceArgument, InterfaceContract, InterfaceEvent, InterfaceMethod,
    contract::ContractStateField, parse_contract_version,
};
use mesh_core_wayland::{ClipboardError, ClipboardWriter};
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;
use tokio::sync::mpsc;

#[test]
fn empty_xdg_runtime_directory_is_absent() {
    assert_eq!(
        super::non_empty_env_path(Some(std::ffi::OsString::new())),
        None
    );
    assert_eq!(
        super::non_empty_env_path(Some(std::ffi::OsString::from("/run/user/1000"))),
        Some(PathBuf::from("/run/user/1000"))
    );
}
