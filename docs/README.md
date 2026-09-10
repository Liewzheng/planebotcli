# planebotcli Documentation

`planebotcli` is a CLI for [Plane.so](https://plane.so) (SaaS or self-hosted).
It manages projects, work items, cycles, modules, documents, labels, states,
intake queues, and comments. Its defining feature is **fuzzy resource
resolution**: any resource can be referenced by a name, an identifier
(`ABC-123`), or a UUID.

The CLI is the Rust line (`planebotcli`, installed alongside the short `pbot`
alias); the Python implementation it replaced has been removed from the
repository, and the documents that describe it are marked as historical
(see [rust-rewrite.md](rust-rewrite.md)).

## Docs index

| Document | Contents |
|---|---|
| [cli-command-reference.md](cli-command-reference.md) | Every command, subcommand, flag, alias, and an example (English) |
| [cli-command-reference.zh.md](cli-command-reference.zh.md) | 同上，中文版 |
| [api/plane-v1-api.md](api/plane-v1-api.md) | The Plane v1 API surface the CLI uses — method, path, params, request/response |
| [architecture.md](architecture.md) | Layered architecture, request flow, ADRs (**historical: the removed Python line**) |
| [caching.md](caching.md) | Cache TTLs, keys, invalidation (**historical: the removed Python line**) |
| [adr/](adr/) | Architecture decision records (**historical: the removed Python line**) |
| [rust-rewrite.md](rust-rewrite.md) | The Rust rewrite plan — delivered (crate layout, milestones) |

## Quick start

```bash
cargo install --path crates/planebotcli-cli --locked   # from a checkout
```

Credentials: environment variables `PLANE_BASE_URL`, `PLANE_API_KEY`,
`PLANE_WORKSPACE`, or the `~/.plane_api` file (lowercase `key=value` lines:
`base_url`, `api_key`, `workspace`). Precedence: CLI flags > env vars >
`~/.plane_api`. See
[crates/planebotcli-core/src/config.rs](../crates/planebotcli-core/src/config.rs)
for the file format and the key-to-env-var mapping.

## Key concepts

- **Fuzzy resolution** — pass a name, an identifier, or a UUID anywhere a
  resource is expected; close names resolve via token-sort-ratio matching
  (threshold 60).
- **Dual output contract** — tables go to **stderr**, JSON to **stdout**:
  `planebotcli wi ls --json 2>/dev/null` yields clean JSON.
- **Caching** — reads are cached on disk with per-resource TTLs;
  `--no-cache` bypasses it for one command; `planebotcli cache clear` resets.
- **`me`** — the authenticated user, valid wherever an assignee is expected.
