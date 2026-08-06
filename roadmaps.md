# Roadmaps

Version 2.x has the following enhancements:

## Architecture

Convert this app into a server-client pattern.

On the server side, the REST API provides:

* Notes manipulation: create, update, delete, view, list, search;
* Pulse and Metric manipulation (see below);
* Data manipulation: backup/sync data, import/export the whole dataset;
* Migrate notes stored in the 1.x format (current on-disk format) to the
  2.x format.

The server owns the git repo. The whole dataset is exported into YAML files
and committed into git by the server. SQLite is the working store; YAML files
on disk are the source of truth on cold start / sync. The server rebuilds
SQLite from YAML on `import`, and runs schema migrations during import.

Each note is stored as YAML with markdown body inside, so the markdown can be
rendered (code blocks, MathJax) properly in view mode in the browser. The YAML
on-disk format is versioned.

The client can be a browser or a CLI app. The server binds to localhost only.
Authentication uses a bearer token; the CLI can `grant`, `revoke`, and `list`
tokens, and tokens are stored in the server config (not in the git-tracked
dataset).

The CLI client edits an item (note, pulse or metric) with Neovim in terminal.
For `view`, the browser renders; the CLI falls back to `cat`-style plain
output.

## Data Items

### Note

Notes keep their current shape (title, tags, notebook, body, timestamps).

Each note has a unique ID of the form `note-20260806-1432-a8f.md` (date, time,
short random suffix). The `list` and `search` results expose this ID so it can
be referenced.

Add a "Related Notes" section in each note. To relate one note to another, the
user provides the target note's ID.

### Pulse

A *pulse* is a recurring boolean tracker with a predefined, fixed time
interval (daily, weekly, monthly, ...). Examples: "jog 15 minutes" (daily),
"call parents" (weekly).

User can *create* a pulse with a topic and an interval. He can *check* or
*uncheck* a pulse for the current interval, which means the target is met or
unmet.

Storage: `Timeseries<bool>` — one boolean per interval slot. This keeps the
model simple while leaving streak/consistency stats open for later.

The app only displays the *active* pulses according to their interval timing.

### Metric

A *metric* is a piece of data recorded at arbitrary time spots for long-term
analysis. Examples: sleep time, weight, BMI.

User can *create* a metric, and *update* it at any time. When updating, he
appends a timestamp (default: current time) and a value to that metric.

Storage: `Timeseries<f64>` (or `Timeseries<Value>` if non-numeric labels are
needed later).

On *review*, the app provides stats about that metric over a given time
interval: average, median, histogram, line graph.

## Cleanup

The patch-based backup/import flow (`backup-patch`, `import-patch`) and the
pickle cache are removed in 2.x, since the server-side SQLite store replaces
them.
