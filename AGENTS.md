# planebotcli (pbot)

`pbot` is an independent command-line client for [Plane.so](https://plane.so) (SaaS or self-hosted):
projects, work items, cycles, modules, documents, labels, states, intake queues, and comments. Its
defining feature is **fuzzy resource resolution** — any resource can be referenced by name,
identifier (`ABC-123`), or UUID. It is a Rust workspace under `crates/`; the Python implementation in
`src/planecli` is a frozen legacy line kept for history.

## Identity

This project is **not a fork, a port, or a downstream of `plane-cli`**, and `plane-cli` is not its
upstream. `pbot` began from that codebase and has since evolved on its own; the two have no
relationship to maintain.

- The only home is `github.com/Liewzheng/planebotcli`. There is nowhere to sync from and nobody to
  send pull requests to.
- Do not reintroduce the vocabulary of a fork: no "upstream", no "syncing with upstream", no PRs
  against another repository, no mirroring branches back and forth.
- Historical references (`plane-cli`, the Python line, the ADRs written for it) stay as history and
  as provenance; they are not a live dependency or a source of incoming changes.
- New work is judged on its own merits — what `pbot` should be — not on parity with `plane-cli`.

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
   PR URL, and one line per review round summarising the `reng` findings and how each was answered),
   and at merge. Set the state to match reality: `In Progress` once work begins, `Done` only after
   the merge is on `main`. A finished change with no comment on its item is not done.
3. **Branch per task**, off `integration-main`, named `<type>/<task>-<slug>` (e.g.
   `feat/planecli-42-relations-remove`). Never commit a task's work straight onto an integration or
   release branch.
4. **Open a pull request — and stop there.** Push the branch to the `planebotcli` remote and open a
   PR against `integration-main` (`gh pr create --repo Liewzheng/planebotcli --base
   integration-main`). Keep the PR number it prints (`gh pr view --json number -q .number` retrieves
   it) — the review gate needs it. Merging is the human's call, made after that gate.
5. **Never merge into a protected branch by hand.** `main`, `master`, and `dev` — on every remote,
   `planebotcli`'s `main` included — accept changes only through a merged PR. No
   `git push <remote> <branch>:main`, no `--force`, no local fast-forward that skips review.
6. **Resync after every merge** — `git fetch planebotcli` and fast-forward `integration-main` — so
   the next task branch starts from the merged state.
7. **Record the change in `CHANGELOG.md` in the same PR.** Every change gets a Keep a Changelog
   entry under `## [Unreleased]` — code, docs, and repository process alike, no exemptions. A change
   that is reverted before release is edited out of its entry rather than answered by a second one.
   Write it in the file's existing voice, one or two sentences: a CLI change names the command and
   flag, a repository-process change says what the process now is, and neither restates what
   `AGENTS.md` already explains in full. Keep internal tracker IDs out (the Plane item is the trace,
   the changelog is for readers).

### Review gate

**Terms:** the published document is the *report*; its individual items are the *findings*.

No PR is merged until `reng` (the local Rust Code Review Engine) has reviewed it and every finding
has been answered on the PR. The gate runs through the `gh` CLI — if `gh` is missing or
unauthenticated, stop and say so rather than driving the API by hand.

1. **Trigger the review and publish it to the PR.** `<n>` is the PR number kept in step 4 above
   (`gh pr view --json number -q .number` prints it again). `reng` is usually not on `PATH`:

   ```bash
   command -v gh >/dev/null || { echo "gh is not installed — the review gate cannot run" >&2; exit 1; }
   gh auth status >/dev/null 2>&1 || { echo "gh is not authenticated — run: gh auth login" >&2; exit 1; }
   RENG=$(command -v reng || echo ~/.local/bin/reng)
   [ -x "$RENG" ] || { echo "reng not found at $RENG — install it or fix PATH, then retry" >&2; exit 1; }
   START=$(date +%s)
   GITHUB_TOKEN=$(gh auth token) timeout 900 "$RENG" review \
     --mr-url "https://github.com/Liewzheng/planebotcli/pull/<n>" --publish
   ```

   `timeout 900` turns a hung review into a failure the gate can report. Pass the token through the
   environment (`reng` reads `GITHUB_TOKEN`), **not** with `--github-token`: command-line arguments
   are visible in `ps` output and land in shell history. An environment variable is not a vault
   either — it is inherited by child processes and can surface in debug dumps — but it is strictly
   better than argv, and it is the only channel `reng` offers.
   `--publish` also leaves the full JSON report under `~/.config/review-engine/reports/`. On GitHub
   the report is posted as a **PR review body**, so read it with:

   ```bash
   gh api "repos/Liewzheng/planebotcli/pulls/<n>/reviews" \
     -q '[.[] | select(.body | startswith("# CodeReview Board"))] | last | .body'
   ```

   Filter on the report heading rather than taking `.[0]`: other reviews may sit on the PR, and
   `.[0]` would then hand back the wrong body. `reng` updates its report in place, so a PR normally
   carries exactly one, and `last` just guards against a newer one being present.
   `gh pr view --comments` fails on this repo with a GraphQL Projects-classic deprecation error, so
   do not reach for it.
2. **Wait 5 minutes from the moment the review command starts before reading anything.** `START`
   above holds that moment in epoch seconds; do not read until `date +%s` is at least `START + 300`.
   The pause is fixed by the maintainer, not derived from `reng`'s runtime: the command blocks until
   it has published, so the wait is a cooling-off window rather than a completion check, and it
   applies even when the report is already up.
3. **A failed or empty publish is not a pass.** `--publish` can exit non-zero with `inline notes`
   while the report *was* published, and re-running updates the existing report in place rather than
   posting a second copy — check the PR before re-running. But if `reng` cannot be resolved, fails to
   start, exits on the `timeout` (status 124, meaning it hung), dies before publishing, or the PR
   carries no report at all, stop: report the failure on the PR and to the human, and do not ask for
   a merge. An expert rendered as *"输出解析失败 / failed to parse its output"* counts as
   **unreviewed, not clean** — say so when reporting the findings. A finding with an empty title is a
   parse artifact rather than a reviewable item: name it as such in the triage comment and move on.
4. **Triage every finding and reply to it on the PR — none may be left unanswered.** The report is a
   review body, so there is normally no inline thread to reply in; each round gets its own top-level
   `gh pr comment <n> --repo Liewzheng/planebotcli --body-file <file>` — separate comments, not one
   growing thread — listing every finding in that round and its disposition.
   - **real bug or good suggestion** → fix it on the same branch, push, and reply with what changed;
   - **needs a human decision, or a change too large for this branch** → reply saying so and carry it
     into the merge request; never drop it silently;
   - **false positive** → reply with the evidence that refutes it (`file:line`, project context,
     history). `reng` is an LLM reviewer that does not know this codebase and over-reports, so triage
     rather than comply blindly — check the `reng-mr-review` skill
     (`~/.kimi-code/skills/reng-mr-review/SKILL.md`) for its known failure modes before calling a
     finding a false positive.
5. **Then ask the human to merge**, reporting the PR URL, every finding, and how each was handled.
   The human merges; the agent does not. The Plane item goes to `Done` only after that merge lands on
   `main`.

## Release management

The repository `github.com/Liewzheng/planebotcli` holds two long-lived branches: `integration-main`
(the integration line where completed tasks accumulate) and `main` (the released line, which only
advances through a PR from `integration-main`). Since **1.0.0 (2026-09-09) the CLI is the Rust line**
(`planebotcli` / `pbot`, a Cargo workspace in `crates/`); the Python line in `src/planecli` is frozen
and kept for history. Version and changelog are managed by the agent on `integration-main` only —
this repository is the project's only home (see Identity).

- Keep SemVer: bump the minor for new commands/flags, the patch for bug fixes. The single version
  lives in the workspace root `Cargo.toml` (`[workspace.package] version`).
- Entries accumulate under `## [Unreleased]` as each change lands (see Task workflow rule 7) —
  never batched in at release time, when nobody remembers what shipped. Cut a release by renaming
  that section to `## [<version>] - YYYY-MM-DD`, in the file's existing Keep a Changelog style and
  without internal tracker IDs.
- Cut a release by committing `release: <version>` on `integration-main`, pushing that branch, and
  opening a PR from `integration-main` into the planebotcli remote's `main`:
  `gh pr create --repo Liewzheng/planebotcli --base main --head integration-main`. A release PR goes
  through the full review gate — the same `reng` run, the same 5-minute wait, the same per-finding
  replies, no shortcut for docs or version bumps — and the **human merges it**. The released line
  never takes a direct push and the agent never merges (see Task workflow).
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
