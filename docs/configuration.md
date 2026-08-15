# Configuration & data files

Everything `ron` reads or writes on disk lives under two directories. This is
a complete reference; for the auth/security model behind the credentials see
[phone-access.md](phone-access.md).

| Path | What it is | Format | Written by |
|------|------------|--------|------------|
| `~/.config/ron/server.json` | listen address, optional viewer gate, CLI `url` fallback | JSON | `ron serve` (defaults); user-edits for `viewer_secret` / `url` |
| `~/.config/ron/tokens.json` | server-side store of API token hashes | JSON | server (`ron token grant` / `revoke`) |
| `~/.config/ron/cli-token.json` | raw API secret this machine sends | JSON | `ron token grant` |
| `~/.local/share/ron/db.sqlite3` | SQLite working store | binary | server, always |
| `~/.local/share/ron/repo/` | git repo of YAML — source of truth | YAML + git | server, every write commits |
| `~/.local/share/ron/repo/.gitignore` | keeps SQLite out of the repo | text | server (auto, once) |

`~` is the user's home. On Linux, `directories::ProjectDirs` resolves these to
`$XDG_CONFIG_HOME/ron` (or `~/.config/ron`) and `$XDG_DATA_HOME/ron` (or
`~/.local/share/ron`); see `src/paths.rs`.

## Config files (`~/.config/ron/`)

These three files live **outside** the git repo deliberately, so secrets and
machine-specific settings are never committed or synced.

### `server.json` — server configuration

`struct ServerConfig` in `src/paths.rs`. Schema:

```json
{
  "listen": "0.0.0.0:7780",
  "url": "http://192.168.1.5:7780",
  "viewer_secret": "optional passphrase"
}
```

- **`listen`** (string, default `"0.0.0.0:7780"`): socket the HTTP server binds.
  Bound on all interfaces by default so a phone or a second machine on the LAN
  can reach it; set `"127.0.0.1:7780"` to restore localhost-only.
- **`url`** (string, optional): base URL the CLI dials when `$RON_URL` is
  unset (`client::base_url`, `src/client.rs`; precedence env var → this key →
  `http://127.0.0.1:7780`). Set it on a **remote machine** to point its CLI at
  the server — it's a dial URL, deliberately separate from `listen`, which is
  a bind spec (`0.0.0.0` isn't a dialable address). Ignored by the server.
- **`viewer_secret`** (string, optional): when present, the browser/HTML routes
  require a `ron_viewer` cookie obtained via `/?key=<secret>` or `/login`. When
  absent, the viewer is open to anyone who can reach the port. Print the
  configured value with `ron viewer-key`.

Lifecycle: `ron serve` creates the file with defaults on first start if it's
absent (`ServerConfig::load`, `src/paths.rs`). It's safe to edit by hand at
any time; restart `ron serve` to pick up changes. Partial JSON is supported —
missing fields fall back to defaults.

### `tokens.json` — server-side API token store

`struct TokenStore` in `src/token.rs`. Schema:

```json
{
  "tokens": [
    { "id": "f3b63e7cf84a", "label": "laptop",
      "hash": "<sha256 hex of secret>", "created": "2026-08-06T20:45:07" }
  ]
}
```

This is the **server's half of the API gate**. Each entry records a token
minted by `ron token grant`; the `hash` field is the SHA-256 hex of the raw
secret — the raw secret is **never** stored here (it's only printed once at
grant time and kept client-side in `cli-token.json`). On every `/api/*`
request the server re-hashes the presented `Authorization: Bearer <secret>`
header and looks for a matching `hash` (`auth::require_token`,
`src/server/auth.rs`).

Lifecycle: loaded into memory on startup (`AppState::load_tokens`), and
re-saved after every grant/revoke. Mutating the file by hand is not
supported — use `ron token grant` / `ron token revoke <id>`.

Note: the mutating endpoints `POST /api/tokens` and `DELETE /api/tokens/:id`
are restricted to loopback peers, so on a LAN-bound server only the operator
on the server's own host can mint or revoke. `GET /api/tokens` (ids/labels/
created only, no secrets) stays open.

### `cli-token.json` — client-side API secret

`struct StoredToken` in `src/client.rs`. Schema:

```json
{ "id": "f3b63e7cf84a", "label": "laptop", "secret": "<raw secret>" }
```

The **client's half of the API gate**. `ron token grant` writes the raw secret
here so that every later CLI command can send it as `Authorization: Bearer
<secret>` (`client::auth`, `src/client.rs`). One file per machine; the server
host has its own for the local CLI.

Lifecycle: created by `ron token grant`; cleared by `ron token revoke <id>`
when the id matches the locally-stored one. To use the CLI from a **second
machine** on the LAN: grant on the server host, then copy this file to the
remote's `~/.config/ron/cli-token.json` and point the remote at the server
(`export RON_URL`, or the `url` key in its `server.json`). The
remote cannot self-grant (token grant is loopback-only).

## Data files (`~/.local/share/ron/`)

### `db.sqlite3` — SQLite working store

`src/db.rs`. The fast path for all reads/writes at runtime. Schema version is
tracked in its `meta` table (`SCHEMA_VERSION = 1`); `db::open` rejects a
mismatched version rather than migrating (a known TODO in `roadmaps.md`). The
DB is **rebuildable**: on cold start, if all data tables are empty and YAML
files exist, the server bootstraps from YAML (`bootstrap_from_yaml`,
`src/server/mod.rs`). `ron import` / `ron sync` drop and reload every row
from YAML (`rebuild_db_from_yaml`). Not git-tracked.

### `repo/` — git repo of YAML files

The **source of truth**. Each note/pulse/metric is one YAML file, stored
under a per-type subdirectory: `notes/note-<id>.yaml`,
`pulses/pulse-<id>.yaml`, `metrics/metric-<id>.yaml`. Every server write
rewrites the affected file and commits it (`persist_yaml` / `delete_yaml`,
`src/server/notes.rs`). The on-disk YAML format is versioned
(`FORMAT_VERSION = 2`, `src/yaml.rs`).

Older releases kept all YAML files flat in the repo root; the server
migrates that layout into the subdirectories automatically on startup
(one `layout:` commit), and still reads the flat layout if it finds one.

The git repo is what backs `ron backup` (`git push origin master`) and
`ron sync` (`git pull --ff-only origin master` then rebuild the DB). Configure
a remote once to enable them:

```
git -C ~/.local/share/ron/repo remote add origin <url>
```

Remote name defaults to `origin`, branch to `master` (`src/server/admin.rs`).

### `repo/.gitignore`

Auto-created by `AppState::new` on first start (`src/server/mod.rs`). Excludes
`*.sqlite*`, `*.db*`, `.wal`, `.shm` so the SQLite store is never tracked even
if it's ever moved into the repo dir. One-time write; safe to extend by hand.
