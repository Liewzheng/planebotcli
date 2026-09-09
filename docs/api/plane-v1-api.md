# Plane v1 API — planebotcli interface reference

Detailed documentation of every Plane v1 API endpoint the CLI talks to.
This is the interface contract for the Rust rewrite (see
[rust-rewrite.md](../rust-rewrite.md)) and the reference for scripting.

## Conventions

- **Base URL**: `{base_url}/api/v1` — `base_url` is your instance
  (e.g. `http://100.64.0.8`); the workspace slug comes from config.
- **Auth**: every request carries the header `X-Api-Key: <service-token>`.
  The app-only endpoints (`/api/assets/v2/...`) require a browser session
  cookie instead — they are not used for writes via the CLI.
- **Workspace scope**: `{ws}` below is the workspace slug.
- **Pagination**: list endpoints return a cursor envelope:
  `{results: [...], next_cursor, prev_cursor, next_page_results, total_results,
  total_pages, count}`. Page with `?limit=50&cursor=<next_cursor>` until
  `next_page_results` is false.
- **Errors**: non-2xx bodies are JSON, either a top-level `detail` string or
  field-level `{"field": ["message"]}`. The CLI maps HTTP 401 → exit 2,
  404 → exit 3, 429 → retry then exit 4, other → exit 4, and renders field
  errors in the message. Some endpoints return `200` but silently ignore the
  write (see ADR-0007) — the CLI verifies by read-back where it matters.

## users

| Method | Path | Notes |
|---|---|---|
| GET | `/api/v1/users/me/` | Current authenticated user: `id`, `display_name`, `first_name`, `last_name`, `email` |

## workspaces

| Method | Path | Notes |
|---|---|---|
| GET | `/api/v1/workspaces/{ws}/members/` | Workspace members (name resolution for assignees) |

## projects

| Method | Path | Notes |
|---|---|---|
| GET | `/api/v1/workspaces/{ws}/projects/` | List (paginated). Query: `limit`, `cursor` |
| GET | `/api/v1/workspaces/{ws}/projects/{project_id}/` | Detail |
| POST | `/api/v1/workspaces/{ws}/projects/` | Create. Body: `{name, identifier?, description?}` |
| PATCH | `/api/v1/workspaces/{ws}/projects/{project_id}/` | Update: `{name?, identifier?, description?}` |
| DELETE | `/api/v1/workspaces/{ws}/projects/{project_id}/` | Delete |

## work items

| Method | Path | Notes |
|---|---|---|
| GET | `/api/v1/workspaces/{ws}/projects/{project_id}/work-items/` | List (paginated). Query: `assignees`, `state`, `labels`, `parent`, `order_by`, `per_page`/`limit`, `cursor` |
| GET | `/api/v1/workspaces/{ws}/projects/{project_id}/work-items/{id}/` | Detail. Query: `expand=estimate_point`. **Note:** returns `state`/`labels`/`assignees` as raw UUIDs — the CLI resolves names via cached lists |
| POST | `/api/v1/workspaces/{ws}/projects/{project_id}/work-items/` | Create. Body: `{name, description_html?, priority?, state?, assignees?, labels?, parent?, start_date?, target_date?}` |
| PATCH | `/api/v1/workspaces/{ws}/projects/{project_id}/work-items/{id}/` | Update (same fields). **Note:** unknown/misspelled fields are silently ignored with 200 (ADR-0007) — dates are verified by read-back |
| DELETE | `/api/v1/workspaces/{ws}/projects/{project_id}/work-items/{id}/` | Delete |
| GET | `/api/v1/workspaces/{ws}/work-items/search/?query=...` | Text search across the workspace |
| GET | `/api/v1/workspaces/{ws}/work-items/{project_identifier}-{issue_identifier}/` | Lookup by identifier (`ABC-123`). **Note:** detail edits must use the UUID path above |

### work item comments

| Method | Path |
|---|---|
| GET | `/api/v1/workspaces/{ws}/projects/{project_id}/work-items/{id}/comments/` |
| POST | `/api/v1/workspaces/{ws}/projects/{project_id}/work-items/{id}/comments/` — body `{comment_html}` |
| PATCH | `/api/v1/workspaces/{ws}/projects/{project_id}/work-items/{id}/comments/{comment_id}/` |
| DELETE | `/api/v1/workspaces/{ws}/projects/{project_id}/work-items/{id}/comments/{comment_id}/` |

`comment_html` is stored **verbatim** (the editor stores HTML; no server-side
markdown/auto-link). The CLI converts plain text / markdown to HTML before
posting.

### work item attachments

| Method | Path | Notes |
|---|---|---|
| POST | `.../work-items/{id}/attachments/` | Register. Body `{name, type, size}` → returns `upload_data` (S3 pre-signed form) + `asset_id` |
| POST | `<upload_data.url>` | Upload the binary as a multipart form using the pre-signed fields in `upload_data` — **no `X-Api-Key`** |
| PATCH | `.../work-items/{id}/attachments/{asset_id}/` | Finalize: body `{is_uploaded: true}` |
| GET | `.../work-items/{id}/attachments/` | List attachments |

Embedded images: the web editor resolves an `img` whose `src` holds only the
asset UUID at
`/api/assets/v2/workspaces/{ws}/projects/{project_id}/issues/{id}/attachments/{asset_id}/`
(browser session route — this is for display, not API writes).

## labels / states

| Method | Path |
|---|---|
| GET / POST | `/api/v1/workspaces/{ws}/projects/{project_id}/labels/` (POST body: `{name, color?, description?}`) |
| GET / PATCH / DELETE | `/api/v1/workspaces/{ws}/projects/{project_id}/labels/{label_id}/` |
| GET / POST | `/api/v1/workspaces/{ws}/projects/{project_id}/states/` (POST body: `{name, color?, group?, description?}`; group: `backlog/unstarted/started/completed/cancelled`) |
| GET / PATCH / DELETE | `/api/v1/workspaces/{ws}/projects/{project_id}/states/{state_id}/` |

## modules

| Method | Path |
|---|---|
| GET / POST | `/api/v1/workspaces/{ws}/projects/{project_id}/modules/` (POST body: `{name, description?, start_date?, target_date?, status?}`) |
| GET / PATCH / DELETE | `/api/v1/workspaces/{ws}/projects/{project_id}/modules/{module_id}/` |
| POST | `/api/v1/workspaces/{ws}/projects/{project_id}/modules/{module_id}/module-issues/` — add work items `{issues: [uuid...]}` |
| GET | `/api/v1/workspaces/{ws}/projects/{project_id}/modules/{module_id}/module-issues/` — list items |

## cycles

| Method | Path |
|---|---|
| GET / POST | `/api/v1/workspaces/{ws}/projects/{project_id}/cycles/` (POST body: `{name, description?, start_date?, end_date?}`) |
| GET / PATCH / DELETE | `/api/v1/workspaces/{ws}/projects/{project_id}/cycles/{cycle_id}/` |
| GET | `/api/v1/workspaces/{ws}/projects/{project_id}/cycles/{cycle_id}/cycle-issues/` — list items |
| POST | `/api/v1/workspaces/{ws}/projects/{project_id}/cycles/{cycle_id}/cycle-issues/` — add work item |
| DELETE | `/api/v1/workspaces/{ws}/projects/{project_id}/cycles/{cycle_id}/cycle-issues/{work_item_id}/` — remove work item |

## intake

| Method | Path | Notes |
|---|---|---|
| GET | `/api/v1/workspaces/{ws}/projects/{project_id}/intake-issues/` | List queue. Query: `status`, `limit`, `cursor`. Response rows carry a wrapper `id` **and** the work-item UUID in `issue_id` |
| POST | `/api/v1/workspaces/{ws}/projects/{project_id}/intake-issues/` | Create — body `{name, description_html?, priority?}` |
| PATCH | `/api/v1/workspaces/{ws}/projects/{project_id}/intake-issues/{work_item_id}/` | Triage (`accept`/`decline` = status change). **Note:** needs project Admin; a non-admin gets `200` with the record **unchanged** — the CLI compares the field and fails loudly (ADR-0007) |
| DELETE | `/api/v1/workspaces/{ws}/projects/{project_id}/intake-issues/{work_item_id}/` | Delete. **Destructive**: for any status other than `accepted` this also deletes the underlying work item |

`accept`/`decline`/`delete` take the **work item UUID** (the `issue_id`
column), never the intake wrapper `id`.

## pages / documents

| Method | Path | Notes |
|---|---|---|
| GET / POST | `/api/v1/workspaces/{ws}/pages/` | Workspace pages (POST body `{name, description_html}`) |
| GET / POST | `/api/v1/workspaces/{ws}/projects/{project_id}/pages/` | Project pages |
| GET / PATCH | `/api/v1/workspaces/{ws}/.../pages/{page_id}/` | Detail / update (`{name?, description_html?}`) |
| PATCH | `/api/v1/workspaces/{ws}/.../pages/{page_id}/` | Archive — body `{archived_at: "YYYY-MM-DD"}`. **Note:** the API requires `archived_at` (a bare `is_archived` flag is silently ignored, ADR-0007) |
| DELETE | `/api/v1/workspaces/{ws}/.../pages/{page_id}/` | Delete — only works **after** the page is archived |

`description_html` is a **required** field on create (an empty `<p></p>` is
sent when no content is given).

## estimate points

There is **no dedicated endpoint** — estimate points are derived from work
items via `GET .../work-items/?expand=estimate_point`.
