# pbot

**English** · [中文](cli-command-reference.zh.md)

## NAME

`pbot` — command-line client for [Plane.so](https://plane.so) (SaaS or self-hosted): work items, projects, cycles, modules, labels, states, documents, intake queues, and comments.

`pbot` is an alias of `planebotcli`; both names run the same binary.

## SYNOPSIS

```
pbot [global flags] <command> [<subcommand>] [<argument>...] [flags]
```

## DESCRIPTION

Every resource argument accepts a **name**, an **identifier** (`ABC-123`), or a **UUID**; close names resolve by fuzzy match (token-sort-ratio, threshold 60). The authenticated user can be referenced as `me` wherever an assignee is expected.

Human-readable tables are written to **stderr**, machine-readable JSON to **stdout**, so `pbot ... --json 2>/dev/null` always yields clean JSON. Reads are cached on disk; writes invalidate the affected resource and, where the API can silently ignore a write (dates, triage, uploads), the result is verified by reading it back.

## GLOBAL OPTIONS

| Option | Description |
|---|---|
| `--json` | Print JSON to stdout; the human table goes to stderr. |
| `--no-cache` | Bypass the disk cache for this one command. |
| `-h, --help` | Show help for any command. |
| `-V, --version` | Print the version. |

## ENVIRONMENT

| Variable | Description |
|---|---|
| `PLANE_BASE_URL` | Instance base URL, e.g. `https://api.plane.so`. |
| `PLANE_API_KEY` | Service token (`plane_api_...`). |
| `PLANE_WORKSPACE` | Workspace slug (not the instance name). |

Configuration file `~/.plane_api` — `key=value` lines with lowercase keys `base_url`, `api_key`, `workspace`; `chmod 600`. Precedence: CLI flags > environment variables > `~/.plane_api`.

> Self-hosted instances behind a proxy: run `env -u http_proxy -u https_proxy -u HTTP_PROXY -u HTTPS_PROXY -u all_proxy pbot ...` so CLI traffic reaches the instance directly.

## EXIT CODES

| Code | Meaning |
|---|---|
| 0 | Success |
| 1 | Generic error |
| 2 | Authentication error |
| 3 | Resource not found |
| 4 | API error (includes HTTP status and field-level detail) |
| 5 | Validation error (client-side) |

## COMMANDS

| Command | Purpose |
|---|---|
| [`whoami`](#pbot-whoami) | Show the authenticated user. |
| [`configure`](#pbot-configure) | Write `~/.plane_api` interactively. |
| [`user`](#pbot-user) | Workspace members. |
| [`cache`](#pbot-cache) | Manage the local disk cache. |
| [`project`](#pbot-project) | Projects. |
| [`wi`](#pbot-wi) | Work items (aliases: `work-item`, `issues`, `issue`). |
| [`relations`](#pbot-relations) | Work-item relations (alias: `relation`). |
| [`comment`](#pbot-comment) | Comments on work items. |
| [`label`](#pbot-label) | Labels. |
| [`state`](#pbot-state) | States. |
| [`module`](#pbot-module) | Modules. |
| [`cycle`](#pbot-cycle) | Cycles (sprints). |
| [`intake`](#pbot-intake) | Intake queue. |
| [`doc`](#pbot-doc) | Documents / pages. |
| [`attachment`](#pbot-attachment) | Work-item attachments (alias: `attachments`). |

---

## pbot whoami

Show the authenticated user.

```
pbot whoami [--json]
```

## pbot configure

Write credentials to `~/.plane_api` interactively (prompts for base URL, API key, workspace slug), then clear the disk cache.

```
pbot configure
```

## pbot user

### pbot user list

List workspace members.

**Aliases** `ls`

```
pbot user list [--json]
```

## pbot cache

### pbot cache clear

Remove the local disk cache.

```
pbot cache clear
```

---

## pbot project

### pbot project list

List projects in the workspace.

**Aliases** `ls`

```
pbot project list [--json]
```

**Options**

| Option | Description |
|---|---|
| `--state <state>` | Filter by project state. |
| `--sort <field>` | Sort by `created` or `linear` (default `linear`). |
| `-l, --limit <n>` | Maximum results (default 50). |

### pbot project show

Show project details.

```
pbot project show <project> [--json]
```

### pbot project create

Create a project.

```
pbot project create <name> [-i <identifier>] [-d <description>] [--json]
```

**Options**

| Option | Description |
|---|---|
| `-i, --identifier <id>` | Short identifier (auto-generated from the name if omitted). |
| `-d, --description <text>` | Description. |

### pbot project update

Update a project.

```
pbot project update <project> [--name <name>] [-i <identifier>] [-d <description>] [--json]
```

### pbot project delete

Delete a project.

```
pbot project delete <project>
```

---

## pbot wi

Manage work items. **Aliases for the group:** `work-item`, `issues`, `issue`.

### pbot wi list

List work items. Without `-p`, lists across all projects.

```
pbot wi list [-p <project>] [--assignee <name|me>] [--state <s>] [--labels <a,b>]
             [--parent <id>] [--sort <field>] [-l <n>] [--json]
```

**Aliases** `ls`

**Options**

| Option | Description |
|---|---|
| `-p, --project <name\|id>` | Project name, identifier, or UUID. Omit to span all projects. |
| `--assignee <name\|me>` | Filter by assignee. |
| `--state <s>` | Filter by state name (comma-separated). |
| `--labels <a,b>` | Filter by label name (comma-separated). |
| `--parent <id>` | List child work items of a parent (`ABC-123`, UUID, or name). |
| `--sort <field>` | `created` (default) or `updated`. |
| `-l, --limit <n>` | Maximum results (default 50). |

**Examples**

```
$ pbot wi ls -p PLANECLI --state "In Progress" --assignee me --json
$ pbot wi ls -p PLANECLI --parent PLANECLI-38 --json
```

### pbot wi show

Show work item details. JSON includes `web_url`, `parent`, and `sub_issues`.

```
pbot wi show <issue> [-p <project>] [--no-comments] [--json]
```

**Options**

| Option | Description |
|---|---|
| `-p, --project <name\|id>` | Project name/ID (required for name-based lookup). |
| `--no-comments` | Skip fetching comments. |

### pbot wi create

Create a work item.

**Aliases** `new`

```
pbot wi create <title> [-p <project>] [--assign <name|me>] [--state <s>]
              [--labels <a,b>] [--priority <p>] [--parent <id>]
              [-d <text> | --desc-md <markdown>] [--start-date <date>]
              [--target-date <date>] [-i <file>...] [--force] [--json]
```

**Options**

| Option | Description |
|---|---|
| `-p, --project <name\|id>` | Project (required). |
| `--assignee, --assign <name\|me>` | Assignee. |
| `--state <s>` | State name (validated against the project). |
| `--labels <a,b>` | Comma-separated label names (validated against the project). |
| `--priority <p>` | `urgent`, `high`, `medium`, `low`, `none` (or `1`–`4`, `0`). |
| `--parent <id>` | Parent work item (`ABC-123`, UUID, or name). |
| `-d, --description <text>` | Description as plain text (wrapped in a paragraph). |
| `--desc-md <markdown>` | Description as native markdown (headings, lists, code, links). Mutually exclusive with `-d`. |
| `--start-date <YYYY-MM-DD>` | Start date (validated; verified by read-back). |
| `--target-date <YYYY-MM-DD>` | Target end date. |
| `-i, --image <file>` | Upload an image and embed it in the description (repeatable). |
| `--force` | Upload images even if an attachment with the same name exists. |

**Examples**

```
$ pbot wi create "Fix login" -p PLANECLI --assign me --state Todo --priority high --json
$ pbot wi create "Sub-task" -p PLANECLI --parent PLANECLI-9 --desc-md "# 背景

- 要点" --json
```

### pbot wi update

Update a work item. Date and parent writes are verified by read-back.

```
pbot wi update <issue> [-p <project>] [--state <s>] [--priority <p>]
              [--assign <name|me>] [--labels <a,b>] [--clear-labels]
              [--name <title>] [-d <text> | --desc-md <markdown>]
              [--start-date <date>] [--target-date <date>]
              [--parent <id>] [--clear-parent] [-i <file>...] [--force] [--json]
```

**Options**

| Option | Description |
|---|---|
| `-p, --project <name\|id>` | Project (needed for name-based lookup). |
| `--state <s>` | New state (validated, lists available on miss). |
| `--priority <p>` | New priority. |
| `--assignee, --assign <name\|me>` | New assignee. |
| `--labels <a,b>` | Set labels (comma-separated). |
| `--clear-labels` | Remove all labels. |
| `--name <title>` | New title. |
| `-d, --description <text>` | New description (plain text). |
| `--desc-md <markdown>` | New description as markdown. Mutually exclusive with `-d`. |
| `--start-date <date>` / `--target-date <date>` | New dates. |
| `--parent <id>` | Set the parent (rejects self, cross-project, and unknown parents locally). |
| `--clear-parent` | Remove the parent. |
| `-i, --image <file>` | Upload an image and append it to the description (repeatable). |
| `--force` | Allow a duplicate-named image upload. |

**Examples**

```
$ pbot wi update PLANECLI-40 --state "In Progress" --json
$ pbot wi update PLANECLI-40 --parent PLANECLI-38 --json
$ pbot wi update PLANECLI-40 --clear-parent --json
```

### pbot wi delete

Delete a work item.

```
pbot wi delete <issue> [-p <project>]
```

### pbot wi search

Search work items by text.

```
pbot wi search <query> [-p <project>] [-l <n>] [--json]
```

### pbot wi assign

Assign a work item (defaults to yourself).

```
pbot wi assign <issue> [--assign <name|me>] [-p <project>]
```

**See also** `pbot relations`, `pbot comment`

---

## pbot relations

Manage work-item relations. **Alias for the group:** `relation`.

Relationship types: `blocking`, `blocked_by`, `duplicate`, `relates_to`, `start_before`, `start_after`, `finish_before`, `finish_after`.

### pbot relations list

List the relations of a work item (all eight buckets).

**Aliases** `ls`

```
pbot relations list <issue> [-p <project>] [--json]
```

### pbot relations add

Create one or more relations from an issue to other issues.

```
pbot relations add <issue> --type <type> --to <target>... [-p <project>] [--json]
```

**Options**

| Option | Description |
|---|---|
| `--type <type>` | One of the eight relationship types. |
| `-t, --to <target>` | Target work item (`ABC-123`, UUID, or name); repeatable. |

**Examples**

```
$ pbot relations add PLANECLI-38 --type relates_to --to PLANECLI-40 -p PLANECLI --json
```

### pbot relations remove

**Not available yet** — the backend exposes no relation-deletion endpoint. The command fails with a validation error and a pointer to the backend request.

---

## pbot comment

### pbot comment list

List comments on a work item (oldest first).

**Aliases** `ls`

```
pbot comment list <issue> [-p <project>] [-l <n>] [--json]
```

### pbot comment create

Add a comment.

**Aliases** `new`

```
pbot comment create <issue> (-b <text> | --body-md <markdown>) [-p <project>] [--json]
```

**Options**

| Option | Description |
|---|---|
| `-b, --body <text>` | Comment text as plain text (paragraphs, `code`, bare URLs converted to HTML). |
| `--body-md <markdown>` | Comment text as native markdown. Mutually exclusive with `-b`. |

### pbot comment update

Update a comment.

```
pbot comment update <comment-id> --issue <issue> (-b <text> | --body-md <markdown>) [--json]
```

### pbot comment delete

Delete a comment.

```
pbot comment delete <comment-id> --issue <issue> [-p <project>]
```

---

## pbot label

### pbot label list

List labels in a project.

**Aliases** `ls`

```
pbot label list -p <project> [--json]
```

### pbot label show

Show label details.

```
pbot label show <label> -p <project> [--json]
```

### pbot label create

Create a label.

```
pbot label create <name> -p <project> [--color <#RRGGBB>] [--json]
```

### pbot label update

Update a label.

```
pbot label update <label> -p <project> [--name <name>] [--color <#RRGGBB>] [--json]
```

### pbot label delete

Delete a label.

```
pbot label delete <label> -p <project>
```

---

## pbot state

### pbot state list

List states in a project.

**Aliases** `ls`

```
pbot state list -p <project> [--group <group>] [--json]
```

**Options**

| Option | Description |
|---|---|
| `--group <group>` | Filter by group: `backlog`, `unstarted`, `started`, `completed`, `cancelled`. |

### pbot state show

Show state details.

```
pbot state show <state> -p <project> [--json]
```

### pbot state create

Create a state.

```
pbot state create <name> -p <project> [--group <group>] [--color <#RRGGBB>] [--json]
```

### pbot state update

Update a state.

```
pbot state update <state> -p <project> [--name <name>] [--group <group>] [--color <#RRGGBB>] [--json]
```

### pbot state delete

Delete a state.

```
pbot state delete <state> -p <project>
```

---

## pbot module

### pbot module list

List modules in a project.

**Aliases** `ls`

```
pbot module list -p <project> [--json]
```

### pbot module show

Show module details.

```
pbot module show <module> -p <project> [--json]
```

### pbot module create

Create a module.

```
pbot module create <name> -p <project> [-d <text>] [--start-date <date>]
                    [--end-date <date>] [--status <status>] [--json]
```

**Options**

| Option | Description |
|---|---|
| `--status <status>` | `backlog`, `planned`, `in-progress`, `paused`, `completed`, `cancelled`. |

### pbot module update

Update a module.

```
pbot module update <module> -p <project> [--name <name>] [-d <text>] [--status <status>]
                    [--start-date <date>] [--end-date <date>] [--json]
```

### pbot module delete

Delete a module.

```
pbot module delete <module> -p <project>
```

---

## pbot cycle

### pbot cycle list

List cycles in a project.

**Aliases** `ls`

```
pbot cycle list -p <project> [--json]
```

### pbot cycle show

Show cycle details.

```
pbot cycle show <cycle> -p <project> [--json]
```

### pbot cycle create

Create a cycle.

```
pbot cycle create <name> -p <project> [-d <text>] [--start-date <date>] [--end-date <date>] [--json]
```

### pbot cycle update

Update a cycle.

```
pbot cycle update <cycle> -p <project> [--name <name>] [--start-date <date>] [--end-date <date>] [--json]
```

### pbot cycle delete

Delete a cycle.

```
pbot cycle delete <cycle> -p <project>
```

### pbot cycle add-item

Add a work item to a cycle.

```
pbot cycle add-item <cycle> <issue> -p <project>
```

### pbot cycle remove-item

Remove a work item from a cycle.

```
pbot cycle remove-item <cycle> <issue> -p <project>
```

### pbot cycle items

List the work items of a cycle.

```
pbot cycle items <cycle> -p <project> [--json]
```

---

## pbot intake

### pbot intake list

List a project's intake queue. Each row carries the queue wrapper `id` and the work item `issue_id`.

**Aliases** `ls`

```
pbot intake list -p <project> [--json]
```

### pbot intake create

Create an intake item.

**Aliases** `new`

```
pbot intake create <name> -p <project> [-d <text>] [-P <priority>] [--json]
```

**Options**

| Option | Description |
|---|---|
| `-d, --description <text>` | Description (HTML-escaped, so tags show as text). |
| `-P, --priority <priority>` | Priority (`none`, `low`, `medium`, `high`, `urgent`). |

### pbot intake accept

Accept an intake item (triage). Requires the project Admin role; the CLI verifies the status actually changed and fails loudly otherwise.

```
pbot intake accept <issue-id> -p <project> [--json]
```

### pbot intake decline

Decline an intake item (triage). Requires the project Admin role.

```
pbot intake decline <issue-id> -p <project> [--json]
```

### pbot intake delete

Delete an intake item. **Destructive:** for any status other than `accepted`, this also deletes the underlying work item.

```
pbot intake delete <issue-id> -p <project>
```

### pbot intake enabled

Report whether intake is enabled for a project.

```
pbot intake enabled <project> [--json]
```

---

## pbot doc

### pbot doc list

List documents. `-p` is required (project pages).

**Aliases** `ls`

```
pbot doc list -p <project> [--json]
```

### pbot doc show

Show document details.

**Aliases** `read`

```
pbot doc show <doc> [-p <project>] [--json]
```

### pbot doc create

Create a document.

**Aliases** `new`

```
pbot doc create --title <title> (--content <text> | --content-md <markdown> | --content-html <html>)
                [-p <project>] [--json]
```

**Options**

| Option | Description |
|---|---|
| `--title <title>` | Page title (required). |
| `-c, --content <text>` | Content as plain text, converted like comments (paragraphs, `code`, links). |
| `--content-md <markdown>` | Content as native markdown (headings, lists, code, links). |
| `--content-html <html>` | Content as raw HTML, stored verbatim (rich layout). |
| `-p, --project <name\|id>` | Project; omit to create a workspace page. |

The three content inputs are mutually exclusive.

**Examples**

```
$ pbot doc create --title "Runbook" -p PLANECLI --content-md "$(cat runbook.md)" --json
$ pbot doc create --title "Spec" -p PLANECLI --content-html "$(cat spec.html)" --json
```

### pbot doc update

Update a document.

```
pbot doc update <doc> [--title <title>] [--content <text> | --content-md <markdown> | --content-html <html>]
                [-p <project>] [--json]
```

### pbot doc archive

Archive (trash) a page without deleting it. The page stays recoverable in the web UI trash.

```
pbot doc archive <doc> [-p <project>]
```

### pbot doc delete

Delete a page. The API only deletes archived pages, so this archives the page first (verifying the archive landed) and then deletes it.

```
pbot doc delete <doc> [-p <project>]
```

---

## pbot attachment

**Alias for the group:** `attachments`.

### pbot attachment attach

Upload a file attachment to a work item (three-step presigned upload, verified by read-back).

**Aliases** `upload`, `new`

```
pbot attachment attach <issue> -f <file> [-p <project>] [--force] [--json]
```

**Options**

| Option | Description |
|---|---|
| `-f, --file <path>` | File to upload. |
| `--force` | Upload even if an attachment with the same name already exists. |

### pbot attachment list

List attachments on a work item.

**Aliases** `ls`

```
pbot attachment list <issue> [-p <project>] [--json]
```

---

## SEE ALSO

- `pbot <command> <subcommand> --help` — per-command help generated from the CLI definition.
- Plane v1 API reference: `docs/api/plane-v1-api.md`.
