# pbot Command Reference

## Table of Contents
- [Global Options](#global-options)
- [Work Items](#work-items)
- [Attachments](#attachments)
- [Projects](#projects)
- [Cycles](#cycles)
- [Modules](#modules)
- [Labels](#labels)
- [States](#states)
- [Documents](#documents)
- [Comments](#comments)
- [Intake](#intake)
- [Users](#users)
- [Cache](#cache)

## Global Options

| Flag | Description |
|---|---|
| `--verbose` / `-v` | Enable verbose logging |
| `--no-cache` | Bypass cache for this command |
| `--json` | Output JSON to stdout (available on most commands) |
| `--version` | Show version |
| `--help` / `-h` | Show help |

Environment variable `PLANECLI_NO_CACHE=1` disables cache globally.

### Command aliases

| Full | Aliases |
|---|---|
| `work-item` | `wi`, `issues`, `issue` |
| `project` | `projects` |
| `document` | `doc`, `docs`, `documents` |
| `comment` | `comments` |
| `module` | `modules` |
| `label` | `labels` |
| `state` | `states` |
| `cycle` | `cycles` |
| `user` | `users` |
| `list` | `ls` |
| `show` | `read` |
| `create` | `new` |

## Work Items

Command group: `pbot wi` (aliases: `work-item`, `issues`, `issue`)

### wi ls

```
pbot wi ls [OPTIONS]
```

| Flag | Description |
|---|---|
| `--project` / `-p` | Project name, identifier, or UUID (omit for all projects) |
| `--assignee` | Filter by assignee name or `me` |
| `--state` | Filter by state name (comma-separated for OR) |
| `--labels` | Filter by label name (comma-separated for OR) |
| `--sort` | Sort by: `created` (default), `updated` |
| `--limit` / `-l` | Max results (default: 50) |
| `--json` | JSON output |

### wi show

```
pbot wi show ISSUE [OPTIONS]
```

| Parameter | Description |
|---|---|
| `ISSUE` | Work item identifier (ABC-123), UUID, or name (required) |
| `--project` / `-p` | Project (required for name-based lookup) |
| `--no-comments` | Skip fetching the work item's comments |
| `--json` | JSON output |

Bundles the work item's comments in the same call (chronological, oldest → newest;
same data `comment ls` returns). In `--json` output, the `comments` field is a list
(`[]` if there are none), or `null` if the comment fetch failed — the work item
itself still returns and the command still exits 0. In human output, a `Comments`
section follows the work item details, showing `(none)` or `(failed to load)` as
appropriate. Pass `--no-comments` to skip the fetch entirely (the `comments` key is
then omitted from JSON output).

### wi create

```
pbot wi create TITLE [OPTIONS]
```

| Parameter | Description |
|---|---|
| `TITLE` | Work item title (required) |
| `--project` / `-p` | Project (required) |
| `--assignee` / `--assign` | Assignee name, email, or `me` |
| `--state` | State name (e.g. `Todo`, `In Progress`) |
| `--labels` | Comma-separated label names |
| `--priority` | `urgent`, `high`, `medium`, `low`, `none` (or 0-4) |
| `--module` | Module name or UUID |
| `--parent` | Parent work item identifier (ABC-123) for sub-issues |
| `--estimate` / `-e` | Story point estimate |
| `--description` / `-d` | Description. Stored as raw HTML, not markdown — see the Gotchas in SKILL.md |
| `--image` / `-i` | Image file path to embed in the description (repeatable). Uploaded and appended as an img tag |
| `--force` | Upload images even if an attachment with the same file name already exists |
| `--start-date` | Start date `YYYY-MM-DD`. Bad formats are rejected (exit 5) |
| `--target-date` | Target end date `YYYY-MM-DD`. The write is verified against the echoed record |
| `--json` | JSON output |

### wi update

```
pbot wi update ISSUE [OPTIONS]
```

| Parameter | Description |
|---|---|
| `ISSUE` | Work item identifier, UUID, or name (required) |
| `--project` / `-p` | Project (required for name-based lookup) |
| `--state` | New state name |
| `--priority` | New priority |
| `--assignee` / `--assign` | New assignee name or `me` |
| `--labels` | Comma-separated labels to set |
| `--clear-labels` | Remove all labels |
| `--name` | New title |
| `--description` / `-d` | New description. Stored as raw HTML, not markdown |
| `--image` / `-i` | Image file path to embed in the description (repeatable). Uploaded and appended as an img tag |
| `--force` | Upload images even if an attachment with the same file name already exists |
| `--start-date` | New start date `YYYY-MM-DD` |
| `--target-date` | New target end date `YYYY-MM-DD`. The write is verified against the echoed record |
| `--json` | JSON output |

### wi delete

```
pbot wi delete ISSUE [OPTIONS]
```

| Parameter | Description |
|---|---|
| `ISSUE` | Work item identifier, UUID, or name (required) |
| `--project` / `-p` | Project (required for name-based lookup) |

### wi search

```
pbot wi search QUERY [OPTIONS]
```

| Parameter | Description |
|---|---|
| `QUERY` | Search text (required) |
| `--project` / `-p` | Project (required) |
| `--sort` | Sort by: `created` (default), `updated` |
| `--limit` / `-l` | Max results (default: 50) |
| `--json` | JSON output |

### wi assign

```
pbot wi assign ISSUE [OPTIONS]
```

| Parameter | Description |
|---|---|
| `ISSUE` | Work item identifier, UUID, or name (required) |
| `--assignee` / `--assign` | Assignee (default: `me`) |
| `--project` / `-p` | Project (required for name-based lookup) |

## Attachments

Command group: `pbot attachment` (alias: `attachments`)

```
pbot attachment ls ISSUE [-p PROJECT] [--json]
pbot attachment attach ISSUE -f FILE [-p PROJECT] [--force] [--json]
```

`attach` aliases: `upload`, `new`; `ls` alias: `list`.

| Parameter | Description |
|---|---|
| `ISSUE` | Work item identifier, UUID, or name (required) |
| `-f` / `--file` | Local file path to upload (required on `attach`) |
| `--project` / `-p` | Project (required for name-based lookup) |
| `--force` | Upload even if an attachment with the same file name already exists on the issue |
| `--json` | JSON output |

Upload follows the API's three-step flow: register an asset (presigned S3 URL), `PUT` the
binary, then mark `is_uploaded` and re-read the asset to verify. Without `--force`, a duplicate
file name prompts for confirmation on a TTY and is rejected in non-interactive runs.

Description images uploaded via `wi create -i` / `wi update -i` use the same upload but are not
listed as attachments; the img tag's `src` holds only the asset UUID, which the web editor
resolves at render time.

## Projects

Command group: `pbot project` (alias: `projects`)

### project ls

```
pbot project ls [OPTIONS]
```

| Flag | Description |
|---|---|
| `--state` / `-s` | Filter: `planned`, `started`, `paused`, `completed`, `canceled` |
| `--limit` / `-l` | Max results (default: 50) |
| `--sort` | Sort: `linear` (default), `created`, `updated` |
| `--json` | JSON output |

### project show

```
pbot project show PROJECT [OPTIONS]
```

### project create

```
pbot project create NAME [OPTIONS]
```

| Parameter | Description |
|---|---|
| `NAME` | Project name (required) |
| `--identifier` / `-i` | Short identifier (e.g. `API`) |
| `--description` / `-d` | Description |
| `--json` | JSON output |

### project update / delete

```
pbot project update PROJECT [OPTIONS]
pbot project delete PROJECT
```

## Cycles

Command group: `pbot cycle` (alias: `cycles`)

### cycle ls / show / create / update / delete

```
pbot cycle ls -p PROJECT
pbot cycle show CYCLE -p PROJECT
pbot cycle create NAME -p PROJECT --start-date YYYY-MM-DD --end-date YYYY-MM-DD
pbot cycle update CYCLE -p PROJECT [OPTIONS]
pbot cycle delete CYCLE -p PROJECT
```

### cycle add-item / remove-item / items

```
pbot cycle add-item CYCLE ISSUE -p PROJECT
pbot cycle remove-item CYCLE ISSUE -p PROJECT
pbot cycle items CYCLE -p PROJECT
```

## Modules

Command group: `pbot module` (alias: `modules`)

```
pbot module ls -p PROJECT
pbot module show MODULE -p PROJECT
pbot module create NAME -p PROJECT [-d DESCRIPTION] [--start-date DATE] [--end-date DATE] [--status STATUS]
pbot module update MODULE -p PROJECT [--name NAME] [-d DESCRIPTION] [--start-date DATE] [--end-date DATE] [--status STATUS]
pbot module delete MODULE -p PROJECT
```

**Module status values** (`--status`): `backlog`, `planned`, `in-progress`, `paused`, `completed`, `cancelled`.
Also accepts Portuguese aliases: `planejado`, `em andamento`, `pausado`, `concluído`, `cancelado` (and `canceled`, `in progress`).

## Labels

Command group: `pbot label` (alias: `labels`)

```
pbot label ls -p PROJECT
pbot label show LABEL -p PROJECT
pbot label create NAME -p PROJECT [--color "#HEX"]
pbot label update LABEL -p PROJECT [--name NAME] [--color "#HEX"]
pbot label delete LABEL -p PROJECT
```

## States

Command group: `pbot state` (alias: `states`)

State groups: `backlog`, `unstarted`, `started`, `completed`, `cancelled`, `triage`

```
pbot state ls -p PROJECT [--group GROUP]
pbot state show STATE -p PROJECT
pbot state create NAME -p PROJECT --group GROUP [--color "#HEX"]
pbot state update STATE -p PROJECT [--color "#HEX"]
pbot state delete STATE -p PROJECT
```

## Documents

Command group: `pbot doc` (aliases: `document`, `documents`, `docs`)

```
pbot doc ls -p PROJECT
pbot doc show TITLE -p PROJECT
pbot doc create --title TITLE --content CONTENT -p PROJECT
pbot doc update TITLE --content CONTENT -p PROJECT
pbot doc delete TITLE -p PROJECT
```

## Comments

Command group: `pbot comment` (alias: `comments`)

```
pbot comment ls ISSUE [--project/-p PROJECT] [--limit/-l N]
pbot comment create ISSUE --body "TEXT" [--project/-p PROJECT]
pbot comment update COMMENT_ID --issue ISSUE --body "TEXT" [--project/-p PROJECT]
pbot comment delete COMMENT_ID --issue ISSUE [--project/-p PROJECT]
```

`--limit` (default 50) selects the **most recent** N comments, still rendered
oldest → newest — not the first N chronologically. A limit of `0` or a negative
value returns no comments (consistent with the `[:limit]` semantics used elsewhere,
where `0` means "none").

## Intake

Command group: `pbot intake` (no alias)

```
pbot intake ls -p PROJECT
pbot intake create NAME -p PROJECT [--description/-d TEXT] [--priority/-P PRIORITY]
pbot intake accept ISSUE_ID -p PROJECT
pbot intake decline ISSUE_ID -p PROJECT
pbot intake delete ISSUE_ID -p PROJECT
pbot intake enabled PROJECT
```

| Parameter | Description |
|---|---|
| `ISSUE_ID` | **Work item UUID** — the `Issue ID` column of `intake ls`, not the intake wrapper `Intake ID` |
| `--project` / `-p` | Project name, identifier, or UUID (required on every subcommand except `enabled`, which takes the project as its argument) |
| `--description` / `-d` | Item description, plain text (wrapped in a paragraph tag and HTML-escaped) |
| `--priority` / `-P` | `none` (default), `low`, `medium`, `high`, `urgent` — an unknown value exits `5` |
| `--json` | JSON output (all subcommands except `delete`) |

Item `status` is reported as a label: `pending`, `rejected`, `snoozed`, `accepted`, `duplicate`.

`accept` and `decline` **require the project Admin role**. Plane answers `HTTP 200` with the
record unchanged for lower roles, so the CLI compares the returned status against the requested
one and exits `4` ("the intake status was not changed") instead of reporting a false success.

`delete` is destructive beyond the queue: for any status other than `accepted` it also
**permanently deletes the underlying work item**. There is no confirmation prompt and no undo —
use `decline` when the intent is only to reject the submission.

`ls` returns an empty list when the project's intake view is off; `enabled` reports the same
project flag (`intake_enabled`).

## Users

```
pbot users ls        # List workspace members
pbot whoami          # Show authenticated user
```

## Cache

```
pbot cache clear     # Clear all cached data
```
