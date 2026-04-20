/// Transitional typed bindings for common service interfaces.
///
/// The long-term source of truth is the interface contract plugin on disk.
/// These Rust traits remain as typed adapters while the runtime finishes
/// moving fully to plugin-declared interfaces and providers.
pub mod audio;
pub mod brightness;
pub mod media;
pub mod network;
pub mod notifications;
pub mod power;
