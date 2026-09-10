# planebotcli (pbot)

`pbot` is an independent command-line client for [Plane.so](https://plane.so) (SaaS or self-hosted):
projects, work items, cycles, modules, documents, labels, states, intake queues, comments, and
attachments. Its defining feature is **fuzzy resource resolution** — any resource can be referenced
by name, identifier (`ABC-123`), or UUID. It is a Rust workspace under `crates/`; the Python
implementation it replaced has been removed, and the documents that describe it are kept as history.

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

Everything runs through `cargo`; common tasks are wrapped in the Makefile.

```bash
make install                       # cargo install the CLI locally (pbot + planebotcli)
make build                         # cargo build --workspace
make test                          # cargo test --workspace
make test-v                        # the same, showing test output (--nocapture)
make lint                          # cargo clippy --workspace --all-targets -- -D warnings
make format                        # cargo fmt --all
make check                         # lint + test — run before committing
make run ARGS="wi ls -p Frontend"  # run the CLI (cargo run --bin pbot)
make e2e                           # scripts/e2e.sh — live smoke test against a real instance
```

Single test: `cargo test --workspace <name>` (e.g. `cargo test -p planebotcli-resolve resolve_project`),
or `cargo run --bin pbot -- wi ls` for the CLI without installing it. Edition 2024, rustfmt
defaults, clippy clean at `-D warnings`.

## Structure

```
crates/
  planebotcli-core       # Config (precedence: flags > env > ~/.plane_api), PlaneError + exit codes
  planebotcli-types      # serde DTOs for every API resource, plus the *Write request bodies
  planebotcli-cache      # TTL disk cache: one JSON file per key, invalidate by key prefix
  planebotcli-client     # reqwest async client for the Plane v1 API: X-Api-Key, cursor pagination,
                         # retry, HTTP→PlaneError mapping, attachment upload, cache wiring
  planebotcli-resolve    # fuzzy matching (rapidfuzz, threshold 60), resolve_<resource> helpers
  planebotcli-html       # plain text / markdown subset → HTML, linkify, strip_html_tags
  planebotcli-format     # output_json (stdout) / output_table (stderr)
  planebotcli-cli        # clap command tree and handlers (lib.rs), JSON/table views (render.rs)
```

`planebotcli-cli` builds both binaries — `planebotcli` (`src/main.rs`) and `pbot`
(`src/bin/pbot.rs`); each is a thin `Cli::parse()` → `planebotcli_cli::run` wrapper. Request flow:
**clap command → resolve → client (cache | HTTP) → render view → format**. Top level: `crates/`
(the workspace), `docs/` (the CLI command reference, the Plane API reference, and the historical
Python-era documents), `skill/` (the agent skill), `scripts/e2e.sh`.

## Conventions

- **Commands are clap subcommands.** The tree is the `Cli`/`Command` enums in
  `crates/planebotcli-cli/src/lib.rs`; each handler is a `cmd_*` function taking `&PlaneClient` and
  `json: bool`, dispatched from the `match` in `run`. Aliases go on the variant
  (`#[command(alias = "ls")]`, `visible_alias = "issues"`), short flags via `#[arg(long, short = 'p')]`,
  and doc comments on variants and fields are the `--help` text.
- **All HTTP goes through `PlaneClient`** (`crates/planebotcli-client`). A handler never calls
  `reqwest` itself: the client owns the `X-Api-Key` header, the `/api/v1` prefix, cursor pagination
  (`paginate`), retry on 429/5xx (5 attempts, linear backoff) and the HTTP→`PlaneError` mapping
  (`map_api_error`).
- **Reads go through the cache layer.** The client's list methods (`list_projects`, `list_states`, …)
  serve from the TTL disk cache and refresh it; every create/update/delete calls
  `self.invalidate("<resource>:{workspace}[:{project_id}]")` on the way out. The TTLs are the
  `TTL_*` constants at the top of `planebotcli-client/src/lib.rs`; the global `--no-cache` flag is
  threaded into `PlaneClient::with_cache` and disables both halves. See
  [ADR-0004](docs/adr/0004-disk-cache-ttls-and-keys.md) (historical).
- **Errors are typed, and the type carries the exit code.** Return `PlaneError` from
  `planebotcli-core`: `Auth`=2, `NotFound`=3, `Api`=4, `Validation`=5, `Other`=1. `main.rs` prints
  `Error: …`, then `Hint: …` when the variant has one, and exits with `err.exit_code()`. Build
  user-facing failures with `PlaneError::validation(...)` / `validation_with_hint(...)`, and make the
  hint name the next command to run.
- **Dual output contract.** `--json` is a global flag; `output_json` writes to stdout, `output_table`
  to stderr, so `--json 2>/dev/null` yields clean JSON. The JSON shape is the `serde_json::Value`
  views built in `render.rs`, so its keys are part of the contract. See
  [ADR-0005](docs/adr/0005-dual-output-contract.md) (historical).
- **Views live in `render.rs`.** `work_item_view` and friends resolve UUIDs to names through the
  `Lookups` maps (states, labels, members) and add the composed `sequence_id`,
  `description_stripped`, and `web_url`. Enrichment that fails degrades to `null`; it never aborts
  the command. See [ADR-0006](docs/adr/0006-secondary-enrichment-degrades-to-null.md) (historical).
- **Request bodies are `*Write` structs** whose `None` fields are stripped by the client, so an
  unset `Option` means "leave this alone". A PATCH that must send an explicit `null` (clearing a
  parent) goes through the client's raw-value path instead. Validate client-side before writing, and
  read the field back where the API can silently ignore it. See
  [ADR-0007](docs/adr/0007-verify-writes-the-api-can-silently-ignore.md) (historical).

## Gotchas

- **A `200` is not always a write.** Plane accepts some mutations and ignores them: the intake
  `PATCH` returns `200` with the record unchanged for a non-Admin, and a misspelled field is
  swallowed the same way. Where that is known the CLI verifies by read-back — the intake status, an
  updated date (`lib.rs`, the `wi update` date check), an attachment's `is_uploaded`, a relation
  POST — and raises `PlaneError::Api` when the server did not record the change. `relations remove`
  exists only to report that the API has no DELETE endpoint for a relation. See
  [ADR-0007](docs/adr/0007-verify-writes-the-api-can-silently-ignore.md).
- **Intake mutations take the work item UUID**, not the intake wrapper `id` — the `Issue ID` column
  of `intake ls`. `intake delete` also deletes the underlying work item for any status other than
  `accepted`.
- **Optional flags are `Option<T>`: test with `is_some()`, not emptiness.** `wi update --labels ""`
  must send an empty label list, so the code checks `opts.labels.is_some()` (and `clear_labels`)
  rather than `!labels.is_empty()`, which would skip the field and leave the labels untouched.
- **Destructive commands are documented, not prompted.** No delete asks on stdin; the only
  interactive command is `configure`. A duplicate attachment name is refused with a validation error
  and a hint to pass `--force`, never confirmed. See
  [ADR-0008](docs/adr/0008-destructive-deletes-are-documented-not-prompted.md).
- **The API's own shapes leak, so a few fields are `serde_json::Value`.** Work-item `state`,
  `labels`, and `assignees` come back as raw UUID strings from the detail endpoint and as UUID lists
  or objects from the list endpoints; `planebotcli-types` keeps those polymorphic fields as `Value`
  and `render.rs` interprets them. Pages use `PageScope` for the two locations (workspace vs
  project), archiving must set `archived_at`, and estimate points have no endpoint at all — they are
  derived from work items with `expand=estimate_point`.
- **Reference versioned docs, not tracker issues.** Do not cite Plane/Linear issue IDs or external
  tracker URLs in code comments — they are unreachable after delivery. Point to an ADR or guide
  instead.
- **Tests never hit a real Plane instance.** Unit tests sit next to the code, and
  `crates/planebotcli-cli/tests/mock_e2e.rs` runs the real binary against a mockito server, pinning
  the `--json` contract and exit codes. Mock the HTTP layer (`mockito`, a `dev-dependency`), and
  prefer testing pure logic (normalizers, fuzzy matching, HTML conversion) directly. The mock e2e
  strips proxy variables from the child environment so local traffic is not hijacked.

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
   `feat/PLANECLI-42-relations-remove`). Never commit a task's work straight onto an integration or
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
(`planebotcli` / `pbot`, a Cargo workspace in `crates/`), and the Python line it replaced has since
been removed. The version and the changelog are edited on
`integration-main`, the branch tasks land on; there is no second repository they could be edited in
(see Identity).

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

- [CLI command reference](docs/cli-command-reference.md) — every command, flag, and alias (current)
- [Plane v1 API reference](docs/api/plane-v1-api.md) — the endpoints the CLI calls (current)
- [Architecture](docs/architecture.md), [Caching](docs/caching.md), [ADRs](docs/adr/) — historical:
  the layers, TTLs, and decisions of the Python implementation that has been removed
