/// Module manifest loading and normalized representation.
mod graph;
mod load;
mod model;

// Legacy manifest readers remain available only to migration regression tests.
// Production loading is exclusively handled by the canonical package manifest
// loader in `crate::package`.
#[cfg(test)]
mod json;
#[cfg(test)]
mod toml;

#[cfg(test)]
mod tests;

pub use graph::*;
pub use load::*;
pub use model::*;
