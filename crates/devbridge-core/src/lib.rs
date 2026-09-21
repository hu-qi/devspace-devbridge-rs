//! DevBridge control-plane primitives.
//!
//! This crate owns runtime settings, credentials, local configuration and the
//! DevBridge REST API. It deliberately does not contain CLI presentation logic
//! or tunnel transport code.

pub mod api;
pub mod auth;
pub mod config;
pub mod settings;

pub use api::{ApiClient, ApiError};
pub use auth::{Credential, UserInfo};
pub use settings::Settings;
