pub mod lifecycle;
/// Plugin manifest, lifecycle, and loading for MESH.
pub mod manifest;

pub use lifecycle::{PluginInstance, PluginState};
pub use manifest::{
    AccessibilitySection, ComponentExport, DependencyGraphError, ExportsSection, LoadedManifest,
    Manifest, ManifestSource, PackageSection, PluginType, ProvidedInterface, ServiceSection,
    SlotContribution, SlotDefinition, validate_plugin_dependency_graph,
};
