# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/)
and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

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
