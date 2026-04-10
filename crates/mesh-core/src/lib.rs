/// Core runtime and orchestration for MESH shell.
///
/// This crate ties together all subsystems: plugin loading, capability
/// enforcement, event routing, theming, localization, and diagnostics.
pub mod shell;

pub use shell::{
    ComponentContext, ComponentError, CoreEvent, CoreRequest, ServiceEvent, Shell, ShellComponent,
    ShellRunError, SurfaceId,
};
