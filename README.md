# qsr

The command-line client for [Quasar](https://github.com/AymericChaverot/quasar),
the self-hosted platform. `qsr` drives one or several Quasar servers from a
terminal or a pipeline: list applications, deploy them, follow their logs,
edit their environment.

```console
$ qsr login prod https://admin.example.com
Paste a personal token (Settings > Tokens on the dashboard):
Signed in to prod as alice (admin).

$ qsr apps
NAME      HOST                  TYPE     STATE
blog      blog.example.com      git      running
grafana   grafana.example.com   compose  running

$ qsr deploy blog
Deploying blog...
Deployed blog in 1m12s.
```

> **Status:** early development. [docs/scope.md](docs/scope.md) lists what the
> first release covers, the server API it relies on, and how the work is
> ordered.

## Install

### Linux and macOS

```sh
curl -sSL https://raw.githubusercontent.com/AymericChaverot/quasar-cli/main/install.sh | sh
```

### Windows (PowerShell)

```powershell
irm https://raw.githubusercontent.com/AymericChaverot/quasar-cli/main/install.ps1 | iex
```

Both scripts fetch the binary for your platform from the latest
[release](https://github.com/AymericChaverot/quasar-cli/releases), check it
against the release's checksums, and put it on your `PATH`. Prebuilt binaries
exist for Linux x86_64 and arm64, macOS Intel and Apple Silicon, and Windows
x86_64.

### From source

```sh
cargo install --git https://github.com/AymericChaverot/quasar-cli
```

## Updating

```sh
qsr update           # install the latest release over the running binary
qsr update --check   # only say whether there is one
```

The download is checked against the release's SHA-256 checksums before it
replaces anything. `qsr` also looks for a new release at most once a day and
mentions it on stderr after a command. Set `QSR_NO_UPDATE_CHECK=1` to turn
that off; it is always off when stderr is not a terminal, so pipelines never
see it.

## Signing in

`qsr` authenticates with a **personal token**, created on the dashboard under
**Settings > Tokens**. A personal token belongs to your account: it acts with
your current role (admin or viewer), stops working when the account is
deleted, and expires when you chose it would. Everything it does shows up in
the audit log under your name.

```sh
qsr login prod https://admin.example.com   # prompts for the token
qsr whoami                                 # who and where you are signed in
qsr logout prod
```

Service tokens (the ones made for CI, without an account) are accepted too,
with one limit: they never read an application's environment.

In a pipeline, skip `login` and pass the server and the token through the
environment:

```sh
QSR_URL=https://admin.example.com QSR_TOKEN=qsr_... qsr deploy blog
```

## Several servers

Each `login` creates a named **context**, like `kubectl` or `docker context`.

```sh
qsr context list
qsr context use staging
qsr --context prod apps        # one command against another server
```

Contexts live in the configuration file, along with their tokens:

| Platform | Path |
|---|---|
| Linux | `~/.config/quasar/config.toml` |
| macOS | `~/Library/Application Support/quasar/config.toml` |
| Windows | `%APPDATA%\quasar\config.toml` |

The file is written readable by its owner only. Treat it like an SSH key.

## Commands

| Command | Does |
|---|---|
| `qsr apps` | List applications and their state |
| `qsr apps show <app>` | Details of one application |
| `qsr deploy <app>` | Redeploy and wait for the outcome (`--detach` to return at once) |
| `qsr restart <app>`, `start`, `stop` | Lifecycle |
| `qsr logs <app>` | Recent logs (`-f` to follow, `-n` for the line count) |
| `qsr env <app>` | Show the environment (admin personal token) |
| `qsr env set <app> KEY=value ...` | Set variables |
| `qsr env unset <app> KEY ...` | Remove variables |
| `qsr whoami` | Current user, role and server |
| `qsr context list`, `use`, `remove` | Manage contexts |
| `qsr update` | Update `qsr` itself |

An application is named by its name, its subdomain or its id.

Every command accepts `--json` for scripts. Colours are dropped when the
output is not a terminal, and when `NO_COLOR` is set.

## Development

```sh
cargo build
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

Commits follow [Conventional Commits](https://www.conventionalcommits.org/):
`feat:`, `fix:`, `docs:`, `ci:`, `chore:`, `refactor:`, `test:`, with an
optional scope (`feat(logs): ...`). Pushing a `vX.Y.Z` tag builds the release
binaries and attaches them, with their checksums, to a GitHub release.

## License

[MIT](LICENSE)
