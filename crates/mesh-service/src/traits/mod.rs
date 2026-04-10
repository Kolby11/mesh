/// Service trait definitions.
///
/// Each module defines a trait that backends must implement and frontends consume.
/// Traits use async methods and return Results so backends can handle real system I/O.

pub mod audio;
pub mod brightness;
pub mod media;
pub mod network;
pub mod notifications;
pub mod power;
