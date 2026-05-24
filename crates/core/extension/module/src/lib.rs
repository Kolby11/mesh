pub mod lifecycle;
/// Module manifest, lifecycle, and loading for MESH.
pub mod manifest;
pub mod package;

pub use lifecycle::{ModuleInstance, ModuleState};
pub use manifest::{
    AccessibilitySection, ComponentExport, DependencyGraphError, ExportsSection,
    IconRequirementsSection, KeybindAction, KeybindScope, KeybindTrigger, KeybindTriggerKind,
    KeybindsSection, LoadedManifest, LocalizedText, Manifest, ManifestSource, ModuleSection,
    ModuleType, ProvidedInterface, ServiceSection, SlotContribution, SlotDefinition,
    validate_module_dependency_graph,
};
pub use package::{ModuleManifestError, RootModuleGraphManifest};
