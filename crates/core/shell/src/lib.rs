/// Core runtime and orchestration for MESH shell.
///
/// This crate ties together all subsystems: module loading, capability
/// enforcement, event routing, theming, localization, and diagnostics.
#[cfg(all(test, feature = "allocation-profiling"))]
#[global_allocator]
static TEST_ALLOCATOR: mesh_core_debug::allocation::CountingAllocator<std::alloc::System> =
    mesh_core_debug::allocation::CountingAllocator::new(std::alloc::System);

pub mod shell;

pub use shell::{
    ComponentContext, ComponentError, CoreEvent, CoreRequest, FrontendEffectRevision,
    FrontendFrame, FrontendFrameEffects, FrontendFrameError, FrontendFrameRevision,
    FrontendFrameRevisions, FrontendInvalidation, FrontendPaintMetadata, FrontendServiceSnapshot,
    ServiceEvent, Shell, ShellComponent, ShellRunError, SurfaceId, default_ipc_socket_path,
};
