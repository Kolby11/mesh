pub mod contract;
pub mod interface;
/// Service and interface plumbing for MESH.
///
/// This crate hosts the registry, contract loader, and transitional typed
/// bindings used by backends and frontends.
///
/// # Architecture
///
/// ```text
///  ┌─────────────────────────────────────────────────────┐
///  │                   Service Trait                      │
///  │            (e.g. AudioService, NetworkService)       │
///  └──────────────┬────────────────────┬─────────────────┘
///                  │                    │
///       ┌─────────▼──────┐   ┌─────────▼──────┐
///       │  Backend Plugin │   │  Backend Plugin │
///       │  (PipeWire)     │   │  (PulseAudio)   │
///       └────────────────┘   └────────────────┘
///                  │                    │
///                  └────────┬───────────┘
///                           │
///              ┌────────────▼────────────┐
///              │    ServiceRegistry      │
///              │  (one active per trait)  │
///              └────────────┬────────────┘
///                           │
///              ┌────────────▼────────────┐
///              │   Frontend / UI Widget  │
///              │  (uses trait, not impl) │
///              └─────────────────────────┘
/// ```
///
/// - An **interface contract** defines what a service can do (read volume, list networks, etc.)
/// - A **backend** is a plugin that implements an interface for a specific system
/// - A **frontend** is a UI component that consumes the interface through bindings
/// - The **registry** holds discovered contracts/providers and exposes them to frontends
///
/// Frontends never import backend crates. They only see the interface bridge.
pub mod registry;
pub mod traits;

pub use contract::{
    ContractCapabilities, ContractError, InterfaceArgument, InterfaceContract, InterfaceEvent,
    InterfaceMethod, InterfaceTypeDef, load_interface_contract, parse_contract_version,
    parse_version_req,
};
pub use interface::{
    InterfaceCatalog, InterfaceProvider, InterfaceRegistry, InterfaceResolution,
    canonical_interface_name,
};
pub use registry::{ServiceEntry, ServiceError, ServiceRegistry};
pub use traits::audio::{AudioDevice, AudioEvent, AudioService, AudioStream};
pub use traits::brightness::{BrightnessEvent, BrightnessService};
pub use traits::media::{MediaEvent, MediaInfo, MediaService, PlaybackState};
pub use traits::network::{NetworkConnection, NetworkDevice, NetworkEvent, NetworkService};
pub use traits::notifications::{Notification, NotificationEvent, NotificationService, Urgency};
pub use traits::power::{BatteryInfo, PowerEvent, PowerProfile, PowerService};
