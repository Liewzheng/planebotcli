---
name: pbot
description: "Manage Plane.so through the pbot / planebotcli CLI — work items, projects, cycles/sprints, modules, labels, states, documents, intake queue, comments, relations. Use when the user mentions Plane, pbot, planebotcli, planecli, or a work-item identifier like ABC-123, or asks about tasks, sprints, or backlogs in a project where Plane is the tracker."
allowed-tools: Bash(pbot *, planebotcli *)
metadata:
  author: planebotcli maintainers
  version: "2.0"
---

# pbot — PlanebotCLI

**The command is `pbot`** (short alias of the full binary `planebotcli`) — a single
static binary installed at `~/.cargo/bin` (both `pbot` and `planebotcli` are
installed). Every command group (whoami, configure, project, wi, comment,
relations, attachment, doc, intake, label, state, module, cycle, user, cache),
fuzzy resolution, `--json` dual output, caching, markdown input and inline images.

Install / update the local binary from the release line after each release:

```bash
cargo install --path <repo>/crates/planebotcli-cli --locked   # repo = integration-main checkout
```

## Key Concepts

- **Fuzzy resolution**: every resource argument (project, state, label, user, work item) accepts a name, an identifier (`ABC-123`), or a UUID; close names resolve.
- **`me`**: the authenticated user, valid wherever an assignee is expected.
- **`--json`**: pass it on every command; JSON goes to stdout, the human table to stderr.
- **Caching**: reads are cached on disk. `--no-cache` bypasses it for one command; `pbot cache clear` resets it. Read back your own writes with `--no-cache`.
- **Project scoping**: most commands take `-p PROJECT`. Identifiers (`ABC-123`) resolve across projects, and `wi ls` without `-p` spans all projects.

## Setup & Authentication

Installed from our release repo `github.com/Liewzheng/planebotcli`, which holds two
long-lived branches: `integration-main` (the integration line where completed tasks
accumulate) and `main` (the released line). The version and the changelog are edited on
`integration-main`; `main` only advances through a PR from it. After each new release, reinstall by
default — do not ask first:
`cargo install --path crates/planebotcli-cli --locked` (run in the integration-main checkout;
installs both `planebotcli` and `pbot` into `~/.cargo/bin`)

Config precedence: CLI flags > env vars (`PLANE_BASE_URL`, `PLANE_API_KEY`, `PLANE_WORKSPACE`) > config file, discovered highest-priority-first from `~/.config/pbot/config.toml` (TOML), `~/.pbot`, `~/.planecli`, then `~/.plane_api` (key=value lines, chmod 600). `pbot configure` keeps writing `~/.plane_api`.

For a **self-hosted** instance (base URL is whatever you host it on — an internal IP, a Tailscale address, or a domain):

- Sanity-check the instance without auth: `curl http://HOST/api/instances/` — confirms setup is done and shows the version.
- The **workspace slug is not the instance name** and there is no "list workspaces" API. Discover it by trying the instance name as the slug, then verify: `curl http://HOST/api/v1/workspaces/SLUG/projects/ -H "X-Api-Key: $TOKEN"` returning project JSON means the slug is right.
- Service tokens (`plane_api_...`) are created in the workspace settings → API tokens on the web UI. A 401 from `/api/instances/admins/` only means "not an instance admin" — evaluate tokens against `/api/v1/...` endpoints instead.

## Quick Reference

Flags below are the common ones; every flag of every command is in
[references/command-reference.md](references/command-reference.md) — read it before guessing a
flag, filter, or sort key.

### Identity & Configuration

```bash
pbot whoami --json          # authenticated user
pbot configure              # interactive setup
pbot users ls --json        # workspace members
```

### Work Items (most common)

```bash
# List / filter
pbot wi ls -p "Project" --state "In Progress" --assignee me --limit 10 --json
pbot wi ls -p "Project" --labels "bug,critical" --sort updated --json
pbot wi ls --assignee me --state "In Progress" --json      # across all projects

# Create
pbot wi create "Title" -p "Project" --assign me --priority urgent --state "Todo" --json
pbot wi create "Sub-task" --parent ABC-123 --assign "Patrick" --labels "backend" --json
pbot wi create "Title" -p "Project" -d "Plain text description." --json   # -d wraps in a paragraph; --desc-md for markdown

# Update
pbot wi update ABC-123 --state "Done" --priority none --json
pbot wi update ABC-123 --assign "Patrick" --labels "bug,urgent" --json

# Other
pbot wi show ABC-123 --json                     # bundles comments (see Gotchas)
pbot wi show ABC-123 --no-comments --json       # skip the comment fetch
pbot wi assign ABC-123 --json                   # assign to yourself
pbot wi assign ABC-123 --assign "Name" --json
pbot wi search "login bug" -p "Project" --json
pbot wi delete ABC-123
```

Priority: `urgent`, `high`, `medium`, `low`, `none` (or `1`–`4`, `0`).

### Attachments & inline images

```bash
pbot attachment ls -p "Project" ABC-123 --json
pbot attachment attach -p "Project" ABC-123 -f ./log.txt --json   # refuses a duplicate name; --force overrides
pbot wi create "Title" -p "Project" -i ./screenshot.png --json    # repeatable -i embeds images in the description
pbot wi update ABC-123 -i ./shot.png --json
```

`-i/--image` uploads the image and appends an `img` tag to the description (on `wi create` the
item is created first, then the description is patched). The tag's `src` holds only the asset
UUID — that is what the web editor resolves at render time; a full path or URL in `src` shows
"Error loading image" on the web UI.

### Projects

```bash
pbot project ls --state started --sort created --json
pbot project show "Frontend" --json
pbot project create "New Project" -i "NP" -d "Description" --json
pbot project update "Name" --name "New Name" --json
pbot project delete "Name"
```

### Cycles (Sprints)

```bash
pbot cycle ls -p "Project" --json
pbot cycle create "Sprint 1" -p "Project" --start-date 2026-02-17 --end-date 2026-03-02 --json
pbot cycle add-item "Sprint 1" ABC-123 -p "Project"
pbot cycle remove-item "Sprint 1" ABC-123 -p "Project"
pbot cycle items "Sprint 1" -p "Project" --json
```

### Intake

`intake ls` returns two ids per row: `id` (the queue wrapper) and `issue_id` (the work item).
`accept`, `decline`, and `delete` take `issue_id`.

```bash
pbot intake ls -p "Project" --json
pbot intake enabled "Project" --json                        # is intake on for the project?
pbot intake create "Login button broken" -p "Project" -d "Steps..." -P high --json

# Triage needs the project Admin role: exit 0 = triaged, exit 4 = the API left the record unchanged
pbot intake accept <issue_id> -p "Project" --json
pbot intake decline <issue_id> -p "Project" --json

# Destructive, no prompt, no undo: for any status other than `accepted` this also deletes the
# work item. Read the status from `intake ls` first; `decline` merely removes it from the queue.
pbot intake delete <issue_id> -p "Project"
```

### Modules, Labels, States, Documents, Comments

```bash
# Modules (--status: backlog, planned, in-progress, paused, completed, cancelled)
pbot module ls -p "Project" --json
pbot module create "Auth" -p "Project" -d "Login flows" --status in-progress --json
pbot module update "Auth" -p "Project" --status completed --json

# Labels
pbot label ls -p "Project" --json
pbot label create "urgent" -p "Project" --color "#FF0000" --json

# States (groups: backlog, unstarted, started, completed, cancelled)
pbot state ls -p "Project" --group started --json
pbot state create "In Review" -p "Project" --group started --color "#FFA500" --json

# Documents
pbot doc ls -p "Project" --json
pbot doc create --title "Spec" --content "Plain text, `code`, https://links" -p "Project" --json
# --content converts like comments (paragraphs/code/links) — NOT a markdown engine.
# Rich layout needs HTML; doc delete archives the page first, then deletes.

# Comments
pbot comment ls ABC-123 --json
pbot comment create ABC-123 --body "Fixed in PR #456" --json
```

## Updating tasks — etiquette

**Progress updates go in comments, never in the description.** The description is the original
requirement; record status, progress, and links (PRs, decisions) with
`pbot comment create ABC-123 --body "..."`. Touch the description only when the user
explicitly asks to change the requirements.

**Update task progress promptly — same turn the work lands.** When a work slice lands (commit
pushed, PR opened or merged, release cut, decision made), post the progress comment and set the
fitting state on the corresponding Plane task immediately. Never batch Plane updates at the end of
a session.

**Comment format** (team standard — write the comment in the team's working language,
Chinese, like the existing task comments):

- One-line conclusion first (`已完成：`, `已合入：`, `阻塞：...`).
- Key links one per line, as bare full URLs — pbot converts them to clickable anchors in
  the comment (the raw API stores them as plain text, it does not auto-link). Markdown link
  syntax is unnecessary:
  ```
  PR https://github.com/<owner>/<repo>/pull/N
  任务 http://HOST/<workspace-slug>/projects/<project-uuid>/issues/<item-uuid>/
  ```
- Close with a parenthetical of technical context: branch name in backticks (rendered as a code
  tag by pbot — the editor stores HTML and does not parse markdown, so the CLI converts
  `code` and fenced blocks itself), what the branch
  contains, and where it landed (`已合入 integration-main`).
- No `-` bullet lists, no restating what the links say, no filler. Multi-line bodies: write to a
  temp file and pass `--body "$(cat file)"`.

Full example:

```
已完成：
PR https://github.com/Liewzheng/planebotcli/pull/N
（分支 `docs/planecli-47-repo-cleanup`，统一 planebotcli 命名 + skill 与 CLI 行为对齐，已合入 integration-main）
```

**Separate items with a blank line.** The comment body is plain text: a blank line starts a new
paragraph, a single newline only becomes a br tag. `1) ...\n2) ...` on adjacent lines renders
joined on the web UI — put an empty line between list items (write the body to a file with real
blank lines, not a one-liner with `\n` escapes).

**No upstream submission.** pbot / planebotcli are an independent distribution: fixes land on the
`integration-main` line and are never opened as issues or pull requests against Plane upstream
or any other repository. There is no upstream-survey step and no `已提交上游：` comment to write —
a task that records a finished fix names the local branch and the merge that carried it.

**Every task gets a label — you pick it.** Creating a work item without a label is incomplete.
Choose the appropriate tag yourself and pass `--labels` on `wi create`: `feat` for features,
`fix` for bug fixes, `docs` for documentation, and so on. Check the project's vocabulary first
(`label ls -p PROJECT`) and reuse existing names; if nothing fits, create the label
(`label create NAME -p PROJECT --color "#D73A4A"`) instead of leaving the task untagged. The same
applies when triaging or updating tasks you touch.

**Task descriptions are documents, not one-liners.** A description must read well on the web UI
for a human who has zero context — set the scene first (背景), then the specifics. Reference
format: PLANECLI-2 (feature) and RENG-1 (epic) in the PLANE/ReviewEngine projects.

- `feat` tasks: 背景与需求 → 技术方案 → 验收标准.
- `fix` tasks are stories about a bug: 背景 → 故障现象（用户看到什么）→
  复现（步骤、频率、条件）→ 原因分析 → 修复方案 → 修复目标 → 补充.
- Write flowing paragraphs, not telegram notes; use headings and lists. Numbers and
  verification evidence (test counts, live checks) beat adjectives.

`--desc-md` accepts native markdown and converts it to HTML client-side — write the description
in markdown and pass `--desc-md "$(cat desc.md)"` (or `-d` for a short plain-text line). Verify
after creating: `wi show ABC-1 --no-cache --json | jq -r .description_html` must contain real h2/li tags.

**New tasks: confirm dates and links with the human first.** When creating a work item:

- The start date is the creation time by default. Always ask the human for an expected end date
  and fill that in — never invent dates silently.
- If you detect related tasks, propose the relationship (related link, or parent/child for
  sub-tasks) but create it only after the human confirms.

NOTE: `wi create`/`wi update` take `--start-date`/`--target-date` (YYYY-MM-DD) natively;
the end-date field is `target_date` on the API, and a wrong field name is silently
ignored — pbot validates the format client-side (exit 5) and verifies the write.

**A progress comment moves the task out of backlog.** Posting a progress update means the task
is being worked on: set the state to In Progress (or another fitting state — e.g. In Review when
the work is done and awaiting merge) in the same breath as `comment create`. Never leave a task
you just updated in Todo/Backlog.

**Only the human closes a task.** Never set Done yourself. A task is complete only when the
human says so, or when the work is merged onto the `integration-main` line. "I finished my
part and pushed" tops out at In Progress — the same applies when correcting a state you set too
eagerly.

## Gotchas

- **Work-item descriptions are stored as HTML.** `wi create` / `wi update -d` wrap plain text in
  a paragraph, so markdown or HTML passed to `-d` renders literally (`##`, backticks, `<h2>`). For
  rich layout use `--desc-md <markdown>` — write the source in markdown and pass
  `--desc-md "$(cat body.md)"` (a file beats a huge inline string). Verify the stored value:
  `wi show ABC-123 --no-cache --json | jq -r .description_html` must contain real tags; a non-empty
  description proves nothing, since malformed input is stored happily. Work items only:
  `intake create -d` HTML-escapes its input, so tags show up as text.
- **Confirm a create against the server before retrying it.** `wi create` prints the created item
  as JSON, so a broken `jq` filter over that output looks exactly like a failed create. Check with
  `wi ls -p PROJECT --no-cache --json | jq -r '.[] | select(.parent=="<parent-uuid>") | .sequence_id'`.
  There is no idempotency key — retrying a create that succeeded silently duplicates the item.
- **`wi show` occasionally returns non-JSON** (`jq: parse error`). Transient — retry once before
  investigating.
- **Web UI URLs use UUIDs, not identifiers.** An issue page is
  `/{workspace}/projects/{project-uuid}/issues/{issue-uuid}/` — identifier-based URLs
  (`.../projects/PLANECLI/issues/PLANECLI-3/`) render "not found". Get both UUIDs from
  `wi ls --json` (project + id) when building links or driving the UI.
- **`*_name` fields from `wi show` are human-readable.** `assignee_names`, `label_names`,
  `label_detail_names`, and `state_detail_name` are resolved through the cached maps (the same
  ones `wi ls` uses), so they carry real names rather than raw UUIDs.
- **`sequence_id` shape differs.** `wi show` / `wi create` return an integer (`204`); `wi ls`
  returns the full identifier as a string (`"PIPERAG-204"`). Build identifiers as
  `sequence_id` from `wi ls`, or `"{project_identifier}-{sequence_id}"` from `wi show`.
- **`wi show` bundles comments and can degrade to `comments: null`.** `[]` = none, a list = some,
  `null` = the comment fetch failed while the work item still returned and the command exited 0.
  Check for `null` explicitly when "no comments" and "couldn't load comments" differ for you.
  `--no-comments` omits the key entirely.
- **Raw API paths take UUIDs only, never `ABC-123`.** If you drop to `curl` against
  `/api/v1/workspaces/WS/projects/PROJ/issues/ISSUE/`, the issue segment must be the work-item
  UUID (get it from `wi ls --json`); the identifier in the URL returns `404 {"error": "Page not
  found."}` — route-level, so it looks like the endpoint doesn't exist. pbot's resolution
  layer exists precisely to hide this.
- **In raw JSON the body is `description_html`, not `description`.** The API's `description`
  field is always null; the rendered content lives in `description_html`. A `wi show --json`
  pipeline that reads `.description` silently sees empty.
- **Priority has no null — "unset" is `none`.** `PATCH {"priority": null}` is rejected
  (`"This field may not be null."`); to clear a priority set `"none"` (pbot:
  `--priority none`).

## Bulk create with rich descriptions

One file per item in markdown, prove the first one renders, then create the rest and count
what landed on the server.

```bash
# 1. write one body per item (01.md, 02.md, ...) in markdown

# 2. create the FIRST item and inspect its stored HTML before going further
pbot wi create "First title" -p "Project" --parent ABC-1 --assign "Name" \
  --state "Todo" --priority high --labels "bug,backend" --desc-md "$(cat 01.md)" --json
pbot wi show ABC-2 --no-cache --json | jq -r .description_html   # expect <h2>, <pre>

# 3. create the remaining items, then verify the whole batch by parent
pbot wi ls -p "Project" --no-cache --json \
  | jq -r '.[] | select(.parent=="<parent-uuid>") | "\(.sequence_id) \(.priority) \(.name)"'
```
