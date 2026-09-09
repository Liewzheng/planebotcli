//! Serde data types shared across the planebotcli crates.
//!
//! DTOs mirror the Plane v1 API responses. Some fields are polymorphic across
//! endpoints (work-item `state`/`labels`/`assignees` come back as raw UUID
//! strings in the detail endpoint and as UUID lists in list endpoints) — those
//! are kept as `serde_json::Value` and interpreted by the render layer, the
//! same way the Python CLI's `_enrich_work_item` resolves names via cached
//! state/label/member maps.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A workspace member / user, as returned by `/api/v1/users/me/`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub display_name: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub email: Option<String>,
}

/// A member row of `/api/v1/workspaces/{ws}/members/`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Member {
    pub id: String,
    #[serde(rename = "display_name")]
    pub display_name: Option<String>,
    #[serde(rename = "first_name")]
    pub first_name: Option<String>,
    #[serde(rename = "last_name")]
    pub last_name: Option<String>,
    pub email: Option<String>,
}

impl Member {
    /// Best-effort full name, matching the Python member_map builder.
    pub fn full_name(&self) -> String {
        let joined = [self.first_name.as_deref(), self.last_name.as_deref()]
            .into_iter()
            .flatten()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        if !joined.is_empty() {
            joined
        } else {
            self.display_name.clone().unwrap_or_default()
        }
    }
}

/// A project as returned by the projects endpoints.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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

/// Request body for project create/update.
#[derive(Debug, Clone, Serialize, Default)]
pub struct ProjectWrite {
    pub name: Option<String>,
    pub identifier: Option<String>,
    pub description: Option<String>,
}

/// A label as returned by the labels endpoints.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Label {
    pub id: String,
    pub name: Option<String>,
    pub color: Option<String>,
    pub description: Option<String>,
    pub parent: Option<String>,
    #[serde(rename = "created_at")]
    pub created_at: Option<String>,
}

/// Request body for label create/update.
#[derive(Debug, Clone, Serialize, Default)]
pub struct LabelWrite {
    pub name: Option<String>,
    pub color: Option<String>,
    pub description: Option<String>,
}

/// A state as returned by the states endpoints.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct State {
    pub id: String,
    pub name: Option<String>,
    pub group: Option<String>,
    pub color: Option<String>,
    pub sequence: Option<i64>,
    #[serde(rename = "created_at")]
    pub created_at: Option<String>,
}

/// Request body for state create/update.
#[derive(Debug, Clone, Serialize, Default)]
pub struct StateWrite {
    pub name: Option<String>,
    pub color: Option<String>,
    pub group: Option<String>,
    pub description: Option<String>,
}

/// Project summary embedded in cross-project work-item listings.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectDetail {
    pub id: Option<String>,
    pub identifier: Option<String>,
    pub name: Option<String>,
}

/// A work item as returned by the list/detail endpoints.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkItem {
    pub id: String,
    pub name: Option<String>,
    pub sequence_id: Option<Value>,
    #[serde(rename = "description_html")]
    pub description_html: Option<String>,
    pub priority: Option<String>,
    /// UUID string, or an object when expanded.
    #[serde(default)]
    pub state: Option<Value>,
    /// List of UUIDs, or objects when expanded.
    #[serde(default)]
    pub labels: Option<Value>,
    /// List of UUIDs, or objects when expanded.
    #[serde(default)]
    pub assignees: Option<Value>,
    pub parent: Option<String>,
    #[serde(rename = "start_date")]
    pub start_date: Option<String>,
    #[serde(rename = "target_date")]
    pub target_date: Option<String>,
    #[serde(rename = "created_at")]
    pub created_at: Option<String>,
    #[serde(rename = "updated_at")]
    pub updated_at: Option<String>,
    pub project: Option<String>,
    #[serde(rename = "project_detail")]
    pub project_detail: Option<ProjectDetail>,
    #[serde(rename = "estimate_point")]
    pub estimate_point: Option<Value>,
    pub point: Option<Value>,
    #[serde(rename = "is_draft")]
    pub is_draft: Option<bool>,
}

/// Request body for work-item create.
#[derive(Debug, Clone, Serialize, Default)]
pub struct WorkItemWrite {
    pub name: Option<String>,
    #[serde(rename = "description_html")]
    pub description_html: Option<String>,
    pub priority: Option<String>,
    pub state: Option<String>,
    pub assignees: Option<Vec<String>>,
    pub labels: Option<Vec<String>>,
    pub parent: Option<String>,
    #[serde(rename = "start_date")]
    pub start_date: Option<String>,
    #[serde(rename = "target_date")]
    pub target_date: Option<String>,
    #[serde(rename = "estimate_point")]
    pub estimate_point: Option<String>,
}

/// A comment as returned by the comments endpoints.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Comment {
    pub id: String,
    #[serde(rename = "comment_html")]
    pub comment_html: Option<String>,
    #[serde(rename = "comment_stripped")]
    pub comment_stripped: Option<String>,
    #[serde(rename = "created_at")]
    pub created_at: Option<String>,
    #[serde(rename = "updated_at")]
    pub updated_at: Option<String>,
    pub actor: Option<String>,
    #[serde(rename = "actor_detail")]
    pub actor_detail: Option<Value>,
    #[serde(rename = "created_by")]
    pub created_by: Option<String>,
    pub parent: Option<String>,
}

/// Request body for comment create/update.
#[derive(Debug, Clone, Serialize)]
pub struct CommentWrite {
    #[serde(rename = "comment_html")]
    pub comment_html: String,
}
