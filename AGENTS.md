# PlaneCLI

A Python CLI for [Plane.so](https://plane.so) (SaaS or self-hosted) that manages projects, work items, cycles, modules, documents, labels, states, intake queues, and comments. Its defining feature is **fuzzy resource resolution**: any resource can be referenced by name, identifier (`ABC-123`), or UUID. Built with cyclopts, Rich, rapidfuzz, cashews, and the official `plane-sdk`.

## Language

All docs, comments, and commit messages in English.

## Commands

Everything runs through `uv`; common tasks are wrapped in the Makefile.

```bash
make install                       # uv sync (dev environment)
make test                          # uv run pytest tests/
make lint                          # ruff check src/
make lint-fix                      # ruff check --fix src/
make format                        # ruff format src/ tests/
make check                         # lint + test — run before committing
make run ARGS="wi ls -p Frontend"  # run the CLI
```

Single test: `uv run pytest tests/test_resolve.py::test_name -v`. Line length 100, Python >= 3.11, ruff selects `E, F, I, W`.

## Structure

```
src/planecli/
  app.py           # Root cyclopts App; main() entry point; sub-app registration
  commands/        # One module per resource (list/show/create/update/delete). New features go here.
  utils/resolve.py # Resolution layer: resolve_<x>/resolve_<x>_async (UUID → identifier → fuzzy name)
  utils/fuzzy.py   # rapidfuzz token_sort_ratio, threshold 60
  api/             # client.py (PlaneClient singleton); async_sdk.py (async wrapper)
  cache.py         # cashews disk cache; one cached_list_<x> per resource
  formatters/      # output() / output_single() — table to stderr, JSON to stdout
  exceptions.py    # PlaneCLIError subclasses with message/hint/exit_code
```

Request flow: **command → resolve → async SDK wrapper → (cache | Plane SDK) → formatter**. See [docs/architecture.md](docs/architecture.md).

## Conventions

- **Never call the sync Plane SDK directly from a command.** Wrap single calls in `run_sdk(fn, *args)` and paginated lists in `paginate_all_async(list_fn, ...)`. For concurrent batches use `create_client()` (a fresh client per thread — don't share the singleton's `requests.Session`). See [ADR-0001](docs/adr/0001-async-wrapper-over-sync-sdk.md).
- **Reads go through the cache layer.** Resolvers and list commands call `cached_list_<x>(...)`, which returns **plain dicts** (not Pydantic models). After any create/update/delete, call `await invalidate_resource("<resource>", workspace, project_id)`. See [ADR-0004](docs/adr/0004-disk-cache-ttls-and-keys.md).
- **Error handling.** Wrap SDK calls in `try/except PlaneError` and `raise handle_api_error(e)`. Raise `ValidationError`/`ResourceNotFoundError` for user-facing problems. Exit codes: Auth=2, NotFound=3, API=4, Validation=5.
- **`--json` flag.** Every read/mutate command takes `json: bool = False` and passes `as_json=json` to the formatter. Table output stays on **stderr** so `--json 2>/dev/null` yields clean JSON. See [ADR-0005](docs/adr/0005-dual-output-contract.md).
- **Lazy imports.** Import SDK models and cache helpers *inside* the command function to keep CLI startup fast (sub-app registration in `app.py` uses `# noqa: E402` deliberately).
- **cyclopts idioms.** Sub-apps declare aliases via `name=["module", "modules"]`; subcommands via `@app.command(name="list", alias="ls")`; short flags via `Annotated[str, Parameter(alias="-p")]`. Numpydoc parameter docstrings become `--help` text.

## Gotchas

- **`--verbose`/`-v` and `--no-cache` are stripped from `sys.argv` in `main()` before cyclopts parses** — cyclopts does not own them. Add new global flags the same way.
- **SDK model mismatches require escape hatches** (see [ADR-0003](docs/adr/0003-sdk-escape-hatches.md)):
  - Work items: `WorkItemDetail` validation fails because the API returns `assignees`/`labels` as UUID strings. Resolvers use raw `client.work_items._get(path)` to get a dict directly. Each site has a `NOTE:` comment.
  - Documents (Pages): SDK support is incomplete; `commands/documents.py` calls the HTTP API directly with `requests` + `X-Api-Key` via `run_sdk(requests.get, ...)`.
  - Estimate points: no API endpoint; `cached_list_estimate_points` derives them from work items with `expand=estimate_point`.
- **A `200` is not always a write.** Some Plane endpoints accept a mutation and ignore it — the intake `PATCH` returns `200` with the record unchanged when the caller is not a project Admin. Where that is known, read the raw response and compare the field you meant to change *before* enriching it for display, then raise `APIError`. See [ADR-0007](docs/adr/0007-verify-writes-the-api-can-silently-ignore.md).
- **Intake mutations take the work item UUID**, not the intake wrapper `id` — the `Issue ID` column of `intake ls`. `intake delete` also deletes the underlying work item for any status other than `accepted`; destructive commands are documented, never prompted (see [ADR-0008](docs/adr/0008-destructive-deletes-are-documented-not-prompted.md)).
- **Guard optional flags with `is not None`, not truthiness.** `--priority ""` must reach the validator and be rejected; `if priority:` would silently fall back to the default.
- **Docstrings are `--help` text, so literal angle brackets vanish.** cyclopts/Rich renders them as markup — write "a paragraph tag", not `<p>`.
- **Reference versioned docs, not tracker issues.** Do not cite Plane/Linear issue IDs or external tracker URLs in code comments — they are unreachable after delivery. Point to an ADR or guide instead.
- **Tests never hit a real Plane instance.** `conftest.py` autouses a `mem://` cache backend and provides a `mock_plane_client` fixture. Mock the SDK/resolvers; prefer testing pure logic (normalizers, fuzzy matching, resolution) directly.

## Task workflow

Every change — code, docs, or config — is tracked and lands through review. There is no "too
small to track" or "too small for a PR" exemption.

1. **Find or create the Plane work item first.** One item per task, in the project that owns the
   change (PLANECLI for this CLI). It carries the scope, the acceptance criterion, and the branch
   name. Work started without an item leaves no trace, and an untraced change is not deliverable.
2. **Comment on the item as the work moves** — at start (branch + plan), at review (commit range,
   PR URL, the `reng` findings and how each was answered), and at merge. Set the state to match
   reality: `In Progress` once work begins, `Done` only after the merge is on `main`. A finished
   change with no comment on its item is not done.
3. **Branch per task**, off `integration-main`, named `<type>/<task>-<slug>` (e.g.
   `feat/planecli-42-relations-remove`). Never commit a task's work straight onto an integration or
   release branch.
4. **Open a pull request — and stop there.** Push the branch to the `planebotcli` remote and open a
   PR against `integration-main` (`gh pr create --repo Liewzheng/planebotcli --base
   integration-main`). Never commit a task's work straight onto an integration or release branch,
   and never merge a PR yourself: merging is the human's call, made after the review gate below.
5. **Never merge into a protected branch by hand.** `main`, `master`, and `dev` — on every remote,
   `planebotcli`'s `main` included — accept changes only through a merged PR. No
   `git push <remote> <branch>:main`, no `--force`, no local fast-forward that skips review.
6. **Resync after every merge** — `git fetch planebotcli` and fast-forward `integration-main` — so
   the next task branch starts from the merged state.

### Review gate

No PR is merged until `reng` (the local Rust Code Review Engine) has reviewed it and every finding
has been answered on the PR.

1. Trigger the review and publish it to the PR as comments:

   ```bash
   ~/.local/bin/reng review \
     --mr-url "https://github.com/Liewzheng/planebotcli/pull/<n>" \
     --github-token "$(gh auth token)" --publish
   ```

   `reng` is not on `PATH` — call it by absolute path. `--publish` writes the findings as PR
   comments and leaves the full report under `~/.config/review-engine/reports/`. The GitHub token
   comes from `gh auth token` (scope `repo`).
2. **Wait 5 minutes after triggering before reading anything.** Answering a half-published review
   wastes a round trip.
3. Read every comment: `gh pr view <n> --repo Liewzheng/planebotcli --comments`.
4. **Triage every finding and reply to it on the PR — none may be left unanswered.**
   - **real bug or good suggestion** → fix it on the same branch, push, and reply with what changed;
   - **false positive** → reply with the evidence that refutes it (`file:line`, project context,
     history). `reng` is an LLM reviewer that does not know this codebase and over-reports, so
     triage rather than comply blindly — the `reng-mr-review` skill lists its known failure modes.
5. **Then ask the human to merge**, reporting the PR URL, every finding, and how each was handled.
   The human merges; the agent does not. The Plane item goes to `Done` only after that merge lands on
   `main`.

## Release management

The release repo `github.com/Liewzheng/planebotcli` holds two long-lived branches: `integration-main`
(the integration line where completed tasks accumulate) and `main` (the released line, which only
advances through a PR from `integration-main`). Since **1.0.0 (2026-09-09) the CLI is the Rust rewrite** (`planebotcli` /
`pbot`, a Cargo workspace in `crates/`); the Python line is frozen to bug fixes. Version and
changelog are managed by the agent on `integration-main` only — never on upstream `main` or the
fork PR branches.

- Keep SemVer: bump the minor for new commands/flags, the patch for bug fixes. The single version
  lives in the workspace root `Cargo.toml` (`[workspace.package] version`).
- Append a Keep a Changelog section to `CHANGELOG.md` in upstream style, without internal tracker IDs.
- Cut a release by committing `release: <version>` on `integration-main`, pushing that branch, and
  opening a PR from `integration-main` into the planebotcli remote's `main`:
  `gh pr create --repo Liewzheng/planebotcli --base main --head integration-main`. That PR goes
  through the same review gate, and the **human merges it** — the released line never takes a direct
  push and the agent never merges (see Task workflow).
- After cutting a release, reinstall the local CLI from the merged `integration-main` checkout by
  default (no need to ask first): `cargo install --path crates/planebotcli-cli --locked`
  (installs both `planebotcli` and the `pbot` alias into `~/.cargo/bin`).
- **Publishing artifacts** (needs the human's go-ahead + external accounts): tag the release and
  push — `git tag v<version> && git push planebotcli v<version>` — so the committed cargo-dist
  GitHub Actions workflow builds per-platform archives + shell/powershell installers to the
  Releases page. npm publish needs an npm scope/token (cargo-dist npm installer config);
  crates.io `cargo publish` needs a crates.io token.
- Update the corresponding Plane task the same turn a release lands: progress comment + fitting
  state (per the pbot skill etiquette).

## Key docs

- [Architecture](docs/architecture.md) — layers and request flow
- [Caching](docs/caching.md) — TTLs, keys, invalidation
- [ADRs](docs/adr/) — the decisions behind the design
