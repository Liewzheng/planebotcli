//! Shared foundation: configuration loading and the typed error hierarchy.

pub mod config;
pub mod errors;

pub use config::{Config, load_config};
pub use errors::PlaneError;
