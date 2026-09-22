# planebotcli (pbot)

`pbot` is an independent command-line client for [Plane.so](https://plane.so) (SaaS or
self-hosted): projects, work items, cycles, modules, documents, labels, states, intake queues,
attachments, and comments.

Its defining feature is **fuzzy resource resolution**: anywhere a resource is expected you can
pass a name, an identifier (`ABC-123`), or a UUID. Close names resolve on their own —
`pbot wi ls -p "Front"` finds `Frontend`.

This repository is `planebotcli`: a Cargo workspace under `crates/`. The Python implementation that
used to ship in this repository has been removed; the docs in `docs/` that describe it are kept
as historical reference.

## Install

Pre-built binaries and installers are published on the
[Releases page](https://github.com/Liewzheng/planebotcli/releases). Pick whichever
form fits your platform — the shell installer is the same artifact on Linux and
macOS and is the fastest way to try the CLI; the GUI installers (`dmg` / `msi`)
are there for users who want a double-click install.

### macOS

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/Liewzheng/planebotcli/releases/latest/download/planebotcli-installer.sh | sh
```

Or download a `.dmg` from the latest release page (Apple Silicon: `planebotcli-aarch64-apple-darwin.dmg`;
Intel: `planebotcli-x86_64-apple-darwin.dmg`), open it, and drag `planebotcli` and `pbot` into a
directory on your `$PATH` — common choices are `~/bin` (user-local, no sudo needed),
`/opt/homebrew/bin` (Homebrew on Apple Silicon), or `/usr/local/bin` (system-wide; on newer
macOS this directory may require `sudo` to write to).

### Linux

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/Liewzheng/planebotcli/releases/latest/download/planebotcli-installer.sh | sh
```

Or download a `.deb` from the latest release page and install it with your
package manager — on Debian / Ubuntu (x86_64):

```sh
apt install ./planebotcli-cli_<version>_amd64.deb      # or sudo apt install
```

The arm64 build is `planebotcli-cli_<version>_arm64.deb` on aarch64 hosts;
`<version>` is the release's version number (e.g. `1.4.2`). Both are built by
the `build-linux-deb` job in `release.yml` — natively on `ubuntu-22.04`
(amd64) and `ubuntu-22.04-arm` (arm64), so no cross-compilation is involved.
Each `.deb` ships both the `planebotcli` binary and the `pbot` binary in
`/usr/bin/` — they are the two `[[bin]]` targets in
`planebotcli-cli/Cargo.toml`, and `pbot` is a short alias of the same CLI,
not a symlink.

### Windows

```powershell
irm https://github.com/Liewzheng/planebotcli/releases/latest/download/planebotcli-installer.ps1 | iex
```

Or download `planebotcli-x86_64-pc-windows-msvc.msi` from the latest release page and
double-click to install through the standard Windows installer UI. As an
alternative, download `planebotcli-x86_64-pc-windows-msvc.zip` and unzip it
somewhere on your `%PATH%`. ARM64 Windows is not currently a release target —
use the shell installer if you're on such a device.

### From a checkout

The CLI is not published to crates.io yet — install it from a checkout:

```bash
cargo install --path crates/planebotcli-cli --locked
```

That installs both binaries into `~/.cargo/bin`:

| Binary | Notes |
|---|---|
| `planebotcli` | Full package name. |
| `pbot` | Short alias of the same binary; used in every example here. |

Update later with the same command from a fresh checkout (`--force` if the version is unchanged).

## Configure

Credentials come from environment variables or a config file discovered from
several locations, highest priority first: `~/.config/pbot/config.toml` (TOML),
`~/.pbot`, `~/.planecli`, then the legacy `~/.plane_api` (lowercase `key=value` lines):

```ini
base_url=https://api.plane.so
api_key=your-personal-access-token
workspace=your-workspace-slug
```

`pbot configure` writes the active config file interactively (the highest-priority
existing candidate, `~/.plane_api` by default, `chmod 600`); the TOML file
takes the same keys at the top level or under `[auth]`. Exporting
`PLANE_BASE_URL` / `PLANE_API_KEY` / `PLANE_WORKSPACE` also works. Precedence is
**flags > environment variables > config file**, per setting. `base_url` is your instance
(`https://api.plane.so` for SaaS); `workspace` is the workspace slug, not the instance name.

## Quick start

```bash
pbot whoami                          # the authenticated user
pbot project ls                      # projects in the workspace
pbot wi ls -p "Frontend"             # work items, resolved by name
pbot wi create "Fix login timeout" -p "Frontend" --assignee me --priority urgent
pbot wi show ABC-123                 # by identifier, UUID, or name
```

Every command takes `--help`; `docs/cli-command-reference.md` lists every flag, alias, and
example.

## Output

Tables go to **stderr**, JSON to **stdout**, so a pipe always carries JSON alone:

```bash
pbot wi ls -p "Frontend" --json 2>/dev/null | jq '.[].name'
```

`--json 2>/dev/null` is the pattern for scripting; `--no-cache` bypasses the disk cache for one
command, and `pbot cache clear` empties it.

## Documentation

- [docs/README.md](docs/README.md) — documentation index and key concepts
- [docs/cli-command-reference.md](docs/cli-command-reference.md) — every command and flag
  ([中文](docs/cli-command-reference.zh.md))
- [AGENTS.md](AGENTS.md) — conventions and gotchas for contributors
- [skill/](skill/) — a portable skill that teaches AI coding agents to drive `pbot`

## License

MIT — see [LICENSE](LICENSE).
