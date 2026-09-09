//! Shared foundation: configuration loading and the typed error hierarchy.

pub mod config;
pub mod errors;

pub use config::{Config, config_file_path, load_config, save_config};
pub use errors::PlaneError;
