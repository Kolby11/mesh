//! Renderer-neutral frontend host contracts.
//!
//! The compiler-facing package intentionally contains only values that can be
//! exchanged without selecting a renderer, compositor protocol, debug
//! backend, package store, or shell policy. Shell integration belongs to
//! `mesh-core-frontend-shell-adapter`.

pub use mesh_core_frontend_abi::{
    DebugEffect, EffectRejection, EffectScope, EffectSource, FrontendEffect, FrontendEffectBatch,
    FrontendEffectRevision, ScopedFrontendEffect, ServiceEffect, SurfaceEffect, SurfaceRole,
};

/// A renderer-neutral sink for effects produced by a frontend runtime.
///
/// Shells implement this contract with an adapter that supplies the concrete
/// request queue and policy. The ABI does not expose that queue or its policy
/// specific request variants to compiler-facing callers.
pub trait FrontendEffectSink {
    type Request;
    type Error;

    fn publish(&mut self, effect: ScopedFrontendEffect) -> Result<Self::Request, Self::Error>;
}
