# planebotcli Rust rewrite

Tracked as **PLANECLI-9**. The Python CLI (v0.7.0) is feature-complete and
stable; this rewrite produces a single static Rust binary with the same
behavior, split into standardized crates that maximize reuse of the Rust
ecosystem and minimize hand-written code. Deadline for the core + distribution
milestones: **2026-09-11**.

## Why

- Python requires a >= 3.11 runtime and 8 third-party libraries
  (cyclopts/rich/rapidfuzz/plane-sdk/cashews/tenacity/loguru); a Rust binary
  is self-contained (no runtime) and starts instantly.
- Distribution: `cargo install`, npm/pnpm package, Python wheel, GitHub
  Releases — all from one source (see Distribution below).
- The Python codebase is ~6100 lines of source across 56 command functions;
  the crate split gives stable seams for the same behaviors.

## Baseline (what must be ported 1:1)

- 13 subcommands + `whoami`/`configure` (~56 command functions):
  project, wi, comment, attachment, doc, intake, label, state, module, cycle,
  user, cache.
- Fuzzy resolution (name / `ABC-123` / UUID) with token_sort_ratio, threshold 60.
- Dual output contract (JSON → stdout, table → stderr).
- Per-resource disk cache with TTLs and invalidation after writes.
- SDK escape hatches (raw paths for work-item detail, documents, attachment
  three-step upload, estimate points via expand).
- Write verification by read-back where the API silently ignores writes.
- Error mapping: Auth=2 / NotFound=3 / API=4 / Validation=5, messages carrying
  HTTP status + field-level detail.
- Plain-text→HTML and markdown-subset→HTML conversion for comments/bodies.
- `web_url` output, `--body-md`/`--desc-md`, write-before state/label
  validation (added in v0.7.0).

## Crate layout

```
Cargo workspace (root Cargo.toml; Python sources stay under src/ during the
transition)
crates/
├── planebotcli-types      # serde DTOs for every API resource + error bodies
├── planebotcli-client     # reqwest async client: v1 API, X-Api-Key, pagination,
│                          # attachment 3-step upload, HTTP error mapping
├── planebotcli-resolve    # fuzzy matching (rapidfuzz) + resolve_<resource>
├── planebotcli-cache      # TTL disk cache + invalidate
├── planebotcli-html       # body_to_html / md_to_html / linkify (pulldown-cmark)
├── planebotcli-format     # JSON → stdout, table → stderr (tabled)
├── planebotcli-core       # PlaneError (exit codes), config, shared helpers
└── planebotcli-cli        # clap binary `planebotcli`: command tree, global flags
```

### Dependency choices (reuse over hand-rolling)

clap 4 (derive) · tokio · reqwest (rustls) · serde/serde_json · thiserror +
anyhow · rapidfuzz · pulldown-cmark (real markdown, stronger than the Python
subset) · tabled · html-escape · regex · dirs · moka (or a file TTL cache).

### Design decisions

- No official Rust plane-sdk exists → `planebotcli-client` implements the v1
  API directly; the Python "escape hatches" become the normal path.
- Native async end to end (the Python line was a sync SDK behind `to_thread`).
- Naming: binary and every package are `planebotcli` (distinct from upstream
  `planecli`).

## Distribution

- **crates.io** → `cargo install planebotcli` (git install as fallback).
- **cargo-dist** produces GitHub Releases (per-platform static binaries +
  sha256), an npm package (per-platform optionalDependencies, pnpm-compatible),
  and shell/PowerShell installers.
- **Python**: a thin wheel that bundles the platform binary behind a console
  script → `pip install planebotcli`.
- Any-channel download; a stable binary URL + version manifest also keeps the
  door open for a future kimi-code `/plugin install` integration.

## Milestones (to 2026-09-11)

1. Skeleton: workspace + clap command tree + config + `whoami` + `project ls`
   (full pipeline working).
2. Core plumbing: types + client + resolve + cache + formatters.
3. Core commands: wi (list/show/create/update/delete/search/assign), comment,
   label/state, project.
4. Distribution chain: cargo-dist (crates.io/npm/Releases) + Python wheel,
   verified on all four install paths.
5. Remaining resources (cycle/module/intake/doc/attachment/user) and advanced
   behaviors (attachment upload, inline images, write verification,
   state/label validation, error detail), tests + live comparison.

## Acceptance

- Behavior parity with the Python v0.7.0 CLI (commands, flags, exit codes,
  JSON contract, fuzzy resolution, caching, write verification).
- `cargo test` green (pure logic: fuzzy, html conversion, error mapping,
  cache, config).
- Live verification of wi flow, comments, attachments, intake, doc.
- All four install paths produce the same working binary.
- Once ready, planebotcli `main` switches to the Rust line; the Python line
  freezes to bug fixes (kept on tags/branches).
