//! Serde data types shared across the planebotcli crates.

use serde::{Deserialize, Serialize};

/// A workspace member / user, as returned by `/api/v1/users/me/`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub display_name: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub email: Option<String>,
}

/// A project as returned by the projects endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: Option<String>,
    pub identifier: Option<String>,
    pub description: Option<String>,
    #[serde(rename = "created_at")]
    pub created_at: Option<String>,
    #[serde(rename = "updated_at")]
    pub updated_at: Option<String>,
}
