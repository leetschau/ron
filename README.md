# ron: donno in Rust

Install: `cargo install --path .`

## Usage

ron is a server-client app: run the server once, then use the CLI (or a
browser at `http://<host>:7780`) against it.

### Start the server

```
ron serve
```

Listens on `0.0.0.0:7780` (override with `listen:` in
`~/.config/ron/server.json`) so a phone or a second machine on the LAN can
reach it. Data lives under `~/.local/share/ron/` (SQLite + git repo of YAML);
tokens under `~/.config/ron/`. See [docs/configuration.md](docs/configuration.md)
for a complete reference of every config and data file.

To stop a foreground server, press Ctrl-C (or `kill <pid>`): every write is
committed to SQLite and the git repo as it happens, so shutdown needs no
flush and is safe at any time. Restart to pick up edits to `server.json`.

To keep the server running across reboots and crashes, run it as a systemd
user service (next section).

### Manage with systemd

Everything ron touches lives under your home (`~/.config/ron`,
`~/.local/share/ron`) and 7780 is an unprivileged port, so a **user** service
needs no root. Create `~/.config/systemd/user/ron.service`:

```ini
[Unit]
Description=ron notes server

[Service]
ExecStart=%h/.local/bin/ron serve
Restart=on-failure

[Install]
WantedBy=default.target
```

Adjust `ExecStart` if your binary lives elsewhere (`~/.cargo/bin/ron` after a
plain `cargo install --path .`). Then:

```
systemctl --user daemon-reload
systemctl --user enable --now ron
systemctl --user status ron
journalctl --user -u ron -f      # logs; ron prints "ron listening on ..."
```

Start at boot without anyone logged in (user services otherwise start at
first login):

```
loginctl enable-linger $USER     # prefix with sudo on most distros
```

`systemctl --user stop ron` (or `reboot`) stops it safely — same as Ctrl-C on
a foreground server. If `ron backup`/`sync` push over SSH, the service must
be able to use your key: either an unencrypted `~/.ssh` key or an agent in
the user session.

### Phone / browser access

Open `http://<server-lan-ip>:7780/` in a browser. The viewer lets you read
**and** write notes, pulses, and metrics (create / edit / delete forms).

By default the viewer is open to anyone who can reach the port. To gate it,
add a passphrase to `~/.config/ron/server.json`:

```json
{
  "listen": "0.0.0.0:7780",
  "viewer_secret": "your-passphrase",
  "default_notebook": "default",
  "editor": "code -w",
  "cli_viewer": "mdless",
  "viewer": true
}
```

- `default_notebook` — notebook for new notes without one; the server is the
  authority (CLI fetches it from the server for the `ron add` prefill, local
  value is an offline fallback)
- `editor` — editor command for `ron add`/`ron edit` (args allowed); beats
  `$EDITOR`, which beats the `nvim` fallback
- `cli_viewer` — command `ron view` pipes notes through (default `mdless`;
  set `""` for raw cat-style stdout)
- `viewer` — set `false` to serve the API only (no HTML pages)

Then on the phone either:

- visit `http://<server-lan-ip>:7780/?key=your-passphrase` once (sets a
  30-day cookie, redirects to `/`), or
- open `http://<server-lan-ip>:7780/login` and type the passphrase.

Print the configured passphrase any time with `ron viewer-key`. See
[docs/phone-access.md](docs/phone-access.md) for the full security model.

### Auth

The CLI authenticates with a bearer token it stores at
`~/.config/ron/cli-token.json`. Mint one once per machine:

```
ron token grant my-laptop   # prints the secret once and saves it locally
ron token list              # show token ids (no secrets)
ron token revoke <id>
```

`POST /api/tokens` and `DELETE /api/tokens/:id` are restricted to loopback
peers, so a token can only be minted on the server's own host. To use the CLI
from a **second machine** on the LAN: run `ron token grant <label>` on the
server host, copy `~/.config/ron/cli-token.json` to the remote machine, and
point it at the server — either `export RON_URL=http://<server-lan-ip>:7780`
or `{ "url": "http://<server-lan-ip>:7780" }` in the remote's
`~/.config/ron/server.json`.

### Notes

```
ron add                       # open $EDITOR on a template; saves a new note
ron list            [n]       # n most-recent notes (default 5)
 ron view            <id>      # print a note through `cli_viewer` (default `mdless`)
ron edit            <id>      # open $EDITOR on an existing note
ron delete          <id>      # delete by ID (or 1-based index from list/search)
ron search          [opts] PATTERN [PATTERN...]
                              # scope prefix per pattern: t:/g:/n:/a:
                              #   --field title|tags|notebook|content
                              #   -C, --case       case-sensitive
                              #   -w, --whole      whole-word match
ron list-notebook             # unique notebooks
ron relate          <id> <to...>   # add related note IDs to a note
```

`list`/`search` print the note ID in its own column so you can pass it to
`view`/`edit`/`delete`/`relate`. You can also pass a 1-based index from the
last listing instead of an ID; `view`/`edit`/`delete` default to `1` (most
recent note). Short aliases: `a` add, `e` edit, `del` delete, `v` view,
`l` list, `s` search, `lnb` list-notebook. From the browser, use the `+ new`
link, and the edit/delete actions on each note.

### Pulses (recurring boolean trackers)

```
ron padd     <topic>                  # --interval daily|weekly|monthly|yearly
ron pcheck   <id>                     # mark today's slot met  (--on YYYY-MM-DD)
ron puncheck <id>                     # mark unmet
ron plist   [--active]                # list pulses (only today's open ones)
ron pedit     <id> [--topic ...] [--interval daily|weekly|monthly|yearly]
ron pdel     <id>
```

The browser `/pulses` page has a create form, per-row check/uncheck,
edit, and delete.

### Metrics (free-form numeric time series)

```
ron madd    <topic>
ron mlog    <id> <value> [--ts YYYY-MM-DDTHH:MM:SS]
ron mstats  <id> [--from ...] [--to ...]   # count/mean/median/min/max
ron mlist
ron medit   <id> [--topic ...]
ron mdel    <id>
```

The browser `/metrics` page and each `/metrics/<id>` detail page offer
create, log-value, edit-topic, and delete forms.

### Backup & sync

The server owns a git repo at `~/.local/share/ron/repo`. Every write
auto-commits the affected YAML file. To use backup/sync, point the repo at a
remote once:

```
git -C ~/.local/share/ron/repo remote add origin <url>
```

Then:

```
ron export     # rewrite all YAML files from the DB (full reconcile)
ron import     # reload the DB from the YAML files on disk
ron backup     # git push origin master
ron sync       # git pull --ff-only, then rebuild the DB
```

### Migrate from 1.x

```
ron migrate <old-notes-dir> <new-yaml-dir>            # interactive: prompts on title/created mismatches
ron migrate <old-notes-dir> <new-yaml-dir> --fix-all  # auto-fix all mismatches
ron migrate <old-notes-dir> <new-yaml-dir> --keep-all # keep originals, no prompt
# then move the new *.yaml files into ~/.local/share/ron/repo/notes/ and run `ron import`
```

When a note's title contains a date that differs from its `Created:` field
(common after a bulk import), migrate offers to rewrite `Created` to the
title's date. Options: `y` yes / `n` no / `a` yes-for-all / `s` skip-all /
`q` quit.

`ron migrate` converts note text only. Image files referenced as
`resources/<name>` must be copied by hand — see
[Images / attachments](#images--attachments) below.

### Browser

Open `http://127.0.0.1:7780/` for the notes index, and `/view/<note-id>` to
read a rendered note (markdown + MathJax). `/search` offers incremental
full-text search plus advanced filters (field, case, whole-word, updated-time
range, order, limit).

Images and other attachments referenced as `resources/<file>` in note bodies
are served from `~/.local/share/ron/repo/resources/` at `/resources/<file>`
(and ride along `backup`/`sync` with the git repo).

### Images / attachments

Note bodies may reference files as `resources/<name>` (the 1.x convention,
e.g. `![image](resources/<hash>.png)`). The viewer rewrites those to the
`/resources/<name>` route at render time, so they resolve on any page. Drop
the files into the repo's resources dir (no restart needed — files are read
per request):

```
mkdir -p ~/.local/share/ron/repo/resources
cp <old-notes-dir>/resources/* ~/.local/share/ron/repo/resources/
git -C ~/.local/share/ron/repo add -A
git -C ~/.local/share/ron/repo commit -m "import image resources"
```

(`ron export` commits them too — it stages the whole tree.)

## Development

```
cargo run -- serve    # run the server from source
cargo test
cargo build --release
```

The Nix devshell (`flake.nix`) provides the toolchain; prefix commands with
`nix develop --command ...` as in the musl build below.

### Static binary (portable across Linux hosts)

A plain `cargo build --release` links against the build host's libc. Under
Nix that means the binary's ELF interpreter points into `/nix/store/...`,
so it only runs on that machine (executing it elsewhere fails with
"Check the interpreter or linker?"). To get a fully static, portable
Linux binary, build for the musl target:

```
# from the GitHub release (CI builds static musl binaries):
#   https://github.com/leetschau/ron/releases  ->  ron-x86_64-linux.tar.gz

# or locally via the Nix devshell (flakes the musl toolchain):
nix develop --command cargo build --release --target x86_64-unknown-linux-musl
cp target/x86_64-unknown-linux-musl/release/ron ~/.local/bin/ron
```

Note the output lives under `target/<triple>/release/`, not
`target/release/` — copying the wrong one is an easy mistake.

