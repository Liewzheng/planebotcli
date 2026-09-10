//! Async HTTP client for the Plane v1 API.
//!
//! There is no official Rust plane-sdk, so this crate implements the v1 API
//! surface directly (the Python CLI's "escape hatches" become the norm here).
//! Endpoints are prefixed with `/api/v1`; workspace-scoped resources take the
//! workspace slug from the config. Authentication is the `X-Api-Key` header.

use planebotcli_cache::Cache;
use planebotcli_core::{Config, PlaneError};
use planebotcli_types::{
    Attachment, Comment, CommentWrite, Cycle, CycleWrite, IntakeItem, IntakeWrite, Label,
    LabelWrite, Member, Module, ModuleWrite, Page, PageWrite, Project, ProjectWrite, State,
    StateWrite, User, WorkItem, WorkItemWrite,
};
use serde::de::DeserializeOwned;
use std::time::Duration;

/// Cache TTLs, mirroring the Python CLI's per-resource TTLs.
const TTL_MEMBERS: Duration = Duration::from_secs(300);
const TTL_PROJECTS: Duration = Duration::from_secs(60);
const TTL_STATES: Duration = Duration::from_secs(120);
const TTL_LABELS: Duration = Duration::from_secs(120);
const TTL_WORK_ITEMS: Duration = Duration::from_secs(60);
const TTL_INTAKE: Duration = Duration::from_secs(60);
const TTL_MODULES: Duration = Duration::from_secs(300);
const TTL_CYCLES: Duration = Duration::from_secs(300);
const TTL_PAGES: Duration = Duration::from_secs(120);
const TTL_ATTACHMENTS: Duration = Duration::from_secs(120);

/// The two places pages (documents) live: directly under the workspace, or
/// under one of its projects.
pub enum PageScope {
    /// `/api/v1/workspaces/{ws}/pages/`
    Workspace,
    /// `/api/v1/workspaces/{ws}/projects/{pid}/pages/`
    Project(String),
}

impl PageScope {
    /// URL path prefix (ends with `/`) for this scope under the workspace.
    fn base_path(&self, workspace: &str) -> String {
        match self {
            Self::Workspace => format!("/api/v1/workspaces/{workspace}/pages/"),
            Self::Project(pid) => {
                format!("/api/v1/workspaces/{workspace}/projects/{pid}/pages/")
            }
        }
    }

    /// Cache key prefix for this scope. The workspace prefix doubles as the
    /// prefix for every project scope, so invalidating it clears both the
    /// workspace page list and all project page lists of the workspace.
    fn cache_prefix(&self, workspace: &str) -> String {
        match self {
            Self::Workspace => format!("pages:{workspace}"),
            Self::Project(pid) => format!("pages:{workspace}:{pid}"),
        }
    }
}

pub struct PbotClient {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
    workspace: String,
    cache: Cache,
    no_cache: bool,
}

impl PbotClient {
    pub fn new(cfg: &Config) -> Result<Self, PlaneError> {
        Self::with_cache(cfg, false)
    }

    pub fn with_cache(cfg: &Config, no_cache: bool) -> Result<Self, PlaneError> {
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
            cache: Cache::new(),
            no_cache,
        })
    }

    pub fn workspace(&self) -> &str {
        &self.workspace
    }

    /// Read a cached list, or fetch + store it (disabled under `--no-cache`).
    async fn cached_list<T>(
        &self,
        key: &str,
        ttl: std::time::Duration,
        fetch: impl std::future::Future<Output = Result<Vec<T>, PlaneError>>,
    ) -> Result<Vec<T>, PlaneError>
    where
        T: DeserializeOwned + serde::Serialize,
    {
        if !self.no_cache
            && let Some(value) = self.cache.get::<Vec<T>>(key, ttl)
        {
            return Ok(value);
        }
        let value = fetch.await?;
        if !self.no_cache {
            self.cache.set(key, &value);
        }
        Ok(value)
    }

    fn invalidate(&self, prefix: &str) {
        if !self.no_cache {
            self.cache.invalidate(prefix);
        }
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

    /// Raw JSON request that serializes `body` verbatim — unlike [`body_json`],
    /// it does not strip `null` fields, so a PATCH can carry an explicit
    /// `{"parent": null}` instead of collapsing to an empty (ignored) body.
    async fn request_value(
        &self,
        method: reqwest::Method,
        path: &str,
        query: &[(&str, String)],
        body: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, PlaneError> {
        self.request_json(method, path, query, body).await
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
            let page: PageEnvelope<T> = self
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
        let key = format!("members:{}", self.workspace);
        let path = format!("/api/v1/workspaces/{}/members/", self.workspace);
        self.cached_list(&key, TTL_MEMBERS, async move {
            self.request_json(reqwest::Method::GET, &path, &[], None)
                .await
        })
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

    /// `GET /api/v1/workspaces/{ws}/projects/` — paginated (cached).
    pub async fn list_projects(&self) -> Result<Vec<Project>, PlaneError> {
        let key = format!("projects:{}", self.workspace);
        let path = format!("/api/v1/workspaces/{}/projects/", self.workspace);
        self.cached_list(
            &key,
            TTL_PROJECTS,
            async move { self.paginate(&path).await },
        )
        .await
    }

    /// `POST /api/v1/workspaces/{ws}/projects/`
    pub async fn create_project(&self, body: &ProjectWrite) -> Result<Project, PlaneError> {
        let path = format!("/api/v1/workspaces/{}/projects/", self.workspace);
        let result = self
            .request_json(reqwest::Method::POST, &path, &[], Some(&body_json(body)?))
            .await;
        self.invalidate(&format!("projects:{}", self.workspace));
        result
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
        let result = self
            .request_json(reqwest::Method::PATCH, &path, &[], Some(&body_json(body)?))
            .await;
        self.invalidate(&format!("projects:{}", self.workspace));
        result
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

    /// `GET /api/v1/workspaces/{ws}/projects/{pid}/labels/` (cached).
    pub async fn list_labels(&self, project_id: &str) -> Result<Vec<Label>, PlaneError> {
        let key = format!("labels:{}:{}", self.workspace, project_id);
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/labels/",
            self.workspace, project_id
        );
        self.cached_list(&key, TTL_LABELS, async move { self.paginate(&path).await })
            .await
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
        let result = self
            .request_json(reqwest::Method::POST, &path, &[], Some(&body_json(body)?))
            .await;
        self.invalidate(&format!("labels:{}:{}", self.workspace, project_id));
        result
    }

    /// `PATCH /api/v1/workspaces/{ws}/projects/{pid}/labels/{id}/`
    pub async fn update_label(
        &self,
        project_id: &str,
        label_id: &str,
        body: &LabelWrite,
    ) -> Result<Label, PlaneError> {
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/labels/{}",
            self.workspace, project_id, label_id
        );
        let result = self
            .request_json(reqwest::Method::PATCH, &path, &[], Some(&body_json(body)?))
            .await;
        self.invalidate(&format!("labels:{}:{}", self.workspace, project_id));
        result
    }

    /// `DELETE /api/v1/workspaces/{ws}/projects/{pid}/labels/{id}/`
    pub async fn delete_label(&self, project_id: &str, label_id: &str) -> Result<(), PlaneError> {
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/labels/{}",
            self.workspace, project_id, label_id
        );
        let _: serde_json::Value = self
            .request_json(reqwest::Method::DELETE, &path, &[], None)
            .await?;
        self.invalidate(&format!("labels:{}:{}", self.workspace, project_id));
        Ok(())
    }

    /// `GET /api/v1/workspaces/{ws}/projects/{pid}/states/` (cached).
    pub async fn list_states(&self, project_id: &str) -> Result<Vec<State>, PlaneError> {
        let key = format!("states:{}:{}", self.workspace, project_id);
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/states/",
            self.workspace, project_id
        );
        self.cached_list(&key, TTL_STATES, async move { self.paginate(&path).await })
            .await
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
        let result = self
            .request_json(reqwest::Method::POST, &path, &[], Some(&body_json(body)?))
            .await;
        self.invalidate(&format!("states:{}:{}", self.workspace, project_id));
        result
    }

    /// `PATCH /api/v1/workspaces/{ws}/projects/{pid}/states/{id}/`
    pub async fn update_state(
        &self,
        project_id: &str,
        state_id: &str,
        body: &StateWrite,
    ) -> Result<State, PlaneError> {
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/states/{}",
            self.workspace, project_id, state_id
        );
        let result = self
            .request_json(reqwest::Method::PATCH, &path, &[], Some(&body_json(body)?))
            .await;
        self.invalidate(&format!("states:{}:{}", self.workspace, project_id));
        result
    }

    /// `DELETE /api/v1/workspaces/{ws}/projects/{pid}/states/{id}/`
    pub async fn delete_state(&self, project_id: &str, state_id: &str) -> Result<(), PlaneError> {
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/states/{}",
            self.workspace, project_id, state_id
        );
        let _: serde_json::Value = self
            .request_json(reqwest::Method::DELETE, &path, &[], None)
            .await?;
        self.invalidate(&format!("states:{}:{}", self.workspace, project_id));
        Ok(())
    }

    /// `GET /api/v1/workspaces/{ws}/projects/{pid}/work-items/` — all pages (cached).
    pub async fn list_work_items(&self, project_id: &str) -> Result<Vec<WorkItem>, PlaneError> {
        let key = format!("work_items:{}:{}", self.workspace, project_id);
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/work-items/",
            self.workspace, project_id
        );
        self.cached_list(
            &key,
            TTL_WORK_ITEMS,
            async move { self.paginate(&path).await },
        )
        .await
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
        let result = self
            .request_json(reqwest::Method::POST, &path, &[], Some(&body_json(body)?))
            .await;
        self.invalidate(&format!("work_items:{}:{}", self.workspace, project_id));
        result
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
        let result = self
            .request_json(reqwest::Method::PATCH, &path, &[], Some(&body_json(body)?))
            .await;
        self.invalidate(&format!("work_items:{}:{}", self.workspace, project_id));
        result
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
        self.invalidate(&format!("work_items:{}:{}", self.workspace, project_id));
        Ok(())
    }

    /// `GET /api/v1/workspaces/{ws}/work-items/search/?query=...`
    pub async fn search_work_items(&self, query: &str) -> Result<Vec<WorkItem>, PlaneError> {
        let path = format!("/api/v1/workspaces/{}/work-items/search/", self.workspace);
        let q = [("query", query.to_string())];
        self.request_json(reqwest::Method::GET, &path, &q, None)
            .await
    }

    /// `PATCH .../work-items/{id}/` with `{"parent": <uuid>}` to set the parent
    /// or an explicit `{"parent": null}` to clear it.
    ///
    /// The body is built here rather than through `body_json`: that helper
    /// drops nulls, which would turn a clear request into an empty PATCH the
    /// API accepts and ignores (ADR-0007).
    pub async fn set_work_item_parent(
        &self,
        project_id: &str,
        work_item_id: &str,
        parent: Option<&str>,
    ) -> Result<(), PlaneError> {
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/work-items/{}",
            self.workspace, project_id, work_item_id
        );
        let body = serde_json::json!({ "parent": parent });
        let _: serde_json::Value = self
            .request_value(reqwest::Method::PATCH, &path, &[], Some(&body))
            .await?;
        self.invalidate(&format!("work_items:{}:{}", self.workspace, project_id));
        Ok(())
    }

    /// `GET .../work-items/{id}/relations/` — the raw relation buckets
    /// (`blocking`, `blocked_by`, `duplicate`, `relates_to`, `start_after`,
    /// `start_before`, `finish_after`, `finish_before`), each a list of
    /// `{project_id, issue_id}`. Returned raw so the CLI can flatten it.
    pub async fn list_relations(
        &self,
        project_id: &str,
        work_item_id: &str,
    ) -> Result<serde_json::Value, PlaneError> {
        let path = relations_path(&self.workspace, project_id, work_item_id);
        self.request_value(reqwest::Method::GET, &path, &[], None)
            .await
    }

    /// `POST .../work-items/{id}/relations/` — body
    /// `{"relation_type": T, "issues": [<uuid>, ...]}`.
    ///
    /// The API only creates relations; there is no endpoint to remove one, so
    /// no delete method exists here.
    pub async fn create_relations(
        &self,
        project_id: &str,
        work_item_id: &str,
        relation_type: &str,
        issues: &[String],
    ) -> Result<serde_json::Value, PlaneError> {
        let path = relations_path(&self.workspace, project_id, work_item_id);
        let body = serde_json::json!({
            "relation_type": relation_type,
            "issues": issues,
        });
        self.request_value(reqwest::Method::POST, &path, &[], Some(&body))
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

    /// `GET .../work-items/{id}/attachments/` — all pages (cached).
    pub async fn list_attachments(
        &self,
        project_id: &str,
        work_item_id: &str,
    ) -> Result<Vec<Attachment>, PlaneError> {
        let key = attachment_cache_key(&self.workspace, project_id, work_item_id);
        let path = attachments_path(&self.workspace, project_id, work_item_id);
        self.cached_list(
            &key,
            TTL_ATTACHMENTS,
            // The attachments list endpoint returns a plain array (like the
            // members endpoint), not a cursor envelope.
            async move {
                self.request_json::<Vec<Attachment>>(reqwest::Method::GET, &path, &[], None)
                    .await
            },
        )
        .await
    }

    /// `POST .../work-items/{id}/attachments/` — register an upload and get the
    /// presigned multipart target.
    ///
    /// Returns the raw response so the caller can read `asset_id` and the
    /// nested `upload_data` (their shape varies between Plane versions).
    pub async fn register_attachment(
        &self,
        project_id: &str,
        work_item_id: &str,
        name: &str,
        mime: &str,
        size: u64,
    ) -> Result<serde_json::Value, PlaneError> {
        let path = attachments_path(&self.workspace, project_id, work_item_id);
        let body = serde_json::json!({ "name": name, "type": mime, "size": size });
        let result = self
            .request_json(reqwest::Method::POST, &path, &[], Some(&body))
            .await;
        self.invalidate(&attachment_cache_key(
            &self.workspace,
            project_id,
            work_item_id,
        ));
        result
    }

    /// Push the file bytes to the presigned URL carried in `upload_data`.
    ///
    /// Every string field of the nested `fields` object becomes a multipart
    /// form field and the bytes ride under the `file` part — the same shape
    /// the Python CLI sends. The request carries no `X-Api-Key`: the signature
    /// lives in the form fields.
    pub async fn upload_to_presigned(
        &self,
        upload_data: &serde_json::Value,
        file_name: &str,
        mime: &str,
        bytes: Vec<u8>,
    ) -> Result<(), PlaneError> {
        let url = upload_data
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or(PlaneError::Api {
                message: "the register response is missing the presigned upload url.".into(),
            })?;
        let mut form = reqwest::multipart::Form::new();
        if let Some(fields) = upload_data.get("fields").and_then(|v| v.as_object()) {
            for (key, value) in fields {
                if let Some(value) = value.as_str() {
                    form = form.text(key.clone(), value.to_string());
                }
            }
        }
        let part = reqwest::multipart::Part::bytes(bytes)
            .file_name(file_name.to_string())
            .mime_str(mime)
            .map_err(|e| PlaneError::Api {
                message: format!("invalid MIME type for the attachment upload: {e}"),
            })?;
        form = form.part("file", part);
        let resp = self
            .http
            .post(url)
            .multipart(form)
            .send()
            .await
            .map_err(|e| PlaneError::Api {
                message: format!("presigned upload to {url} failed: {e}"),
            })?;
        let status = resp.status();
        if !matches!(status.as_u16(), 200 | 201 | 204) {
            return Err(PlaneError::Api {
                message: format!("File upload failed: HTTP {}", status.as_u16()),
            });
        }
        Ok(())
    }

    /// `PATCH .../attachments/{asset_id}/` with `{"is_uploaded": true}`.
    ///
    /// A 2xx here is not proof the write landed (ADR-0007); the caller reads
    /// the attachment list back and verifies the asset before reporting.
    pub async fn finalize_attachment(
        &self,
        project_id: &str,
        work_item_id: &str,
        asset_id: &str,
    ) -> Result<(), PlaneError> {
        let path = format!(
            "{}{}/",
            attachments_path(&self.workspace, project_id, work_item_id),
            asset_id
        );
        let body = serde_json::json!({ "is_uploaded": true });
        let result: Result<serde_json::Value, PlaneError> = self
            .request_json(reqwest::Method::PATCH, &path, &[], Some(&body))
            .await;
        self.invalidate(&attachment_cache_key(
            &self.workspace,
            project_id,
            work_item_id,
        ));
        result.map(|_| ())
    }

    /// Register a description-embed image on the app assets v2 endpoint:
    /// `POST {base}/api/assets/v2/workspaces/{ws}/projects/{pid}/` with
    /// `{name, type, size, entity_type: "ISSUE_DESCRIPTION",
    /// entity_identifier: <work item id>}`.
    ///
    /// The endpoint is session-only on many deployments and answers
    /// 401/403/404, so the raw status is returned for the caller to fall back
    /// to a v1 ISSUE_ATTACHMENT upload (mirrors the Python
    /// `upload_embed_image`, which POSTs to the same URL and payload).
    pub async fn register_description_asset(
        &self,
        project_id: &str,
        work_item_id: &str,
        name: &str,
        mime: &str,
        size: u64,
    ) -> Result<(u16, serde_json::Value), PlaneError> {
        let url = format!(
            "{}/api/assets/v2/workspaces/{}/projects/{}/",
            self.base_url, self.workspace, project_id
        );
        let body = serde_json::json!({
            "name": name,
            "type": mime,
            "size": size,
            "entity_type": "ISSUE_DESCRIPTION",
            "entity_identifier": work_item_id,
        });
        self.app_json(reqwest::Method::POST, &url, Some(&body))
            .await
    }

    /// `PATCH {assets-v2 base}/{asset_id}/` with `{"is_uploaded": true}` —
    /// mark a description asset uploaded.
    ///
    /// Like the v1 attachment PATCH, a 2xx is not proof the write landed
    /// (ADR-0007); callers read the asset back afterwards.
    pub async fn confirm_description_asset(
        &self,
        project_id: &str,
        asset_id: &str,
    ) -> Result<(u16, serde_json::Value), PlaneError> {
        let url = format!(
            "{}/api/assets/v2/workspaces/{}/projects/{}/{}/",
            self.base_url, self.workspace, project_id, asset_id
        );
        let body = serde_json::json!({ "is_uploaded": true });
        self.app_json(reqwest::Method::PATCH, &url, Some(&body))
            .await
    }

    /// `GET {base}/api/v1/workspaces/{ws}/assets/{asset_id}/` — read a single
    /// asset back through the workspace-scoped generic-asset endpoint (works
    /// for any entity type). Raw status so the caller can reject an upload the
    /// server did not actually confirm.
    pub async fn get_workspace_asset(
        &self,
        asset_id: &str,
    ) -> Result<(u16, serde_json::Value), PlaneError> {
        let url = format!(
            "{}/api/v1/workspaces/{}/assets/{}/",
            self.base_url, self.workspace, asset_id
        );
        self.app_json(reqwest::Method::GET, &url, None).await
    }

    /// Raw authenticated JSON request to a full URL (the app endpoints are not
    /// under `/api/v1`), returning the status and parsed body without retries
    /// or error mapping — callers branch on status codes (fallbacks) that
    /// `request_json` would collapse into a typed error.
    async fn app_json(
        &self,
        method: reqwest::Method,
        url: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<(u16, serde_json::Value), PlaneError> {
        let mut req = self
            .http
            .request(method, url)
            .header("X-Api-Key", &self.api_key)
            .header("Content-Type", "application/json");
        if let Some(b) = body {
            req = req.json(b);
        }
        let resp = req.send().await.map_err(|e| PlaneError::Api {
            message: format!("request to {url} failed: {e}"),
        })?;
        let status = resp.status().as_u16();
        let text = resp.text().await.unwrap_or_default();
        let value = serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);
        Ok((status, value))
    }

    /// `GET .../projects/{pid}/modules/` — all pages (cached).
    pub async fn list_modules(&self, project_id: &str) -> Result<Vec<Module>, PlaneError> {
        let key = format!("modules:{}:{}", self.workspace, project_id);
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/modules/",
            self.workspace, project_id
        );
        self.cached_list(&key, TTL_MODULES, async move { self.paginate(&path).await })
            .await
    }

    /// `GET .../projects/{pid}/modules/{id}/`
    pub async fn get_module(
        &self,
        project_id: &str,
        module_id: &str,
    ) -> Result<Module, PlaneError> {
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/modules/{}",
            self.workspace, project_id, module_id
        );
        self.request_json(reqwest::Method::GET, &path, &[], None)
            .await
    }

    /// `POST .../projects/{pid}/modules/`
    pub async fn create_module(
        &self,
        project_id: &str,
        body: &ModuleWrite,
    ) -> Result<Module, PlaneError> {
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/modules/",
            self.workspace, project_id
        );
        let result = self
            .request_json(reqwest::Method::POST, &path, &[], Some(&body_json(body)?))
            .await;
        self.invalidate(&format!("modules:{}:{}", self.workspace, project_id));
        result
    }

    /// `PATCH .../projects/{pid}/modules/{id}/`
    pub async fn update_module(
        &self,
        project_id: &str,
        module_id: &str,
        body: &ModuleWrite,
    ) -> Result<Module, PlaneError> {
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/modules/{}",
            self.workspace, project_id, module_id
        );
        let result = self
            .request_json(reqwest::Method::PATCH, &path, &[], Some(&body_json(body)?))
            .await;
        self.invalidate(&format!("modules:{}:{}", self.workspace, project_id));
        result
    }

    /// `DELETE .../projects/{pid}/modules/{id}/`
    pub async fn delete_module(&self, project_id: &str, module_id: &str) -> Result<(), PlaneError> {
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/modules/{}",
            self.workspace, project_id, module_id
        );
        let _: serde_json::Value = self
            .request_json(reqwest::Method::DELETE, &path, &[], None)
            .await?;
        self.invalidate(&format!("modules:{}:{}", self.workspace, project_id));
        Ok(())
    }

    /// `POST .../modules/{id}/module-issues/` — body `{"issues": [ids]}`.
    pub async fn add_work_items_to_module(
        &self,
        project_id: &str,
        module_id: &str,
        work_item_ids: &[String],
    ) -> Result<(), PlaneError> {
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/modules/{}/module-issues",
            self.workspace, project_id, module_id
        );
        let body = serde_json::json!({ "issues": work_item_ids });
        let _: serde_json::Value = self
            .request_json(reqwest::Method::POST, &path, &[], Some(&body))
            .await?;
        self.invalidate(&format!("modules:{}:{}", self.workspace, project_id));
        Ok(())
    }

    /// `GET .../modules/{id}/module-issues/` — the module's work items.
    pub async fn list_module_work_items(
        &self,
        project_id: &str,
        module_id: &str,
    ) -> Result<Vec<WorkItem>, PlaneError> {
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/modules/{}/module-issues",
            self.workspace, project_id, module_id
        );
        self.paginate(&path).await
    }

    /// `GET .../projects/{pid}/cycles/` — all pages (cached).
    pub async fn list_cycles(&self, project_id: &str) -> Result<Vec<Cycle>, PlaneError> {
        let key = format!("cycles:{}:{}", self.workspace, project_id);
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/cycles/",
            self.workspace, project_id
        );
        self.cached_list(&key, TTL_CYCLES, async move { self.paginate(&path).await })
            .await
    }

    /// `GET .../projects/{pid}/cycles/{id}/`
    pub async fn get_cycle(&self, project_id: &str, cycle_id: &str) -> Result<Cycle, PlaneError> {
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/cycles/{}",
            self.workspace, project_id, cycle_id
        );
        self.request_json(reqwest::Method::GET, &path, &[], None)
            .await
    }

    /// `POST .../projects/{pid}/cycles/`
    pub async fn create_cycle(
        &self,
        project_id: &str,
        body: &CycleWrite,
    ) -> Result<Cycle, PlaneError> {
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/cycles/",
            self.workspace, project_id
        );
        let result = self
            .request_json(reqwest::Method::POST, &path, &[], Some(&body_json(body)?))
            .await;
        self.invalidate(&format!("cycles:{}:{}", self.workspace, project_id));
        result
    }

    /// `PATCH .../projects/{pid}/cycles/{id}/`
    pub async fn update_cycle(
        &self,
        project_id: &str,
        cycle_id: &str,
        body: &CycleWrite,
    ) -> Result<Cycle, PlaneError> {
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/cycles/{}",
            self.workspace, project_id, cycle_id
        );
        let result = self
            .request_json(reqwest::Method::PATCH, &path, &[], Some(&body_json(body)?))
            .await;
        self.invalidate(&format!("cycles:{}:{}", self.workspace, project_id));
        result
    }

    /// `DELETE .../projects/{pid}/cycles/{id}/`
    pub async fn delete_cycle(&self, project_id: &str, cycle_id: &str) -> Result<(), PlaneError> {
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/cycles/{}",
            self.workspace, project_id, cycle_id
        );
        let _: serde_json::Value = self
            .request_json(reqwest::Method::DELETE, &path, &[], None)
            .await?;
        self.invalidate(&format!("cycles:{}:{}", self.workspace, project_id));
        Ok(())
    }

    /// `POST .../cycles/{id}/cycle-issues/` — body `{"issues": [id]}`.
    pub async fn add_work_item_to_cycle(
        &self,
        project_id: &str,
        cycle_id: &str,
        work_item_id: &str,
    ) -> Result<(), PlaneError> {
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/cycles/{}/cycle-issues",
            self.workspace, project_id, cycle_id
        );
        let body = serde_json::json!({ "issues": [work_item_id] });
        let _: serde_json::Value = self
            .request_json(reqwest::Method::POST, &path, &[], Some(&body))
            .await?;
        self.invalidate(&format!("cycles:{}:{}", self.workspace, project_id));
        Ok(())
    }

    /// `DELETE .../cycles/{id}/cycle-issues/{work_item_id}/`
    pub async fn remove_work_item_from_cycle(
        &self,
        project_id: &str,
        cycle_id: &str,
        work_item_id: &str,
    ) -> Result<(), PlaneError> {
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/cycles/{}/cycle-issues/{}",
            self.workspace, project_id, cycle_id, work_item_id
        );
        let _: serde_json::Value = self
            .request_json(reqwest::Method::DELETE, &path, &[], None)
            .await?;
        self.invalidate(&format!("cycles:{}:{}", self.workspace, project_id));
        Ok(())
    }

    /// `GET .../cycles/{id}/cycle-issues/` — the cycle's work items.
    pub async fn list_cycle_work_items(
        &self,
        project_id: &str,
        cycle_id: &str,
    ) -> Result<Vec<WorkItem>, PlaneError> {
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/cycles/{}/cycle-issues",
            self.workspace, project_id, cycle_id
        );
        self.paginate(&path).await
    }

    /// `GET .../pages/` — all pages of a scope, following pagination (cached).
    pub async fn list_pages(&self, scope: &PageScope) -> Result<Vec<Page>, PlaneError> {
        let key = scope.cache_prefix(&self.workspace);
        let path = scope.base_path(&self.workspace);
        self.cached_list(&key, TTL_PAGES, async move { self.paginate(&path).await })
            .await
    }

    /// Workspace-scoped convenience for [`Self::list_pages`].
    pub async fn list_workspace_pages(&self) -> Result<Vec<Page>, PlaneError> {
        self.list_pages(&PageScope::Workspace).await
    }

    /// Project-scoped convenience for [`Self::list_pages`].
    pub async fn list_project_pages(&self, project_id: &str) -> Result<Vec<Page>, PlaneError> {
        self.list_pages(&PageScope::Project(project_id.to_string()))
            .await
    }

    /// `GET .../pages/{id}/` — a single page in the given scope.
    pub async fn get_page(&self, scope: &PageScope, page_id: &str) -> Result<Page, PlaneError> {
        let path = format!("{}{}/", scope.base_path(&self.workspace), page_id);
        self.request_json(reqwest::Method::GET, &path, &[], None)
            .await
    }

    /// `POST .../pages/` — create a page in the given scope.
    pub async fn create_page(
        &self,
        scope: &PageScope,
        body: &PageWrite,
    ) -> Result<Page, PlaneError> {
        let path = scope.base_path(&self.workspace);
        let result = self
            .request_json(reqwest::Method::POST, &path, &[], Some(&body_json(body)?))
            .await;
        self.invalidate(&scope.cache_prefix(&self.workspace));
        result
    }

    /// `PATCH .../pages/{id}/` — update a page in the given scope.
    pub async fn update_page(
        &self,
        scope: &PageScope,
        page_id: &str,
        body: &PageWrite,
    ) -> Result<Page, PlaneError> {
        let path = format!("{}{}/", scope.base_path(&self.workspace), page_id);
        let result = self
            .request_json(reqwest::Method::PATCH, &path, &[], Some(&body_json(body)?))
            .await;
        self.invalidate(&scope.cache_prefix(&self.workspace));
        result
    }

    /// `PATCH .../pages/{id}/` with `{"archived_at": date}` — archive a page.
    ///
    /// The API rejects DELETE on a page that has not been archived, and a bare
    /// `is_archived` flag is silently ignored (ADR-0007), so deletion goes
    /// through this archive step first. Returns the raw parsed response so the
    /// caller can verify the archive actually landed before deleting.
    pub async fn archive_page(
        &self,
        scope: &PageScope,
        page_id: &str,
        date: &str,
    ) -> Result<serde_json::Value, PlaneError> {
        let path = format!("{}{}/", scope.base_path(&self.workspace), page_id);
        let body = serde_json::json!({ "archived_at": date });
        let result = self
            .request_json(reqwest::Method::PATCH, &path, &[], Some(&body))
            .await;
        self.invalidate(&scope.cache_prefix(&self.workspace));
        result
    }

    /// `DELETE .../pages/{id}/` — permanently delete an archived page.
    pub async fn delete_page(&self, scope: &PageScope, page_id: &str) -> Result<(), PlaneError> {
        let path = format!("{}{}/", scope.base_path(&self.workspace), page_id);
        let _: serde_json::Value = self
            .request_json(reqwest::Method::DELETE, &path, &[], None)
            .await?;
        self.invalidate(&scope.cache_prefix(&self.workspace));
        Ok(())
    }

    /// `GET .../projects/{pid}/intake-issues/` — all pages (cached).
    ///
    /// The API returns an empty list whenever the project's intake view is
    /// off; there is no client-side gate (the Python CLI does the same).
    pub async fn list_intake(&self, project_id: &str) -> Result<Vec<IntakeItem>, PlaneError> {
        let key = format!("intake:{}:{}", self.workspace, project_id);
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/intake-issues/",
            self.workspace, project_id
        );
        self.cached_list(&key, TTL_INTAKE, async move { self.paginate(&path).await })
            .await
    }

    /// `POST .../projects/{pid}/intake-issues/` — submit a work item to the
    /// project's intake queue.
    pub async fn create_intake(
        &self,
        project_id: &str,
        body: &IntakeWrite,
    ) -> Result<IntakeItem, PlaneError> {
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/intake-issues/",
            self.workspace, project_id
        );
        let result = self
            .request_json(reqwest::Method::POST, &path, &[], Some(&body_json(body)?))
            .await;
        self.invalidate_intake(project_id);
        result
    }

    /// `PATCH .../projects/{pid}/intake-issues/{work_item_id}/` — set the
    /// intake status (accept = 1, decline = -1).
    ///
    /// Returns the raw parsed response so the caller can verify the write:
    /// a non-project-Admin caller gets HTTP 200 with the record unchanged
    /// (ADR-0007), so a 200 alone is not proof the triage happened.
    pub async fn update_intake_status(
        &self,
        project_id: &str,
        work_item_id: &str,
        status: i64,
    ) -> Result<serde_json::Value, PlaneError> {
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/intake-issues/{}/",
            self.workspace, project_id, work_item_id
        );
        let body = serde_json::json!({ "status": status });
        let result = self
            .request_json(reqwest::Method::PATCH, &path, &[], Some(&body))
            .await;
        self.invalidate_intake(project_id);
        result
    }

    /// `DELETE .../projects/{pid}/intake-issues/{work_item_id}/` — remove an
    /// intake item. For any status other than `accepted` the server also
    /// permanently deletes the underlying work item.
    pub async fn delete_intake(
        &self,
        project_id: &str,
        work_item_id: &str,
    ) -> Result<(), PlaneError> {
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/intake-issues/{}/",
            self.workspace, project_id, work_item_id
        );
        let _: serde_json::Value = self
            .request_json(reqwest::Method::DELETE, &path, &[], None)
            .await?;
        self.invalidate_intake(project_id);
        Ok(())
    }

    /// Drop the intake queue cache and the work-item cache for a project.
    /// Every intake write also creates/moves/deletes the underlying work item,
    /// so both caches are stale afterwards (mirrors the Python
    /// `invalidate_resource("work_items", ...)` on intake mutations).
    fn invalidate_intake(&self, project_id: &str) {
        self.invalidate(&format!("intake:{}:{}", self.workspace, project_id));
        self.invalidate(&format!("work_items:{}:{}", self.workspace, project_id));
    }
}

/// URL path for a work item's attachments collection (ends with `/`).
fn attachments_path(workspace: &str, project_id: &str, work_item_id: &str) -> String {
    format!(
        "/api/v1/workspaces/{workspace}/projects/{project_id}/work-items/{work_item_id}/attachments/"
    )
}

/// URL path for a work item's relations collection (ends with `/`).
fn relations_path(workspace: &str, project_id: &str, work_item_id: &str) -> String {
    format!(
        "/api/v1/workspaces/{workspace}/projects/{project_id}/work-items/{work_item_id}/relations/"
    )
}

/// Cache key for one work item's attachment list.
fn attachment_cache_key(workspace: &str, project_id: &str, work_item_id: &str) -> String {
    format!("attachments:{workspace}:{project_id}:{work_item_id}")
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
struct PageEnvelope<T> {
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

#[cfg(test)]
mod http_tests {
    //! Command-layer HTTP tests against a lightweight mock server (mockito).
    //! Pins URL building, parsing, and error mapping without a real instance.
    use super::*;
    use planebotcli_core::Config;

    fn cfg(base: &str) -> Config {
        Config {
            base_url: base.to_string(),
            api_key: "test-key".into(),
            workspace: "ws".into(),
        }
    }

    #[tokio::test]
    async fn get_me_parses_user() {
        let mut server = mockito::Server::new_async().await;
        let m = server
            .mock("GET", "/api/v1/users/me/")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"id":"u1","display_name":"Bot","first_name":"B","last_name":"","email":"b@x"}"#,
            )
            .create_async()
            .await;
        let client = PbotClient::with_cache(&cfg(&server.url()), true).unwrap();
        let me = client.get_me().await.unwrap();
        assert_eq!(me.id, "u1");
        assert_eq!(me.display_name.as_deref(), Some("Bot"));
        m.assert_async().await;
    }

    #[tokio::test]
    async fn list_projects_parses_results() {
        let mut server = mockito::Server::new_async().await;
        let m = server
            .mock("GET", "/api/v1/workspaces/ws/projects/")
            .match_query(mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"results":[{"id":"p1","name":"One","identifier":"ONE","created_at":"2026-01-01T00:00:00Z"}],"next_cursor":null,"next_page_results":false}"#)
            .create_async()
            .await;
        let client = PbotClient::with_cache(&cfg(&server.url()), true).unwrap();
        let projects = client.list_projects().await.unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].identifier.as_deref(), Some("ONE"));
        m.assert_async().await;
    }

    #[tokio::test]
    async fn error_400_renders_field_detail() {
        let mut server = mockito::Server::new_async().await;
        let m = server
            .mock("GET", "/api/v1/users/me/")
            .with_status(400)
            .with_header("content-type", "application/json")
            .with_body(r#"{"name":["This field is required."]}"#)
            .create_async()
            .await;
        let client = PbotClient::with_cache(&cfg(&server.url()), true).unwrap();
        let err = client.get_me().await.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("HTTP 400"), "{msg}");
        assert!(msg.contains("name: This field is required."), "{msg}");
        assert_eq!(err.exit_code(), planebotcli_core::errors::EXIT_API);
        m.assert_async().await;
    }

    #[tokio::test]
    async fn error_404_maps_to_not_found_exit3() {
        let mut server = mockito::Server::new_async().await;
        let m = server
            .mock("GET", "/api/v1/workspaces/ws/projects/p1/work-items/wi1")
            .match_query(mockito::Matcher::Any)
            .with_status(404)
            .create_async()
            .await;
        let client = PbotClient::with_cache(&cfg(&server.url()), true).unwrap();
        let err = client.get_work_item("p1", "wi1").await.unwrap_err();
        assert_eq!(err.exit_code(), planebotcli_core::errors::EXIT_NOT_FOUND);
        m.assert_async().await;
    }

    #[tokio::test]
    async fn set_parent_sends_the_uuid() {
        let mut server = mockito::Server::new_async().await;
        let m = server
            .mock("PATCH", "/api/v1/workspaces/ws/projects/p1/work-items/wi1")
            .match_body(mockito::Matcher::Json(serde_json::json!({"parent": "wi2"})))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"id":"wi1","parent":"wi2"}"#)
            .create_async()
            .await;
        let client = PbotClient::with_cache(&cfg(&server.url()), true).unwrap();
        client
            .set_work_item_parent("p1", "wi1", Some("wi2"))
            .await
            .unwrap();
        m.assert_async().await;
    }

    #[tokio::test]
    async fn clear_parent_sends_an_explicit_null() {
        let mut server = mockito::Server::new_async().await;
        // The matcher demands the literal `"parent": null` — a null-stripped
        // `{}` body must NOT satisfy it.
        let m = server
            .mock("PATCH", "/api/v1/workspaces/ws/projects/p1/work-items/wi1")
            .match_body(mockito::Matcher::Json(serde_json::json!({"parent": null})))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"id":"wi1","parent":null}"#)
            .create_async()
            .await;
        let client = PbotClient::with_cache(&cfg(&server.url()), true).unwrap();
        client
            .set_work_item_parent("p1", "wi1", None)
            .await
            .unwrap();
        m.assert_async().await;
    }

    #[tokio::test]
    async fn list_relations_returns_raw_buckets() {
        let mut server = mockito::Server::new_async().await;
        let body = r#"{"blocking":[{"project_id":"p1","issue_id":"wi2"}],"blocked_by":[],"duplicate":[],"relates_to":[],"start_after":[],"start_before":[],"finish_after":[],"finish_before":[]}"#;
        let m = server
            .mock(
                "GET",
                "/api/v1/workspaces/ws/projects/p1/work-items/wi1/relations/",
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body)
            .create_async()
            .await;
        let client = PbotClient::with_cache(&cfg(&server.url()), true).unwrap();
        let relations = client.list_relations("p1", "wi1").await.unwrap();
        assert_eq!(relations["blocking"][0]["issue_id"], "wi2");
        m.assert_async().await;
    }

    #[tokio::test]
    async fn create_relations_posts_the_type_and_issue_uuids() {
        let mut server = mockito::Server::new_async().await;
        let m = server
            .mock(
                "POST",
                "/api/v1/workspaces/ws/projects/p1/work-items/wi1/relations/",
            )
            .match_body(mockito::Matcher::Json(serde_json::json!({
                "relation_type": "blocking",
                "issues": ["wi2", "wi3"],
            })))
            .with_status(201)
            .with_header("content-type", "application/json")
            .with_body(r#"{"blocking":[{"project_id":"p1","issue_id":"wi2"}]}"#)
            .create_async()
            .await;
        let client = PbotClient::with_cache(&cfg(&server.url()), true).unwrap();
        let created = client
            .create_relations(
                "p1",
                "wi1",
                "blocking",
                &["wi2".to_string(), "wi3".to_string()],
            )
            .await
            .unwrap();
        assert!(created["blocking"].is_array());
        m.assert_async().await;
    }
}
