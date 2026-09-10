# planebotcli Documentation

`planebotcli` is a CLI for [Plane.so](https://plane.so) (SaaS or self-hosted).
It manages projects, work items, cycles, modules, documents, labels, states,
intake queues, and comments. Its defining feature is **fuzzy resource
resolution**: any resource can be referenced by a name, an identifier
(`ABC-123`), or a UUID.

The current release line is the Python implementation (v0.7.0); a Rust rewrite
is in progress (see [rust-rewrite.md](rust-rewrite.md)).

## Docs index

| Document | Contents |
|---|---|
| [cli-command-reference.md](cli-command-reference.md) | Every command, flag, alias, and an example |
| [api/plane-v1-api.md](api/plane-v1-api.md) | The Plane v1 API surface the CLI uses — method, path, params, request/response |
| [architecture.md](architecture.md) | Layered architecture, request flow, ADRs (Python line) |
| [caching.md](caching.md) | Cache TTLs, keys, invalidation |
| [adr/](adr/) | Architecture decision records |
| [rust-rewrite.md](rust-rewrite.md) | The Rust rewrite plan (crate layout, milestones) |

## Quick start

```bash
cargo install planebotcli       # once the Rust line is published (planned)
# or, today (Python line):
pip install planecli            # not yet published; install from the repo
```

Credentials: environment variables `PLANE_BASE_URL`, `PLANE_API_KEY`,
`PLANE_WORKSPACE`, or the `~/.plane_api` file (lowercase `key=value` lines:
`base_url`, `api_key`, `workspace`). Precedence: CLI flags > env vars >
`~/.plane_api`. See [src/planecli/config.py](../../src/planecli/config.py)
(`_FIELD_MAP`) in the Python line.

## Key concepts

- **Fuzzy resolution** — pass a name, an identifier, or a UUID anywhere a
  resource is expected; close names resolve via token-sort-ratio matching
  (threshold 60).
- **Dual output contract** — tables go to **stderr**, JSON to **stdout**:
  `planebotcli wi ls --json 2>/dev/null` yields clean JSON.
- **Caching** — reads are cached on disk with per-resource TTLs;
  `--no-cache` bypasses it for one command; `planebotcli cache clear` resets.
- **`me`** — the authenticated user, valid wherever an assignee is expected.
