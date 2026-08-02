mod parse;
mod resolve;
mod types;

pub use parse::{parse_animation_shorthand, parse_transform};
pub use resolve::*;
pub use types::*;

#[cfg(test)]
mod tests;
