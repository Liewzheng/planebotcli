//! Async HTTP client for the Plane v1 API.
//!
//! There is no official Rust plane-sdk, so this crate implements the v1 API
//! surface directly (the Python CLI's "escape hatches" become the norm here).
//! Endpoints are prefixed with `/api/v1`; workspace-scoped resources take the
//! workspace slug from the config. Authentication is the `X-Api-Key` header.

use planebotcli_core::{Config, PlaneError};
use planebotcli_types::{
    Comment, CommentWrite, Label, LabelWrite, Member, Project, ProjectWrite, State, StateWrite,
    User, WorkItem, WorkItemWrite,
};
use serde::de::DeserializeOwned;

pub struct PlaneClient {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
    workspace: String,
}

impl PlaneClient {
    pub fn new(cfg: &Config) -> Result<Self, PlaneError> {
        let http = reqwest::Client::builder()
            .user_agent("planebotcli/0.1.0")
            .build()
            .map_err(|e| PlaneError::Api {
                message: format!("failed to build HTTP client: {e}"),
            })?;
        Ok(Self {
            http,
            base_url: cfg.base_url.clone(),
            api_key: cfg.api_key.clone(),
            workspace: cfg.workspace.clone(),
        })
    }

    pub fn workspace(&self) -> &str {
        &self.workspace
    }

    async fn request_json<T: DeserializeOwned>(
        &self,
        method: reqwest::Method,
        path: &str,
        query: &[(&str, String)],
        body: Option<&serde_json::Value>,
    ) -> Result<T, PlaneError> {
        let url = format!("{}{}", self.base_url, path);
        let mut attempts = 0;
        loop {
            attempts += 1;
            let mut req = self
                .http
                .request(method.clone(), &url)
                .header("X-Api-Key", &self.api_key)
                .header("Content-Type", "application/json");
            for (k, v) in query {
                req = req.query(&[(k, v)]);
            }
            if let Some(b) = body {
                req = req.json(b);
            }
            let resp = req.send().await.map_err(|e| PlaneError::Api {
                message: format!("request to {path} failed: {e}"),
            })?;
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            if status.is_success() {
                if text.trim().is_empty() {
                    return serde_json::from_str("null").map_err(|e| PlaneError::Api {
                        message: format!("empty response from {path}: {e}"),
                    });
                }
                return serde_json::from_str(&text).map_err(|e| PlaneError::Api {
                    message: format!("invalid JSON from {path}: {e}"),
                });
            }
            let retryable =
                status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.is_server_error();
            if retryable && attempts < 5 {
                tokio::time::sleep(tokio::time::Duration::from_millis(200 * attempts as u64)).await;
                continue;
            }
            return Err(map_api_error(status, &text));
        }
    }

    /// Follow cursor pagination until exhausted, collecting all results.
    async fn paginate<T: DeserializeOwned>(&self, path: &str) -> Result<Vec<T>, PlaneError> {
        let mut all = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let mut query: Vec<(&str, String)> = vec![("limit", "50".to_string())];
            if let Some(c) = &cursor {
                query.push(("cursor", c.clone()));
            }
            let page: Page<T> = self
                .request_json(reqwest::Method::GET, path, &query, None)
                .await?;
            let had_more = page.next_page_results;
            all.extend(page.results);
            match page.next_cursor {
                Some(c) if had_more => cursor = Some(c),
                _ => break,
            }
        }
        Ok(all)
    }

    /// `GET /api/v1/users/me/` — the current authenticated user.
    pub async fn get_me(&self) -> Result<User, PlaneError> {
        self.request_json(reqwest::Method::GET, "/api/v1/users/me/", &[], None)
            .await
    }

    /// `GET /api/v1/workspaces/{ws}/members/` — returns a plain array.
    pub async fn list_members(&self) -> Result<Vec<Member>, PlaneError> {
        let path = format!("/api/v1/workspaces/{}/members/", self.workspace);
        self.request_json(reqwest::Method::GET, &path, &[], None)
            .await
    }

    /// `GET /api/v1/workspaces/{ws}/projects/{id}/`
    pub async fn get_project(&self, project_id: &str) -> Result<Project, PlaneError> {
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/",
            self.workspace, project_id
        );
        self.request_json(reqwest::Method::GET, &path, &[], None)
            .await
    }

    /// `GET /api/v1/workspaces/{ws}/projects/` — paginated (cursor-based).
    pub async fn list_projects(&self) -> Result<Vec<Project>, PlaneError> {
        let path = format!("/api/v1/workspaces/{}/projects/", self.workspace);
        self.paginate(&path).await
    }

    /// `POST /api/v1/workspaces/{ws}/projects/`
    pub async fn create_project(&self, body: &ProjectWrite) -> Result<Project, PlaneError> {
        let path = format!("/api/v1/workspaces/{}/projects/", self.workspace);
        self.request_json(reqwest::Method::POST, &path, &[], Some(&body_json(body)?))
            .await
    }

    /// `PATCH /api/v1/workspaces/{ws}/projects/{id}/`
    pub async fn update_project(
        &self,
        project_id: &str,
        body: &ProjectWrite,
    ) -> Result<Project, PlaneError> {
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/",
            self.workspace, project_id
        );
        self.request_json(reqwest::Method::PATCH, &path, &[], Some(&body_json(body)?))
            .await
    }

    /// `DELETE /api/v1/workspaces/{ws}/projects/{id}/`
    pub async fn delete_project(&self, project_id: &str) -> Result<(), PlaneError> {
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/",
            self.workspace, project_id
        );
        let _: serde_json::Value = self
            .request_json(reqwest::Method::DELETE, &path, &[], None)
            .await?;
        Ok(())
    }

    /// `GET /api/v1/workspaces/{ws}/projects/{pid}/labels/`
    pub async fn list_labels(&self, project_id: &str) -> Result<Vec<Label>, PlaneError> {
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/labels/",
            self.workspace, project_id
        );
        self.paginate(&path).await
    }

    /// `POST /api/v1/workspaces/{ws}/projects/{pid}/labels/`
    pub async fn create_label(
        &self,
        project_id: &str,
        body: &LabelWrite,
    ) -> Result<Label, PlaneError> {
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/labels/",
            self.workspace, project_id
        );
        self.request_json(reqwest::Method::POST, &path, &[], Some(&body_json(body)?))
            .await
    }

    /// `GET /api/v1/workspaces/{ws}/projects/{pid}/states/`
    pub async fn list_states(&self, project_id: &str) -> Result<Vec<State>, PlaneError> {
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/states/",
            self.workspace, project_id
        );
        self.paginate(&path).await
    }

    /// `POST /api/v1/workspaces/{ws}/projects/{pid}/states/`
    pub async fn create_state(
        &self,
        project_id: &str,
        body: &StateWrite,
    ) -> Result<State, PlaneError> {
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/states/",
            self.workspace, project_id
        );
        self.request_json(reqwest::Method::POST, &path, &[], Some(&body_json(body)?))
            .await
    }

    /// `GET /api/v1/workspaces/{ws}/projects/{pid}/work-items/` — all pages.
    pub async fn list_work_items(&self, project_id: &str) -> Result<Vec<WorkItem>, PlaneError> {
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/work-items/",
            self.workspace, project_id
        );
        self.paginate(&path).await
    }

    /// `GET /api/v1/workspaces/{ws}/projects/{pid}/work-items/{id}/?expand=estimate_point`
    pub async fn get_work_item(
        &self,
        project_id: &str,
        work_item_id: &str,
    ) -> Result<WorkItem, PlaneError> {
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/work-items/{}",
            self.workspace, project_id, work_item_id
        );
        let expand = [("expand", "estimate_point".to_string())];
        self.request_json(reqwest::Method::GET, &path, &expand, None)
            .await
    }

    /// `POST /api/v1/workspaces/{ws}/projects/{pid}/work-items/`
    pub async fn create_work_item(
        &self,
        project_id: &str,
        body: &WorkItemWrite,
    ) -> Result<WorkItem, PlaneError> {
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/work-items/",
            self.workspace, project_id
        );
        self.request_json(reqwest::Method::POST, &path, &[], Some(&body_json(body)?))
            .await
    }

    /// `PATCH /api/v1/workspaces/{ws}/projects/{pid}/work-items/{id}/`
    pub async fn update_work_item(
        &self,
        project_id: &str,
        work_item_id: &str,
        body: &WorkItemWrite,
    ) -> Result<WorkItem, PlaneError> {
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/work-items/{}",
            self.workspace, project_id, work_item_id
        );
        self.request_json(reqwest::Method::PATCH, &path, &[], Some(&body_json(body)?))
            .await
    }

    /// `DELETE /api/v1/workspaces/{ws}/projects/{pid}/work-items/{id}/`
    pub async fn delete_work_item(
        &self,
        project_id: &str,
        work_item_id: &str,
    ) -> Result<(), PlaneError> {
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/work-items/{}",
            self.workspace, project_id, work_item_id
        );
        let _: serde_json::Value = self
            .request_json(reqwest::Method::DELETE, &path, &[], None)
            .await?;
        Ok(())
    }

    /// `GET /api/v1/workspaces/{ws}/work-items/search/?query=...`
    pub async fn search_work_items(&self, query: &str) -> Result<Vec<WorkItem>, PlaneError> {
        let path = format!("/api/v1/workspaces/{}/work-items/search/", self.workspace);
        let q = [("query", query.to_string())];
        self.request_json(reqwest::Method::GET, &path, &q, None)
            .await
    }

    /// `GET .../work-items/{id}/comments/`
    pub async fn list_comments(
        &self,
        project_id: &str,
        work_item_id: &str,
    ) -> Result<Vec<Comment>, PlaneError> {
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/work-items/{}/comments/",
            self.workspace, project_id, work_item_id
        );
        self.paginate(&path).await
    }

    /// `POST .../work-items/{id}/comments/`
    pub async fn create_comment(
        &self,
        project_id: &str,
        work_item_id: &str,
        body: &CommentWrite,
    ) -> Result<Comment, PlaneError> {
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/work-items/{}/comments/",
            self.workspace, project_id, work_item_id
        );
        self.request_json(reqwest::Method::POST, &path, &[], Some(&body_json(body)?))
            .await
    }

    /// `PATCH .../comments/{id}/`
    pub async fn update_comment(
        &self,
        project_id: &str,
        work_item_id: &str,
        comment_id: &str,
        body: &CommentWrite,
    ) -> Result<Comment, PlaneError> {
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/work-items/{}/comments/{}",
            self.workspace, project_id, work_item_id, comment_id
        );
        self.request_json(reqwest::Method::PATCH, &path, &[], Some(&body_json(body)?))
            .await
    }

    /// `DELETE .../comments/{id}/`
    pub async fn delete_comment(
        &self,
        project_id: &str,
        work_item_id: &str,
        comment_id: &str,
    ) -> Result<(), PlaneError> {
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/work-items/{}/comments/{}",
            self.workspace, project_id, work_item_id, comment_id
        );
        let _: serde_json::Value = self
            .request_json(reqwest::Method::DELETE, &path, &[], None)
            .await?;
        Ok(())
    }
}

/// Serialize a request body, dropping `null` fields so PATCH bodies only carry
/// the fields the caller actually set (the Python SDK used exclude_none=True).
fn body_json<T: serde::Serialize>(value: &T) -> Result<serde_json::Value, PlaneError> {
    let value = serde_json::to_value(value).map_err(|e| PlaneError::Api {
        message: format!("failed to serialize request body: {e}"),
    })?;
    Ok(strip_nulls(value))
}

fn strip_nulls(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.into_iter()
                .filter(|(_, v)| !v.is_null())
                .map(|(k, v)| (k, strip_nulls(v)))
                .collect(),
        ),
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.into_iter().map(strip_nulls).collect())
        }
        other => other,
    }
}

/// Cursor-paginated envelope used by list endpoints.
#[derive(Debug, serde::Deserialize)]
struct Page<T> {
    results: Vec<T>,
    next_cursor: Option<String>,
    #[serde(default)]
    next_page_results: bool,
}

/// Map an HTTP error to a typed `PlaneError` (exit codes 2/3/4).
pub fn map_api_error(status: reqwest::StatusCode, body: &str) -> PlaneError {
    match status.as_u16() {
        401 => PlaneError::Auth {
            message: "Authentication failed.".into(),
        },
        404 => PlaneError::NotFound {
            message: "Resource not found (HTTP 404).".into(),
        },
        429 => PlaneError::Api {
            message: "Rate limited by Plane API after multiple retries. Try again later.".into(),
        },
        code => {
            let message = match response_error_detail(body) {
                Some(d) => format!("API error (HTTP {code}): {d}"),
                None => format!("API error (HTTP {code}): {body}"),
            };
            PlaneError::Api { message }
        }
    }
}

/// Extract human-readable detail from an API error body: a top-level `detail`
/// string, or field-level errors like `{"description": ["This field is
/// required."]}` rendered as `description: This field is required.`.
/// Sensitive keys are never rendered.
pub fn response_error_detail(body: &str) -> Option<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
        let trimmed = body.trim();
        return if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.chars().take(200).collect())
        };
    };
    if let Some(detail) = value
        .get("detail")
        .and_then(|d| d.as_str())
        .filter(|s| !s.trim().is_empty())
    {
        return Some(detail.trim().to_string());
    }
    let obj = value.as_object()?;
    let parts: Vec<String> = obj
        .iter()
        .filter(|(k, _)| !is_sensitive_key(k))
        .filter_map(|(key, val)| {
            let messages: Vec<String> = match val {
                serde_json::Value::Array(items) => items
                    .iter()
                    .filter_map(|x| x.as_str().map(|s| s.trim().to_string()))
                    .filter(|s| !s.is_empty())
                    .collect(),
                serde_json::Value::String(s) if !s.trim().is_empty() => vec![s.trim().to_string()],
                _ => return None,
            };
            if messages.is_empty() {
                None
            } else {
                Some(format!("{key}: {}", messages.join("; ")))
            }
        })
        .collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" | "))
    }
}

fn is_sensitive_key(key: &str) -> bool {
    matches!(
        key.to_ascii_lowercase().as_str(),
        "api_key" | "authorization" | "token" | "access_token" | "password"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(code: u16) -> reqwest::StatusCode {
        reqwest::StatusCode::from_u16(code).unwrap()
    }

    #[test]
    fn maps_401_to_auth() {
        assert!(matches!(
            map_api_error(status(401), ""),
            PlaneError::Auth { .. }
        ));
    }

    #[test]
    fn maps_404_to_not_found() {
        assert!(matches!(
            map_api_error(status(404), ""),
            PlaneError::NotFound { .. }
        ));
    }

    #[test]
    fn maps_429_to_rate_limit() {
        assert!(matches!(
            map_api_error(status(429), ""),
            PlaneError::Api { .. }
        ));
    }

    #[test]
    fn renders_field_errors() {
        let body = r#"{"description": ["This field is required."]}"#;
        let err = map_api_error(status(400), body);
        let PlaneError::Api { message } = err else {
            panic!("expected Api")
        };
        assert!(message.contains("HTTP 400"));
        assert!(message.contains("description: This field is required."));
    }

    #[test]
    fn renders_top_level_detail() {
        let detail = response_error_detail(r#"{"detail": "Not allowed"}"#);
        assert_eq!(detail.as_deref(), Some("Not allowed"));
    }

    #[test]
    fn filters_sensitive_keys() {
        let body = r#"{"api_key": ["leak"], "name": ["bad"]}"#;
        let detail = response_error_detail(body).unwrap();
        assert!(!detail.contains("leak"));
        assert!(detail.contains("name: bad"));
    }

    #[test]
    fn truncates_text_body() {
        let body = "x".repeat(500);
        let detail = response_error_detail(&body).unwrap();
        assert!(detail.len() <= 200);
    }
}
