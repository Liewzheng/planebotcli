# planebotcli CLI Command Reference

Covers the Python line **v0.7.0** — every subcommand, flag, alias, and an
example. Global flags (available on every command):

| Flag | Meaning |
|---|---|
| `--json` | Output JSON to **stdout**; the human table goes to **stderr** (`--json 2>/dev/null` = clean JSON) |
| `--no-cache` | Bypass the disk cache for this one command |
| `-v` / `--verbose` | Verbose logging |

Resource arguments accept a **name, identifier (`ABC-123`), or UUID** (fuzzy).
`me` is the authenticated user and works wherever an assignee is expected.

---

## Identity & configuration

```bash
planebotcli whoami [--json]            # current authenticated user
planebotcli configure                  # interactive credential setup (~/.plane_api)
planebotcli cache clear                # wipe the disk cache
```

## project

```bash
planebotcli project ls [--state STATE] [--sort created|linear] [-l N] [--json]
planebotcli project show PROJECT [--json]
planebotcli project create "NAME" [-i IDENTIFIER] [-d DESCRIPTION] [--json]
planebotcli project update PROJECT [--name NEW] [--identifier NEW] [-d DESCRIPTION] [--json]
planebotcli project delete PROJECT
```

Flags: `-p`/`--project`-style scoping is implicit; `--state` filters by
project state (`started`, `active`, etc.); `--sort` and `-l/--limit` order and
cap results.

## work item — `wi` (aliases `work-item`, `issues`, `issue`)

```bash
planebotcli wi ls [-p PROJ] [--assignee NAME|me] [--state S] [--labels a,b]
                  [--parent ABC-1] [--sort created|updated] [-l N] [--json]
planebotcli wi show ABC-123 [--no-comments] [--json]   # bundles comments unless --no-comments
planebotcli wi create "TITLE" -p PROJ [--assign NAME|me] [--state S] [--labels a,b]
                  [--priority urgent|high|medium|low|none] [--module M] [--parent ABC-1]
                  [-e N] [-d "<p>HTML</p>"] [--desc-md "markdown"] [--start-date YYYY-MM-DD]
                  [--target-date YYYY-MM-DD] [-i IMG ...] [--force] [--json]
planebotcli wi update ABC-123 [-p PROJ] [--state S] [--priority P] [--assign NAME]
                  [--labels a,b] [--clear-labels] [--name NEW] [-e N]
                  [-d HTML] [--desc-md MD] [--start-date D] [--target-date D]
                  [-i IMG ...] [--force] [--json]
planebotcli wi delete ABC-123 [-p PROJ]
planebotcli wi search "QUERY" [-p PROJ] [-l N] [--json]
planebotcli wi assign ABC-123 [--assign NAME]   # defaults to yourself
```

Notes:
- `--priority`: `urgent|high|medium|low|none`, or `1`–`4` / `0` (0=none).
- `-d/--description` stores raw **HTML**; `--desc-md` accepts a markdown subset
  (headings, lists, code, bold/italic, auto-linked URLs). They are mutually
  exclusive.
- `-i/--image` (repeatable) uploads an image and embeds it in the description.
- `--start-date` / `--target-date` are `YYYY-MM-DD`, validated client-side
  (exit 5 on a bad format); the write is verified by read-back.
- `--state` / `--labels` are validated against the project before writing —
  an unknown name fails with exit 5 and lists the available values.
- `wi show` JSON output includes `web_url` (a browsable link) and resolves
  `*_name` fields to human-readable names.

## comment

```bash
planebotcli comment ls ABC-123 [-l N] [--json]
planebotcli comment create ABC-123 [--body "text"] [--body-md "markdown"] [-p PROJ] [--json]
planebotcli comment update COMMENT-ID --issue ABC-123 [--body "text"] [--body-md "md"] [--json]
planebotcli comment delete COMMENT-ID --issue ABC-123 [-p PROJ]
```

`--body` (plain text) and `--body-md` (markdown subset) are mutually exclusive;
exactly one is required. Plain text is converted to HTML: blank-line
paragraphs, `br` for single newlines, `` `code` ``, and bare URLs → anchors.

## attachment

```bash
planebotcli attachment attach ABC-123 -f ./file.txt [-p PROJ] [--force] [--json]
planebotcli attachment ls ABC-123 [-p PROJ] [--json]
```

The upload is a three-step flow: register metadata (returns a pre-signed S3
form), POST the binary to the pre-signed URL, then mark the attachment
uploaded. `--force` skips the "name already exists" prompt.

## document — `doc`

```bash
planebotcli doc ls [-p PROJ] [--json]
planebotcli doc show DOC [-p PROJ] [--json]
planebotcli doc create --title "T" [--content "text"] [-p PROJ] [--json]
planebotcli doc update DOC [--title T] [--content C] [-p PROJ] [--json]
planebotcli doc delete DOC [-p PROJ]
```

`--content` converts plain text to HTML like comments (paragraphs/code/links)
— not a markdown engine; rich layout needs HTML. `doc delete` archives the
page first, then deletes.

## intake

```bash
planebotcli intake ls [-p PROJ] [--json]            # columns: id (wrapper), issue_id (work item)
planebotcli intake create "TITLE" [-p PROJ] [-d "HTML"] [-P priority] [--json]
planebotcli intake accept ISSUE_ID [-p PROJ] [--json]
planebotcli intake decline ISSUE_ID [-p PROJ] [--json]
planebotcli intake delete ISSUE_ID [-p PROJ]
planebotcli intake enabled PROJ [--json]
```

- `accept` / `decline` / `delete` take the **work item UUID** (`issue_id`),
  not the intake wrapper `id`.
- `accept` / `decline` need the project Admin role; if the API silently
  ignores the change, the CLI fails loudly (exit 4).
- `delete` is destructive: for any status other than `accepted` it also deletes
  the underlying work item. Documented, never prompted.

## label / state / module

```bash
planebotcli label ls [-p PROJ] [--json]       planebotcli state ls [-p PROJ] [--group G] [--json]
planebotcli label show L [-p PROJ] [--json]   planebotcli state show S [-p PROJ] [--json]
planebotcli label create "N" [-p PROJ] [--color "#RRGGBB"] [--json]
planebotcli label update L [-p PROJ] [--name N] [--color C] [--json]
planebotcli label delete L [-p PROJ]
planebotcli state create "N" [-p PROJ] [--group backlog|unstarted|started|completed|cancelled] [--color C] [--json]
planebotcli state update S [-p PROJ] [--name N] [--group G] [--color C] [--json]
planebotcli state delete S [-p PROJ]

planebotcli module ls [-p PROJ] [--json]
planebotcli module show M [-p PROJ] [--json]
planebotcli module create "N" [-p PROJ] [-d DESC] [--start-date D] [--end-date D] [--status S] [--json]
planebotcli module update M [-p PROJ] [--name N] [--status S] [--json]
planebotcli module delete M [-p PROJ]
```

Module `--status`: `backlog | planned | in-progress | paused | completed |
cancelled` (English and Portuguese aliases accepted).

## cycle

```bash
planebotcli cycle ls [-p PROJ] [--json]
planebotcli cycle show C [-p PROJ] [--json]
planebotcli cycle create "NAME" [-p PROJ] [-d DESC] [--start-date D] [--end-date D] [--json]
planebotcli cycle update C [-p PROJ] [--name N] [--start-date D] [--end-date D] [--json]
planebotcli cycle delete C [-p PROJ]
planebotcli cycle add-item C ABC-1 [-p PROJ]
planebotcli cycle remove-item C ABC-1 [-p PROJ]
planebotcli cycle items C [-p PROJ] [--json]
```

## user

```bash
planebotcli user ls [--json]   # workspace members
```

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Success |
| 1 | Generic error |
| 2 | Authentication error |
| 3 | Resource not found |
| 4 | API error (HTTP status + field-level detail in the message) |
| 5 | Validation error (client-side) |

## Config file

`~/.plane_api` — `key=value` lines, lowercase keys (`base_url`, `api_key`,
`workspace`), `#` comments, `chmod 600`. Precedence: CLI flags > env vars
(`PLANE_BASE_URL`, `PLANE_API_KEY`, `PLANE_WORKSPACE`) > file. Source of truth:
`src/planecli/config.py` (`_FIELD_MAP`).
