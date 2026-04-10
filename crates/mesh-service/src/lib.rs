/// Service trait system for MESH.
///
/// This crate defines the contract between backends and frontends.
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
/// - A **service trait** defines what a service can do (read volume, list networks, etc.)
/// - A **backend** is a plugin that implements a service trait for a specific system
/// - A **frontend** is a UI component that consumes the trait through bindings
/// - The **registry** holds one active backend per service trait and exposes it to frontends
///
/// Frontends never import backend crates. They only see the trait.

pub mod registry;
pub mod traits;

pub use registry::{ServiceRegistry, ServiceEntry, ServiceError};
pub use traits::audio::{AudioService, AudioDevice, AudioStream, AudioEvent};
pub use traits::network::{NetworkService, NetworkConnection, NetworkDevice, NetworkEvent};
pub use traits::notifications::{NotificationService, Notification, Urgency, NotificationEvent};
pub use traits::power::{PowerService, PowerProfile, BatteryInfo, PowerEvent};
pub use traits::media::{MediaService, PlaybackState, MediaInfo, MediaEvent};
pub use traits::brightness::{BrightnessService, BrightnessEvent};
