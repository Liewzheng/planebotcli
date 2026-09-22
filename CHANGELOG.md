# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/)
and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- `pbot skill show` / `pbot skill path` / `pbot skill version` print the AI skill (`skill/SKILL.md`) that ships with this build — vendored at compile time via `include_str!`, no network round-trip, no side-channel install. `pbot skill show --json` emits a one-line `{ path, version, skill_md }` object so installers / CI can parse it. Pair with the existing version-lint workflow (PR #26) — skill and CLI versions can no longer be out of sync in any direction.

### Fixed
- Windows `.msi` installer restored. The v1.4.x release intentionally
  dropped `msi` because cargo-dist 0.32's plan step could not detect
  `[package.metadata.wix]` GUIDs in planebotcli-cli's Cargo.toml. Instead
  of waiting on a cargo-dist fix, the release workflow now has a dedicated
  `build-windows-msi` job (windows-latest) that installs `cargo-wix`
  and runs `cargo wix --package planebotcli-cli --output msi` directly.
  The MSI artefact lands on the GitHub release alongside the other
  cargo-dist installers; README's Windows section is restored to link
  to `planebotcli-x86_64-pc-windows-msvc.msi`.
- The v1.4.2 release's first `Release` workflow run errored on
  windows-latest with `ParserError: Missing '(' after 'if'` —
  the default shell on `windows-latest` is PowerShell, which doesn't
  parse `if [ … ]; then … fi`. Three steps in `build-windows-msi`
  (`Install cargo-wix`, `Generate MSI`, `Verify MSI was produced`)
  now declare `shell: bash` explicitly so the bash conditionals
  parse correctly. **Post-merge housekeeping**: the v1.4.2 tag
  currently sits at the broken commit; once this PR is merged,
  re-tag `v1.4.2` against the new `main` HEAD (`git tag -d v1.4.2
  && git tag v1.4.2 && git push planebotcli :refs/tags/v1.4.2
  v1.4.2`) so the `Release` workflow re-runs against the fixed
  workflow.
- v1.4.2's `Release` workflow re-run failed at the `cargo wix` step
  with `[2] (Generic): There are no WXS files to create an installer`
  — cargo-wix 0.3.9 reads `wix/main.wxs` from the package root and
  planebotcli-cli didn't have one. Added a hand-written minimal v3
  WiX template at `crates/planebotcli-cli/wix/main.wxs`
  (Product, single Component for `planebotcli.exe`, single Feature,
  perMachine InstallScope, hardcoded stable UpgradeCode +
  path-guid UUIDs) and added a `Build planebotcli-cli
  (x86_64-pc-windows-msvc, release)` step before `cargo wix` so
  the binary lands at the path the template expects
  (`target/x86_64-pc-windows-msvc/release/planebotcli.exe`).
- The `Release` workflow re-ran failed at the `cargo wix` step with
  `CNDL0104: Not a valid source file; ... An XML comment cannot
  contain '--', and '-' cannot be the last character` (line 17 of
  `wix/main.wxs`). The comment block mentioned the cargo build CLI
  flags literally (`--release --target`), and WiX 3's strict-XML
  parser terminates any XML comment at the first `--`. Rewrote the
  comment without the literal sequence.
- The `Release` workflow re-ran failed again with
  `error CNDL0150 : Undefined preprocessor variable '$(var.CargoPkgVersion)'`
  (line 29 of `wix/main.wxs`). cargo-wix 0.3.9 defines only
  `$(var.Version)` (sourced from `src/templates/main.wxs.mustache` line
  68, populated via `-dVersion=…` in `src/create.rs` line 539). The
  previous attempts `$(env.CARGO_PKG_VERSION)` (shell-style, invalid
  WiX) and `$(var.CargoPkgVersion)` (cargo-wix 0.3.9 doesn't define
  it) both fail with the same CNDL0150. Switched to `$(var.Version)`.
- The `Release` workflow re-ran failed a third time with
  `main.wxs(27) : error CNDL0199 : The Wix element has an incorrect
  namespace of 'http://schemas.microsoft.com/wix/2006/main'. … Please
  make the Wix element look like the following:
  <Wix xmlns="http://schemas.microsoft.com/wix/2006/wi">`. The
  `http://schemas.microsoft.com/wix/2006/main` namespace is not what
  WiX 3 (and therefore cargo-wix 0.3.9) uses; the correct one is
  `http://schemas.microsoft.com/wix/2006/wi`. Switched the `xmlns=`
  attribute on `<Wix>` to match.
- SKILL.md Comments section gained an "Images in comments"
  paragraph documenting that `comment create` has no `-i/--image`
  flag but accepts the same Markdown as `wi create/update`: upload
  via `attachment attach`, then embed the returned uuid as
  `<img src='<uuid>'/>` in the `--body-md` string. The web side
  parses the `src` value as a literal asset uuid only. Closes the
  "comment image workflow not documented in the skill" gap from
  PLANECLI-10 feedback (comment by Jicai Liu, 2026-09-22).

## [1.4.0] - 2026-09-21

### Changed
- **BREAKING** `pbot doc create/update --content` now accepts Markdown (headings, lists, code, links, images) and converts it to HTML — the same path `--content-md` takes. The two flags are clap-level aliases (clap rejects passing both at parse time). A `# heading` line that previously rendered as the literal text `# heading` now becomes `<h1>heading</h1>`. Pass `--content-raw` to keep the v1.3.x plain-text behaviour (paragraphs from blank lines, single newlines become `<br/>`, markdown syntax literal).

### Added
- `pbot doc create/update --content-raw <text>` — the v1.3.x plain-text behaviour, kept under a new flag name so the old use cases keep working.
- `AGENTS.md` task-workflow rule 8: `skill/SKILL.md` frontmatter `version` must equal `workspace.package.version` for every change that ships CLI surface; the doc is the contract an AI agent reads before answering anything else, so a stale version tells them the doc matches a CLI they don't actually have. Pure-process / pure-build commits still bump `version` so a release that only touches `AGENTS.md` / workflows doesn't trip the lint.
- `.github/workflows/version-lint.yml`: a pure-grep CI lint that fails any PR where `skill/SKILL.md` frontmatter `version` drifts from `Cargo.toml`'s `workspace.package.version`. Runs on every push to `main` and every pull request, parallel to the existing `CI` workflow. Catches drift at PR time instead of waiting for a release PR.

### Fixed
- `skill/SKILL.md` frontmatter `version` was `2.0` (a stale skill-internal counter); corrected to `1.3.1` to match the current CLI version, and added a visible top-of-doc callout so any AI loading a future stale copy is told to re-sync instead of trusting the body.

### Removed
- (nothing)

## [1.3.1] - 2026-09-18

### Changed
- The release workflow now produces a drag-and-drop `.dmg` for each macOS target
  (Apple Silicon and Intel), built by a `macos-latest` job using
  `create-dmg` over the per-arch tarballs that `cargo-dist` already ships.
  The `.dmg` includes `planebotcli`, `pbot`, and a short `INSTALL.txt` so
  non-technical users can drag the binaries onto `/usr/local/bin` from Finder.
- The release workflow now produces a `planebotcli-x86_64-pc-windows-msvc.msi`
  double-click installer (cargo-dist's native `msi` builder). Windows users
  who prefer the GUI path no longer have to extract a `.zip`.
- `README.md` documents three install paths per platform (shell one-liner,
  drag-and-drop `.dmg` / `.msi`, and `cargo install` from a checkout), so users
  can pick whichever fits their workflow.

## [1.3.0] - 2026-09-18

### Added
- A local `reng` self-review step in `AGENTS.md`: every branch is reviewed
  locally before it is pushed and opened as a PR, so findings are caught earlier.
  The PR gate still runs afterwards.
- `doc create` / `doc update --content-md` now handle images with no extra flags:
  inline links `[text](url)` and images `![alt](src)` convert properly; a local
  image path (relative or `file://`) is uploaded as a page asset before the
  markdown renderer sees it and its source is replaced with the asset id, while
  remote URLs and existing asset ids stay as-is and fenced code is untouched.
  `--dry-run` prints the target and every image verdict and writes nothing,
  exiting non-zero when an image is missing or its MIME is not allowed for
  pages. Markdown links and images whose source has a `javascript:`, `data:`,
  `vbscript:`, or `file:` URL — i.e. ones the preflight did not already replace —
  are left as literal text, never written into an `href`/`src` attribute.

### Fixed
- `comment create --body` no longer sends raw `<...>` tags from a plain-text
  comment through to Plane's HTML renderer: `<img>` and `<image-component>`
  strings (and any other stray angle-bracket markup) are now HTML-escaped so
  they display as literal text. The conversion is aligned with the `--body-md`
  path — code spans first, then bare URLs, then a full-text escape — so a
  literal `&` inside a URL query string is preserved. Also closes an HTML
  injection vector for a malicious URL pasted into a plain-text body: the URL
  regex excludes both quote characters, so a URL cannot break out of the
  generated `href` attribute.
- `wi show <short-id>` and any other work-item lookup now resolve by exact
  `UUID` or `PROJ-N` before any substring-of-name check. The first pass
  sweeps the full candidate list, so a task whose title happens to contain
  the literal `PROJ-N` substring (e.g. `PLANE-3` matching a sentence that
  mentions `PLANE-33`) can no longer hijack the lookup of `PLANE-3` when the
  API returns that task earlier in the list. Substring and fuzzy matching keep
  working when there is no exact hit.

### Changed
- The repository keeps a single long-lived branch, `main`: every task branch is cut from
  and merged into it, a release is bumped and its `[Unreleased]` section renamed in the
  release PR itself, and tags are placed on `main`. The former `integration-main`
  integration line is retired.

## [1.2.0] - 2026-09-14

### Added
- A `CHANGELOG.md` entry is required for every change, written in the same pull
  request that makes it, under this section (`AGENTS.md` rule 7).
- A GitHub Actions CI workflow, `.github/workflows/ci.yml`: every push to
  `main` / `integration-main` and every pull request runs `cargo fmt --all -- --check`,
  `cargo clippy --locked --workspace --all-targets -- -D warnings`, and
  `cargo test --locked --workspace` on Linux, macOS, and Windows — mirroring the
  local `make check` gate. Previously only the cargo-dist release workflow existed.
  Three existing files were reformatted with rustfmt as part of this change (the
  new fmt gate caught them).
- Credentials can now come from a config file discovered from several locations,
  highest priority first: `~/.config/pbot/config.toml` (TOML with top-level
  `base_url` / `api_key` / `workspace`, optionally under `[auth]`), `~/.pbot`,
  `~/.planecli`, then the legacy `~/.plane_api`. `pbot configure` writes the active
  config file (highest-priority existing candidate, `~/.plane_api` by default);
  precedence stays CLI flags > env vars > config file.

### Fixed
- **Behaviour change**: project resolution now prefers exact identifiers and names
  (case-insensitive) over fuzzy name matches, and prints a warning when only a fuzzy
  match exists — `project show RENG` can no longer land on a different project
  (Sirena) by name. `wi create --parent` therefore resolves parents within the right
  project again.
- `--labels` now matches strictly: a missing label errors with the available list
  instead of silently applying the closest one (e.g. `release-0.10.9` → `release-0.10.6`).
  States keep exact-name priority with a fuzzy fall-back.
- `wi search` failed against Plane 1.4+ because the search endpoint returns an
  `{"issues": [...]}` envelope (older builds return a bare array) and the CLI sent
  the wrong `query` parameter, which the server ignores (empty results). It now
  sends `search` / `limit`, unpacks both response shapes, supports `-p <project>`
  (mapped to `project_id` + `workspace_search=false`), and derives the displayed
  identifier from the search payload's `project__identifier` field.
- API error messages for undecodable responses now include the HTTP status code
  and the start of the response body, so a shape mismatch is distinguishable from
  an HTTP failure.
- Repository workflow conventions (`AGENTS.md`): every change is tracked by a
  work item, branched off `integration-main`, and landed through a pull request;
  `main`, `master`, and `dev` take no direct merges.
- A `reng` review gate on pull requests: the report is published to the PR, read
  no earlier than five minutes later, and every finding is answered before the
  merge is requested.
- A Chinese edition of the CLI command reference,
  `docs/cli-command-reference.zh.md`, alongside the English one.

### Changed
- The maintainer merges pull requests; the agent that opened one no longer does.
- The CLI is documented as an independent distribution: changes land on the
  `integration-main` line and are never opened as issues or pull requests against
  Plane upstream. The skill's comment format and gotchas describe this build rather than
  an upstream one.
- Release notes accumulate under `## [Unreleased]` as changes land. A release
  renames that section to `## [<version>] - YYYY-MM-DD`, instead of reconstructing
  the entries at release time.
- The CLI command reference was rewritten for the Rust line, covering all 58
  subcommands; the previous one still described the 0.7.0 Python line.
- The project's identity is stated explicitly: `planebotcli` / `pbot` is an
  independent client for Plane, not a fork or downstream of `plane-cli`, and it
  has no upstream to sync with.
- The build and development toolchain is cargo: the Makefile targets (`install`,
  `build`, `run`, `test`, `test-v`, `lint`, `format`, `check`, `e2e`, `clean`)
  wrap cargo, clippy, rustfmt, and `scripts/e2e.sh` instead of uv, pytest, and
  ruff.
- The project is named `planebotcli` (the repository, the workspace crates, and the
  installed binary); `pbot` is the short call name used in every example.
- The stale descriptions left by the rename were cleaned up: the skill now matches the
  Rust line's behaviour (`-d` wraps plain text, `--desc-md` is the markdown path,
  `relations` is a first-class command group, and the long-lived branches are
  `integration-main` / `main`); the `pbotcli` spelling was dropped in favour of
  `planebotcli` everywhere; the GitHub repository description no longer calls the
  project a fork of `plane-cli`.

### Removed
- The Python implementation was removed — its source package, tests,
  `pyproject.toml`, and `uv.lock` — leaving the Rust line under `crates/` as the
  only implementation; the `skills/` directory, the older skill that drove the
  Python binary, went with it.
- The documents that describe the Python implementation
  (`docs/architecture.md`, `docs/caching.md`, the ADRs, `docs/rust-rewrite.md`)
  are kept, each marked as historical reference rather than current behaviour.

## [1.1.0] - 2026-09-10

### Added
- `wi update --parent <ID|UUID>` and `--clear-parent`: set or clear a work item's
  parent (self/cross-project/not-found are rejected locally, listing candidates).
- `relations` command group (alias `relation`): `ls` (the 8 Plane relation buckets)
  and `add --type T --to TARGET...` (blocking/blocked_by/duplicate/relates_to/
  start_before/start_after/finish_before/finish_after). Removal needs a backend
  DELETE endpoint that does not exist yet (tracked separately).
- `wi show` now returns `sub_issues` (child work items) and surfaces `parent`.

## [1.0.5] - 2026-09-10

### Fixed
- `--content-md` keeps the fence info string's language: ```` ```bash ````
  now emits `<code class="language-bash">`, so the web code block records
  the language (and highlights) instead of showing as plain text.

## [1.0.4] - 2026-09-10

### Fixed
- `doc create` / `doc update` with `--content-md` now parse markdown blocks
  line by line instead of by blank-line chunks: a heading followed by a list
  (or a paragraph followed by a table) no longer renders as one literal
  paragraph. Added GitHub-style tables, blockquotes (`>`) and horizontal
  rules; tilde fences (`~~~`) are accepted alongside backticks. Inline
  conversion (code spans, bold/italic, links) is unchanged.

## [1.0.3] - 2026-09-09

### Added
- `doc archive` — archive (trash) a page without deleting it: sets
  `archived_at` = today, verifies the archive landed, and leaves the page
  recoverable in the web UI trash. (`doc delete` still archives then deletes.)

## [1.0.2] - 2026-09-09

### Added
- `doc create` / `doc update` accept **native markdown** (`--content-md`, converted
  to HTML with headings/lists/code/links) and **raw HTML** (`--content-html`, stored
  verbatim for rich layout). `--content` (plain text) stays; the three are mutually
  exclusive. Previously pasting markdown or HTML through `--content` rendered
  literally, producing poorly formatted pages.

## [1.0.1] - 2026-09-09

### Fixed
- `module update` now accepts `-d/--description`, `--start-date`, and `--end-date`
  (parity gap vs the Python line; module end date maps to the API `target_date`).

## [1.0.0] - 2026-09-09

### Changed
- **Rewritten in Rust.** The CLI is now `planebotcli` / `pbot` — a single static
  binary built from a Cargo workspace (`crates/planebotcli-{types,client,
  resolve,cache,html,format,core,cli}`), replacing the Python implementation
  line. No Python runtime required.
- Full feature parity with the Python v0.7.0 line: every command group
  (whoami, configure, project, wi, comment, attachment, doc, intake, label,
  state, module, cycle, user, cache), fuzzy resolution (name / `ABC-123` /
  UUID), the dual output contract (`--json` → stdout, table → stderr),
  per-resource disk cache with write invalidation, write verification by
  read-back (dates, intake triage, attachment/uploads), markdown input
  (`--body-md` / `--desc-md`), inline image embedding (`-i`), `web_url`
  output, and HTTP-status + field-level error messages.
- Install: `cargo install --path crates/planebotcli-cli` (installs both
  `planebotcli` and the `pbot` alias). The Python line is frozen to bug fixes
  (kept on tags/branches).

## [0.7.0] - 2026-09-09

### Added
- `--desc-md` on `wi create` / `wi update` and `--body-md` on `comment create` / `comment update`: native markdown-subset input (headings, unordered/ordered lists, inline and fenced code, bold/italic, auto-linked bare URLs) converted to HTML client-side, so no external markdown-to-HTML step is needed. `--desc-md` is mutually exclusive with `-d`/`--description`; `--body-md` with `--body`
- A `web_url` field on work item output — a browsable link (`{base}/{workspace}/projects/{project-uuid}/issues/{issue-uuid}/`) built from config, present in the JSON of `wi ls` / `wi show` / `wi create` / `wi update` / `wi search`, and shown as a `Web URL` row in `wi show`

### Fixed
- `wi create` / `wi update` now validate `--state` and `--labels` against the project before writing: an unknown name fails with exit code 5 and a message that lists the available states/labels (e.g. `State 'In Review' not found in project SIRENA. Available: Backlog, Todo, ...`), instead of a bare not-found error. Fuzzy matching is preserved and resolution stays authoritative
- API error messages now include the HTTP status code and field-level errors parsed from the response body (e.g. `description: This field is required.`); they degrade gracefully when the SDK exposes no detail, and never include credentials

## [0.6.0] - 2026-09-09

### Added
- `planecli intake` command group: `ls`, `create`, `accept`, `decline`, `delete`, `enabled` for project intake queues. Mutations take the work item UUID shown in the `Issue ID` column of `intake ls`. `accept`/`decline` require the project Admin role (the API silently ignores the change for lower roles, so the CLI verifies it and fails loudly). `delete` also permanently deletes the underlying work item for any status other than `accepted`
- `planecli attachment` command group: `ls` lists a work item's attachments and `attach` uploads a local file (asks for confirmation when the name already exists, `--force` to skip)
- `-i` / `--image` repeatable flag on `wi create` and `wi update`: uploads an image and embeds it in the description as an `img` tag
- `--start-date` / `--target-date` flags on `wi create` and `wi update` (YYYY-MM-DD, validated client-side with exit code 5 on a bad format; writes are verified by read-back per ADR-0007)

### Fixed
- Comment bodies now render as separate paragraphs and line breaks instead of being joined into a single paragraph; inline backtick spans render as code and bare http(s) URLs become clickable anchors (code content is HTML-escaped and URLs inside code spans are not linkified)
- `wi show` `*_name` fields (`state_detail_name`, `label_names`, `label_detail_names`, `assignee_names`) now resolve to human-readable names like `wi ls` does, instead of raw UUIDs
- `doc create` no longer crashes when `--content` is omitted (an empty paragraph is sent instead of a missing required field); `--content` converts plain text to HTML the same way comments do (paragraphs, line breaks, code, links); `doc delete` archives the page before deleting it, so it succeeds instead of returning a 400

## [0.5.1] - 2026-07-03

### Added
- `wi show` now includes the work item's comments in the same call. In `--json` output, the `comments` field is a list (`[]` when there are no comments) or `null` if the comment fetch fails — the work item is still returned and the command exits with code 0. In human-readable output, a `Comments` section follows the details, showing `(none)` or `(failed to load)` as appropriate
- `--no-comments` flag on `wi show` to skip fetching comments (the `comments` key is then omitted from JSON)
- `--limit` / `-l` option on `comment ls` (default 50): selects the N most recent comments, still rendered oldest to newest
- Per-work-item comment cache (1-minute TTL), invalidated on `comment create`/`update`/`delete`
- `--parent` filter on `wi list` to list the child work items of a parent, referenced by identifier (`ABC-123`), UUID, or name
- `--status` option on the `module create` and `module update` commands (values: `backlog`, `planned`, `in-progress`, `paused`, `completed`, `cancelled`), with English and Portuguese aliases (e.g. `em andamento`, `concluído`, `cancelado`) and validation with a clear error message

### Fixed
- `comment ls --limit 0` (or a negative value) now returns no results
- `wi show` no longer claims comments "failed to load" when the fetch was never attempted
- A failure to fetch members now degrades gracefully instead of aborting the comment fetch

### Documentation
- Added the Architecture Decision Records in `docs/adr/`, `docs/architecture.md`, and `AGENTS.md` at the root
- Renamed `docs/03-caching.md` to `docs/caching.md` and fixed the reference to the cached resource

## [0.3.0] - 2026-02-12

### Added
- Structured logging with loguru and fewer API calls via smart caching
- Automatic retry with tenacity for rate limits and transient API errors
- estimate_point UUID resolution in the `--estimate` parameter of the `wi` command

### Changed
- Removed the redundant Project column from the `wi list` output
- Updated the README terminology from "API Key" to "Personal Access Token"

### Fixed
- Fixed the SDK validation bypass for assignees/labels in work item resolution
- Removed the silent suppression of exceptions in the `wi` command's assignees filter

## [0.2.0] - 2026-02-11

### Added
- Disk caching of API responses with cashews for faster repeated requests
- Async execution of all API calls using `asyncio.to_thread` and parallel execution
- Comma-separated state and label filtering in the `wi list` command
- Color styling for labels, states, priorities, and work item tables
- Commands for managing cycles, labels, and states
- Comment update and delete operations
- `--project` became optional in `wi list`, listing all projects when omitted
- Separator lines and vertical spacing in table output
- Comprehensive test coverage for config, fuzzy matching, and resource resolution

### Changed
- Migrated all API calls from synchronous to asynchronous with a thread pool and semaphore-based rate limiting
- Rewrote the README with improved structure, a quick-start guide, and a complete command reference
- Added a Makefile with development tasks
- Updated the installation instructions with the `uv tool install` option

## [0.1.0] - 2026-02-11

### Added
- Initial release of PlaneCLI
- CLI application with commands for projects, work items, comments, documents, users, and modules
- Configuration via arguments, environment variables (`PLANE_BASE_URL`, `PLANE_API_KEY`, `PLANE_WORKSPACE`), and the `~/.plane_api` file
- Fuzzy resource search using rapidfuzz
- Rich table output (stderr) and JSON output (stdout) with the `--json` flag
- Smart resource resolution: UUID, identifier (e.g. `ABC-123`), or name search
- Cursor-based pagination in resource listing
