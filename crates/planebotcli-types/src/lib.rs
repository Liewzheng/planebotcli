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
    /// Whether the project's intake queue is enabled (the project's
    /// `intake_view` flag; absent in some serializers, so `None` means off).
    #[serde(default, rename = "intake_view")]
    pub intake_view: Option<bool>,
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
    /// Numeric sequence; the API returns integers or floats depending on the
    /// project sort config.
    pub sequence: Option<serde_json::Value>,
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

/// A module as returned by the modules endpoints.
///
/// The API exposes the end date under `target_date` (not `end_date`), which is
/// the field name the write DTO and the CLI both use.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Module {
    pub id: String,
    pub name: Option<String>,
    pub description: Option<String>,
    /// backlog | planned | in-progress | paused | completed | cancelled
    pub status: Option<String>,
    #[serde(rename = "start_date")]
    pub start_date: Option<String>,
    #[serde(rename = "target_date")]
    pub target_date: Option<String>,
    #[serde(rename = "created_at")]
    pub created_at: Option<String>,
    #[serde(rename = "updated_at")]
    pub updated_at: Option<String>,
}

/// Request body for module create/update.
#[derive(Debug, Clone, Serialize, Default)]
pub struct ModuleWrite {
    pub name: Option<String>,
    pub description: Option<String>,
    pub status: Option<String>,
    #[serde(rename = "start_date")]
    pub start_date: Option<String>,
    #[serde(rename = "target_date")]
    pub target_date: Option<String>,
}

/// A cycle as returned by the cycles endpoints.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Cycle {
    pub id: String,
    pub name: Option<String>,
    pub description: Option<String>,
    #[serde(rename = "start_date")]
    pub start_date: Option<String>,
    #[serde(rename = "end_date")]
    pub end_date: Option<String>,
    #[serde(rename = "owned_by")]
    pub owned_by: Option<String>,
    #[serde(rename = "total_issues")]
    pub total_issues: Option<i64>,
    #[serde(rename = "completed_issues")]
    pub completed_issues: Option<i64>,
    #[serde(rename = "started_issues")]
    pub started_issues: Option<i64>,
    #[serde(rename = "unstarted_issues")]
    pub unstarted_issues: Option<i64>,
    #[serde(rename = "backlog_issues")]
    pub backlog_issues: Option<i64>,
    #[serde(rename = "cancelled_issues")]
    pub cancelled_issues: Option<i64>,
    #[serde(rename = "created_at")]
    pub created_at: Option<String>,
    #[serde(rename = "updated_at")]
    pub updated_at: Option<String>,
}

/// Request body for cycle create/update.
///
/// `owned_by` and `project_id` are required on create (the API fills in the
/// owner from the authenticated user when omitted from PATCH bodies).
#[derive(Debug, Clone, Serialize, Default)]
pub struct CycleWrite {
    pub name: Option<String>,
    pub description: Option<String>,
    #[serde(rename = "start_date")]
    pub start_date: Option<String>,
    #[serde(rename = "end_date")]
    pub end_date: Option<String>,
    #[serde(rename = "owned_by")]
    pub owned_by: Option<String>,
    #[serde(rename = "project_id")]
    pub project_id: Option<String>,
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

/// An intake queue row (the `IntakeIssue` record), as returned by the
/// `/intake-issues/` endpoints.
///
/// The wrapper has its own `id`; the underlying work item's UUID is the
/// `issue` field (shown as "Issue ID" by the CLI and used as the target of
/// accept/decline/delete — never the wrapper `id`). `issue_detail` carries an
/// expanded copy of the work item (name, priority, state, ...) that the API
/// includes.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IntakeItem {
    /// Intake wrapper id — NOT the work-item uuid.
    pub id: String,
    /// Work-item UUID — the triage target / "Issue ID" column.
    #[serde(default, alias = "issue_id")]
    pub issue: Option<String>,
    /// Expanded work item (id/name/priority/state/...), when the API expands it.
    #[serde(default, rename = "issue_detail")]
    pub issue_detail: Option<Value>,
    /// Intake status code: -2 pending, -1 rejected, 0 snoozed, 1 accepted,
    /// 2 duplicate.
    #[serde(default)]
    pub status: Option<i64>,
    #[serde(default, rename = "created_at")]
    pub created_at: Option<String>,
    #[serde(default, rename = "updated_at")]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub workspace: Option<String>,
}

/// Request body for `intake create`: the work item data nested under `issue`.
#[derive(Debug, Clone, Serialize, Default)]
pub struct IntakeWrite {
    #[serde(rename = "issue")]
    pub issue: IntakeIssueWrite,
}

/// The embedded work item of an intake create request.
#[derive(Debug, Clone, Serialize, Default)]
pub struct IntakeIssueWrite {
    pub name: String,
    #[serde(rename = "description_html")]
    pub description_html: Option<String>,
    pub priority: Option<String>,
}
