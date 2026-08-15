# Phone access & full browser CRUD — design

Goal: create and modify notes, pulses, and metrics from both the CLI and a
phone (mobile browser), with a single trusted server on the LAN.

For a reference of every config and data file involved, see
[configuration.md](configuration.md).

## Surfaces and credentials

Two independent credentials protect two disjoint route sets. They are not
layered, not derivable from each other, and not interchangeable.

|                 | `viewer_secret`                         | API token (`ron token grant`)                |
|-----------------|------------------------------------------|----------------------------------------------|
| Protects        | HTML viewer routes (`/`, `/view/*`, `/pulses`, `/metrics`, `/notes/new`, `/login`, …) | JSON API routes (`/api/notes`, `/api/pulses`, `/api/metrics`, …) |
| Shape           | One user-chosen passphrase, shared by all browsers/phones | Per-device random secret, minted server-side, individually revocable |
| Stored (server) | `~/.config/ron/server.json` (`viewer_secret`) | `~/.config/ron/tokens.json` (token store)    |
| Stored (client) | only as a cookie after login             | `~/.config/ron/cli-token.json` on each CLI host |
| Presented by    | cookie `ron_viewer`, set after `/?key=` or `/login` POST | `Authorization: Bearer <secret>` header      |
| Minted how      | user types it into `server.json` by hand | `ron token grant <label>` generates & prints |
| Read how        | `ron viewer-key` reads `server.json` and prints (local file read, no server call) | already in `cli-token.json`; `ron token list` queries `/api/tokens` |
| Rotate          | edit `server.json` + restart → invalidates every browser cookie, forces re-login everywhere | `ron token revoke <id>` → kills one device only |

The only thing they share is the outermost trust ring: the server's network
bind. A browser cookie won't satisfy `/api/*`; a bearer header won't satisfy
the viewer gate.

## Network bind

`src/paths.rs`: default `listen` changes from `127.0.0.1:7780` to
`0.0.0.0:7780` so a phone on the LAN can reach the server out of the box.
Override as usual via `listen` in `~/.config/ron/server.json`.

## Viewer gate (opt-in)

When `viewer_secret` is present in `server.json`, all viewer routes require
the `ron_viewer` cookie. When absent, the viewer stays open (today's
behaviour) — backward compatible.

Cookie: `ron_viewer`, value = the secret, `SameSite=Strict`, `HttpOnly`,
`Path=/`, `Max-Age=30d`. Plain string compare against `viewer_secret` is
sufficient for a LAN-only, single-user app.

Unlock flows, both end in the same cookie:

1. **One-shot bootstrap:** `GET /?key=<secret>`. Middleware validates the
   query param against `viewer_secret`, sets the cookie, 302 → `/` (stripping
   the param from the URL bar). Bookmark `/` afterwards.
2. **Form fallback:** `GET /login` renders a password field; `POST /login`
   validates and sets the cookie, 302 → `/`. Use when you don't want the
   secret ever appearing in the URL/history.

If a viewer request has no cookie or a wrong one, viewer middleware redirects
to `/login`. `/login` itself (GET and POST) and `/static/*` are always
reachable.

## API gate

`/api/*` keeps using bearer tokens exactly as today. One tightening:

`/api/tokens` POST and `/api/tokens/:id` DELETE are **localhost-only** when
the server is bound externally. The middleware reads the peer `SocketAddr`
from axum's `ConnectInfo` and rejects non-loopback callers with 403. Rationale:
`/api/tokens` is auth-exempt by design (it can't require a token to mint a
token), and the roadmap's original safety assumption was a localhost bind.
With `0.0.0.0` that assumption no longer holds, so the loopback check restores
it without changing the CLI flow on the server host.

## Remote CLI

A CLI on a second machine (still on the LAN) works in three steps:

1. On the server host: `ron token grant <remote-label>` (peer is loopback,
   accepted).
2. Copy `~/.config/ron/cli-token.json` from the server host to the same path
   on the remote machine. (It's plain JSON: `{id, label, secret}`.)
3. On the remote: `export RON_URL=http://<server-lan-ip>:7780`, or set
   `{ "url": "http://<server-lan-ip>:7780" }` in the remote's
   `~/.config/ron/server.json`. Then `ron
   list`, `ron add`, etc. send `Authorization: Bearer <secret>` over the LAN.

The remote cannot self-grant a token (loopback-only), which is what keeps a
LAN snooper from minting API access.

## Browser CRUD parity

The viewer gains write forms for all three item types. Forms POST to
viewer routes whose handlers call shared `*_inner` helpers (same DB + YAML +
git logic the JSON API uses), so there is a single source of truth for the
business rules. The viewer routes remain cookie-gated (or open, if no
`viewer_secret`).

Notes:

- `GET  /notes/new`              — create form (title, tags `;`-sep, notebook, body)
- `POST /notes/new`              — create, redirect to `/view/:id`
- `GET  /notes/:id/edit`         — edit form, pre-filled (incl. `Related:`)
- `POST /notes/:id/edit`         — update, redirect to `/view/:id`
- `POST /notes/:id/delete`       — delete, redirect to `/`

Pulses (in addition to the existing check/uncheck toggles):

- `POST /pulses/new`             — create (topic + interval)
- `GET  /pulses/:id/edit`        — edit topic/interval
- `POST /pulses/:id/edit`        — update
- `POST /pulses/:id/delete`      — delete

Metrics (in addition to the existing read-only views):

- `POST /metrics/new`            — create (topic)
- `POST /metrics/:id/log`        — append a value (optional `--ts`)
- `GET  /metrics/:id/edit`       — edit topic
- `POST /metrics/:id/edit`       — update
- `POST /metrics/:id/delete`     — delete

## CLI gap-fill

Two new subcommands use endpoints that already exist:

- `ron pedit <id> [--topic ...] [--interval daily|weekly|monthly|yearly]`
- `ron medit <id> [--topic ...]`

Plus `ron viewer-key`, which reads `viewer_secret` from `server.json` and
prints it. It performs no server call and needs no token.

## Scope (explicitly out)

- Histogram / line-graph metric review (roadmap TODO) — not requested.
- Schema-migration engine (roadmap TODO) — unrelated.
- HTTPS / reverse proxy / public-internet hardening — LAN-only model.
