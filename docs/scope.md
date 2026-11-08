# Scope

What `qsr` is for, what the first release covers, the server API it needs,
and the order the work is done in. Decided on 2026-11-09; update this page
when a decision changes.

## Purpose

Quasar is driven from its dashboard. `qsr` drives it from a terminal and from
pipelines, and is the first client of a JSON API that will grow to cover what
the dashboard does. The longer-term aim is a desktop application with features
a browser cannot offer (deploying a local folder, for one); the CLI comes
first because it is cheap, and because it proves the API on real use before
anything is built on top of it.

## Decisions

| Topic | Decision |
|---|---|
| Language | Rust, stable toolchain |
| Binary | `qsr` (crate `quasar-cli`) |
| Licence | MIT |
| Language of the tool | English: messages, help, docs, commits |
| Authentication | Personal tokens tied to an account, pasted at `qsr login`. No browser sign-in (device flow) for now |
| Token storage | Configuration file, owner-only permissions. `QSR_URL` / `QSR_TOKEN` override it |
| Servers | Named contexts, one per server, `--context` for a single command |
| Output | Readable tables by default, `--json` everywhere, colour only on a terminal and without `NO_COLOR` |
| Secrets | An admin's personal token reads and writes environments. A service token never reads them |
| Updates | `qsr update` replaces the binary from the latest GitHub release after a checksum check, and a daily check mentions new releases |
| Distribution | GitHub release archives for 5 targets, `install.sh` and `install.ps1` |
| Commits | Conventional Commits, one line, atomic, no co-author trailer |
| Versions | Tags `vX.Y.Z`, decided by the maintainer, never automatic |

## First release (v0.1.0)

- Contexts: `login`, `logout`, `whoami`, `context list|use|remove`.
- Applications: `apps`, `apps show`, `deploy` (waits for the outcome),
  `restart`, `start`, `stop`.
- Logs: `logs`, with `-n` and `-f`.
- Environment: `env`, `env set`, `env unset`.
- `update`.

Out of it, for later: deploying a local folder, creating and deleting
applications, domains, limits, backups, tasks, the desktop application, and
browser sign-in.

## Personal tokens (server side)

Created by any signed-in user in **Settings > Tokens**, beside the existing
service tokens (which are kept as they are).

- Belongs to the user who created it, and acts with that user's **current**
  role: demoting the user demotes the token.
- Deleted with the account.
- Expiry chosen at creation: 30 days, 90 days, 1 year or never. An expired
  token is refused like an unknown one.
- Shares the `qsr_` prefix and the SHA-256-at-rest storage of service tokens.
- Each user sees and revokes their own; an admin sees and revokes everyone's.
- The audit log attributes its actions to the user, through the token:
  `alice via laptop`.
- Failed token authentications count toward the same sign-in protection as
  failed passwords.

## API contract

All routes are under `/api/v1`, take `Authorization: Bearer <token>`, and
answer JSON. Errors are `{"error": "<message>"}` with a meaningful status:
401 unknown or expired token, 403 role or token kind not allowed, 404 unknown
application, 409 conflicting state.

Routes marked **new** are added to Quasar alongside the CLI command that needs
them, and documented in Quasar's `docs/openapi/quasar.yaml` in the same
change.

| Route | Who | Status | Purpose |
|---|---|---|---|
| `GET /me` | any token | **new** | Who the token is, and the server's version |
| `GET /apps` | any token | exists, **extended** | List, now with the deploy state |
| `GET /apps/{id}` | any token | exists, **extended** | One application, with the deploy state |
| `POST /apps/{id}/deploy` | admin | exists | Start a deploy (202) |
| `POST /apps/{id}/restart` | admin | exists | Restart |
| `POST /apps/{id}/start` | admin | **new** | Start |
| `POST /apps/{id}/stop` | admin | **new** | Stop |
| `GET /apps/{id}/logs` | any token | **new** | Recent lines, or a live stream |
| `GET /apps/{id}/env` | admin, personal | **new** | Read the environment |
| `PUT /apps/{id}/env` | admin | **new** | Replace the environment |

### `GET /me`

```json
{
  "kind": "personal",
  "token": "laptop",
  "user": "alice",
  "role": "admin",
  "expires_at": "2027-02-07T10:00:00Z",
  "server_version": "v0.5.3"
}
```

A service token answers `"kind": "service"` and `"user": null`. The CLI
checks `server_version` against the oldest Quasar it supports and says so
plainly when the server is too old, rather than failing on a missing route.

### Deploy state on an application

Each application gains:

```json
"deploy": { "running": false, "error": null }
```

`qsr deploy` posts the deploy, then polls the application until `running` is
false, and reports `error` when there is one. A deploy that is already
running answers 409.

### `GET /apps/{id}/logs`

Query: `tail` (lines, default 200, at most 5000), `follow` (`true` keeps the
response open). The body is newline-delimited JSON, one line per log line:

```json
{"time": "2026-11-09T10:00:00.123Z", "line": "GET / 200"}
```

`time` is null when Docker gave none. For a compose application, the lines
come from the same container the dashboard shows.

### `GET` and `PUT /apps/{id}/env`

```json
{ "content": "DATABASE_URL=postgres://...\n# a comment\nDEBUG=0\n" }
```

The environment is the dotenv text the dashboard edits, comments and order
included, so a round trip through the CLI keeps them. `qsr env set` and
`unset` read, change the lines concerned, and write the whole text back.
Saving does not redeploy, as on the dashboard; the CLI says that a redeploy
applies the change, and `--deploy` does it straight away.

Reading needs a personal token of an admin: 403 for a service token, 403 for
a viewer. Every read and every write is audited, without the values.

### Naming an application

The CLI accepts a name, a subdomain or an id. It resolves it from `GET /apps`
and refuses an ambiguous name, listing the candidates.

## Self-update

- Releases carry one archive per target (`qsr-<version>-<target>.tar.gz`, or
  `.zip` on Windows) and a `SHA256SUMS` file.
- `qsr update` reads the latest release from the GitHub API, downloads the
  archive for the running target, checks it against `SHA256SUMS`, and swaps
  the binary in place, which also works on Windows where a running binary
  cannot be overwritten.
- The daily check stores when it last ran beside the configuration. It runs
  only when stderr is a terminal and `QSR_NO_UPDATE_CHECK` is unset, never
  delays a command by more than a short timeout, and prints a single line
  after the command's own output.
- An installation that `qsr` cannot write to (a package manager's, a
  read-only directory) is reported with the command to run instead.

## Work order

Server changes go to the Quasar repository, in its own commit style, each
with tests. CLI changes go here.

1. CLI: README, this scope, licence.
2. CLI: cargo project, command skeleton, CI (format, lint, tests on Linux,
   macOS and Windows).
3. CLI: release workflow for the 5 targets with checksums, install scripts.
4. CLI: configuration file and contexts.
5. Server: personal tokens (storage, Settings page, authentication, audit,
   expiry) and `GET /me`.
6. CLI: HTTP client, `login`, `logout`, `whoami`.
7. CLI: `apps`, `apps show`, application naming.
8. Server: deploy state on applications, `start` and `stop`.
9. CLI: `deploy`, `restart`, `start`, `stop`.
10. Server: logs route. CLI: `logs`.
11. Server: environment routes. CLI: `env`.
12. CLI: `update` and the daily check.
13. Release v0.1.0, when the maintainer decides.
