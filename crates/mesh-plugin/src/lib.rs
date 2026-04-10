/// Plugin manifest, lifecycle, and loading for MESH.
pub mod manifest;
pub mod lifecycle;

pub use manifest::{Manifest, PackageSection, PluginType, ServiceSection};
pub use lifecycle::{PluginState, PluginInstance};
