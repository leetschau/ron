//! ron CLI.
//!
//! Subcommand groups:
//!   - `serve`                            run the HTTP server
//!   - `migrate <src> <dst>`              1.x -> 2.x YAML migration (P1)
//!   - `token grant|list|revoke`          bearer-token management
//!   - `viewer-key`                       print the viewer passphrase
//!   - Notes:   add / edit / delete / view / list / search / relate
//!   - Drafts:  draft edit|list|clear     note-edit recovery cache
//!   - Pulses:  padd / pcheck / puncheck / plist / pedit / pdel
//!   - Metrics: madd / mlog / mstats / mlist / medit / mdel

use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};

fn main() -> Result<()> {
    let args = parse_args();
    let matches = args.subcommand();
    match matches {
        Some(("serve", _)) => run_serve(),
        Some(("migrate", sub)) => run_migrate(sub),
        Some(("token", sub)) => run_token(sub),
        Some(("add", _)) => notes_cmd::add(),
        Some(("edit", sub)) => notes_cmd::edit(index_or_id(sub)),
        Some(("delete", sub)) => notes_cmd::delete(index_or_id(sub)),
        Some(("draft", sub)) => match sub.subcommand() {
            Some(("edit", m)) => drafts_cmd::edit(m.get_one::<String>("key").unwrap().clone()),
            Some(("list", _)) => drafts_cmd::list(),
            Some(("clear", m)) => drafts_cmd::clear(m.get_one::<String>("key").cloned()),
            _ => unreachable!("subcommand_required prevents None"),
        },
        Some(("view", sub)) => notes_cmd::view(index_or_id(sub)),
        Some(("list", sub)) => {
            let n: u32 = sub.get_one::<String>("number").map(|s| s.parse().unwrap_or(5)).unwrap_or(5);
            notes_cmd::list(Some(n))
        }
        Some(("search", sub)) => {
            let ptns: Vec<String> = sub
                .get_many::<String>("patterns")
                .map(|v| v.cloned().collect())
                .unwrap_or_default();
            let field = sub.get_one::<String>("field").cloned().unwrap_or_else(|| "content".into());
            let ignore_case = !*sub.get_one::<bool>("case").unwrap_or(&false);
            let whole_word = *sub.get_one::<bool>("whole").unwrap_or(&false);
            notes_cmd::search(&ptns, &field, ignore_case, whole_word)
        }
        Some(("list-notebook", _)) => notes_cmd::list_notebooks(),
        Some(("relate", sub)) => {
            let id = sub.get_one::<String>("id").unwrap().clone();
            let related: Vec<String> = sub
                .get_many::<String>("to")
                .map(|v| v.cloned().collect())
                .unwrap_or_default();
            notes_cmd::relate(&id, related)
        }
        Some(("padd", sub)) => pulses_cmd::add(sub),
        Some(("pcheck", sub)) => pulses_cmd::set_check(sub, true),
        Some(("puncheck", sub)) => pulses_cmd::set_check(sub, false),
        Some(("plist", sub)) => pulses_cmd::list(sub),
        Some(("pedit", sub)) => pulses_cmd::edit(sub),
        Some(("pdel", sub)) => pulses_cmd::delete(sub),
        Some(("madd", sub)) => metrics_cmd::add(sub),
        Some(("mlog", sub)) => metrics_cmd::log(sub),
        Some(("mstats", sub)) => metrics_cmd::stats(sub),
        Some(("mlist", _)) => metrics_cmd::list(),
        Some(("mdel", sub)) => metrics_cmd::delete(sub),
        Some(("medit", sub)) => metrics_cmd::edit(sub),
        Some(("export", _)) => admin_cmd::export(),
        Some(("import", _)) => admin_cmd::import(),
        Some(("backup", sub)) => {
            admin_cmd::backup(*sub.get_one::<bool>("dry-run").unwrap_or(&false))
        }
        Some(("sync", _)) => admin_cmd::sync(),
        Some(("viewer-key", _)) => admin_cmd::viewer_key(),
        _ => unreachable!("subcommand_required prevents None"),
    }
}

fn index_or_id(sub: &clap::ArgMatches) -> String {
    sub.get_one::<String>("target").cloned().unwrap_or_else(|| "1".into())
}

fn parse_args() -> clap::ArgMatches {
    use clap::{command, Arg, ArgAction, Command};
    command!()
        .propagate_version(true)
        .subcommand_required(true)
        // ---- server ----
        .subcommand(Command::new("serve").about("run the ron HTTP server"))
        .subcommand(
            Command::new("migrate")
                .about("migrate notes from 1.x markdown to 2.x YAML")
                .arg(Arg::new("src").required(true))
                .arg(Arg::new("dst").required(true))
                .arg(
                    Arg::new("fix-all")
                        .long("fix-all")
                        .action(ArgAction::SetTrue)
                        .help("rewrite Created to the title's date for every mismatch (non-interactive)"),
                )
                .arg(
                    Arg::new("keep-all")
                        .long("keep-all")
                        .action(ArgAction::SetTrue)
                        .help("keep original Created on every mismatch (non-interactive)"),
                ),
        )
        .subcommand(
            Command::new("token")
                .about("manage bearer tokens")
                .subcommand_required(true)
                .subcommand(
                    Command::new("grant")
                        .about("mint a token; secret is saved locally for later commands")
                        .arg(Arg::new("label").default_value("cli")),
                )
                .subcommand(Command::new("list").about("show tokens (no secrets)"))
                .subcommand(
                    Command::new("revoke")
                        .about("revoke a token by id (clears local secret if it matches)")
                        .arg(Arg::new("id").required(true)),
                ),
        )
        // ---- notes ----
        .subcommand(Command::new("add").visible_alias("a").about("add a new note"))
        .subcommand(
            Command::new("edit")
                .visible_alias("e")
                .about("edit a note by ID (1-based index from `list`/`search`)")
                .arg(Arg::new("target").default_value("1")),
        )
        .subcommand(
            Command::new("delete")
                .visible_alias("del")
                .about("delete a note by ID or index")
                .arg(Arg::new("target").default_value("1")),
        )
        .subcommand(
            Command::new("view")
                .visible_alias("v")
                .about("view a note by ID or index (cat to stdout)")
                .arg(Arg::new("target").default_value("1")),
        )
        .subcommand(
            Command::new("list")
                .visible_alias("l")
                .about("list recent notes")
                .arg(Arg::new("number").default_value("5")),
        )
        .subcommand(
            Command::new("search")
                .visible_alias("s")
                .about("search notes; supports `t:`/`g:`/`n:`/`a:` scope prefixes per pattern (defaults to a/content)")
                .arg(
                    Arg::new("patterns")
                        .action(ArgAction::Append)
                        .required(true),
                )
                .arg(
                    Arg::new("field")
                        .long("field")
                        .short('f')
                        .default_value("content")
                        .help("content | title | tags | notebook"),
                )
                .arg(
                    Arg::new("case")
                        .long("case")
                        .short('C')
                        .action(ArgAction::SetTrue)
                        .help("case-sensitive"),
                )
                .arg(
                    Arg::new("whole")
                        .long("whole")
                        .short('w')
                        .action(ArgAction::SetTrue)
                        .help("match whole words only"),
                ),
        )
        .subcommand(
            Command::new("list-notebook")
                .visible_alias("lnb")
                .about("list notebooks"),
        )
        .subcommand(
            Command::new("relate")
                .about("add related note IDs to a note")
                .arg(Arg::new("id").required(true).help("note ID"))
                .arg(Arg::new("to").required(true).num_args(1..).help("note ID(s) to relate")),
        )
        .subcommand(
            Command::new("draft")
                .about("manage note drafts (recovery cache for interrupted create/edit)")
                .subcommand_required(true)
                .subcommand(
                    Command::new("edit")
                        .about("edit a draft in $EDITOR without creating a note")
                        .arg(Arg::new("key").default_value("new").help("new | note:<id>")),
                )
                .subcommand(Command::new("list").about("list cached drafts (server + local)"))
                .subcommand(
                    Command::new("clear")
                        .about("delete draft(s); omit the key to clear all")
                        .arg(Arg::new("key").help("new | note:<id> (default: all)")),
                ),
        )
        // ---- pulses ----
        .subcommand(
            Command::new("padd")
                .about("create a pulse: --topic ... --interval daily|weekly|monthly|yearly")
                .arg(Arg::new("topic").required(true))
                .arg(
                    Arg::new("interval")
                        .short('i')
                        .long("interval")
                        .default_value("daily"),
                ),
        )
        .subcommand(
            Command::new("pcheck")
                .about("check a pulse for today (or --on YYYY-MM-DD)")
                .arg(Arg::new("id").required(true))
                .arg(Arg::new("on").long("on").short('o')),
        )
        .subcommand(
            Command::new("puncheck")
                .about("uncheck a pulse for today (or --on YYYY-MM-DD)")
                .arg(Arg::new("id").required(true))
                .arg(Arg::new("on").long("on").short('o')),
        )
        .subcommand(
            Command::new("plist")
                .about("list pulses (pass --active to show only active)")
                .arg(Arg::new("active").long("active").short('a').action(ArgAction::SetTrue)),
        )
        .subcommand(
            Command::new("pedit")
                .about("edit a pulse's topic and/or interval")
                .arg(Arg::new("id").required(true))
                .arg(Arg::new("topic").long("topic").short('t'))
                .arg(
                    Arg::new("interval")
                        .long("interval")
                        .short('i')
                        .help("daily | weekly | monthly | yearly"),
                ),
        )
        .subcommand(
            Command::new("pdel").about("delete a pulse").arg(Arg::new("id").required(true)),
        )
        // ---- metrics ----
        .subcommand(
            Command::new("madd")
                .about("create a metric")
                .arg(Arg::new("topic").required(true)),
        )
        .subcommand(
            Command::new("mlog")
                .about("append a value to a metric")
                .arg(Arg::new("id").required(true))
                .arg(Arg::new("value").required(true))
                .arg(Arg::new("ts").long("ts").help("YYYY-MM-DDTHH:MM:SS (default: now)")),
        )
        .subcommand(
            Command::new("mstats")
                .about("show stats for a metric (optionally --from / --to)")
                .arg(Arg::new("id").required(true))
                .arg(Arg::new("from").long("from"))
                .arg(Arg::new("to").long("to")),
        )
        .subcommand(Command::new("mlist").about("list metrics"))
        .subcommand(
            Command::new("mdel").about("delete a metric").arg(Arg::new("id").required(true)),
        )
        .subcommand(
            Command::new("medit")
                .about("edit a metric's topic")
                .arg(Arg::new("id").required(true))
                .arg(Arg::new("topic").long("topic").short('t')),
        )
        // ---- admin ----
        .subcommand(
            Command::new("export")
                .about("rewrite all YAML files from the DB; git add+commit"),
        )
        .subcommand(
            Command::new("import").about("reload the DB from the YAML files on disk"),
        )
        .subcommand(
            Command::new("backup")
                .about("git push origin master (the repo must have a remote)")
                .arg(
                    Arg::new("dry-run")
                        .long("dry-run")
                        .action(ArgAction::SetTrue)
                        .help("show ahead/behind vs origin and hints; don't push"),
                ),
        )
        .subcommand(
            Command::new("sync")
                .about("git pull --ff-only origin master, then reload DB from YAML"),
        )
        .subcommand(
            Command::new("viewer-key")
                .about("print the configured viewer_secret (for the phone unlock URL/form)"),
        )
        .get_matches()
}

// ----- server / migrate / token -----

fn run_serve() -> Result<()> {
    let paths = ron::Paths::detect()?;
    let cfg = ron::ServerConfig::load(&paths)?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(ron::server::app::run(paths, cfg))?;
    Ok(())
}

fn run_migrate(sub: &clap::ArgMatches) -> Result<()> {
    let src: PathBuf = sub.get_one::<String>("src").unwrap().into();
    let dst: PathBuf = sub.get_one::<String>("dst").unwrap().into();
    let fix_all = *sub.get_one::<bool>("fix-all").unwrap_or(&false);
    let keep_all = *sub.get_one::<bool>("keep-all").unwrap_or(&false);

    let report = if fix_all {
        ron::migrate::migrate_dir_with(&src, &dst, |_, _| ron::migrate::FixDecision::FixAll)
    } else if keep_all {
        ron::migrate::migrate_dir(&src, &dst)
    } else {
        ron::migrate::migrate_dir_with(&src, &dst, prompt_created_fix)
    };

    if let Some(fatal) = report.fatal {
        eprintln!("fatal: {fatal}");
        std::process::exit(1);
    }
    if report.aborted {
        println!("aborted by user");
    }
    println!("migrated {} note(s)", report.succeeded.len());
    if report.fixed > 0 {
        println!("fixed {} Created timestamp(s) from title dates", report.fixed);
    }
    for (path, id) in &report.succeeded {
        println!("  {} -> {id}", path.display());
    }
    if !report.failed.is_empty() {
        eprintln!("{} file(s) failed:", report.failed.len());
        for (path, err) in &report.failed {
            eprintln!("  {}: {err}", path.display());
        }
    }
    if !report.skipped.is_empty() {
        eprintln!("{} file(s) skipped", report.skipped.len());
    }
    Ok(())
}

/// Ask the user whether to rewrite a note's `Created` to the date found in its
/// title. Keys: y=yes, n=no, a=yes-for-all, s=skip-all, q=abort. On EOF /
/// non-interactive stdin the safe default is "keep".
fn prompt_created_fix(parsed: &ron::migrate::ParsedV1, title_date: chrono::NaiveDate) -> ron::migrate::FixDecision {
    use std::io::{self, Write};
    loop {
        print!(
            "  {:?}: title date {} != Created {}. Fix Created to {}? [y]es / [n]o / yes-to-[a]ll / [s]kip-all / [q]uit: ",
            parsed.title, title_date, parsed.created.date(), title_date
        );
        let _ = io::stdout().flush();
        let mut line = String::new();
        if io::stdin().read_line(&mut line).unwrap_or(0) == 0 {
            return ron::migrate::FixDecision::Keep; // EOF / non-tty: safe default
        }
        match line.trim().to_ascii_lowercase().as_str() {
            "y" | "yes" => return ron::migrate::FixDecision::Fix,
            "n" | "no" => return ron::migrate::FixDecision::Keep,
            "a" | "all" => return ron::migrate::FixDecision::FixAll,
            "s" | "skip" => return ron::migrate::FixDecision::KeepAll,
            "q" | "quit" | "abort" => return ron::migrate::FixDecision::Abort,
            _ => eprintln!("    please answer y / n / a / s / q"),
        }
    }
}

fn run_token(sub: &clap::ArgMatches) -> Result<()> {
    use ron::client::{self, StoredToken};
    match sub.subcommand() {
        Some(("grant", m)) => {
            let label = m.get_one::<String>("label").unwrap().clone();
            let resp: serde_json::Value = client::Api::post_json_no_auth(
                "/api/tokens",
                &serde_json::json!({ "label": label }),
            )
            .and_then(|r| json_or_err::<serde_json::Value>(r))
            .context("token grant failed; is the server running?")?;
            let stored = StoredToken {
                id: resp["id"].as_str().context("missing id")?.to_string(),
                label: resp["label"].as_str().context("missing label")?.to_string(),
                secret: resp["secret"].as_str().context("missing secret")?.to_string(),
            };
            client::save_token(&stored)?;
            println!("id:     {}", stored.id);
            println!("label:  {}", stored.label);
            println!("secret: {}", stored.secret);
            println!("(saved to {})", client::token_file()?.display());
        }
        Some(("list", _)) => {
            let resp: serde_json::Value =
                client::Api::get_no_auth("/api/tokens").and_then(|r| json_or_err::<serde_json::Value>(r))?;
            for t in resp.as_array().unwrap_or(&vec![]) {
                println!("{}\t{}\t{}", t["id"], t["label"], t["created"]);
            }
        }
        Some(("revoke", m)) => {
            let id = m.get_one::<String>("id").unwrap().clone();
            client::Api::delete_no_auth(&format!("/api/tokens/{id}"))?;
            if let Some(stored) = client::load_token()? {
                if stored.id == id {
                    client::clear_token()?;
                }
            }
            println!("revoked {id}");
        }
        _ => unreachable!(),
    }
    Ok(())
}

/// Top-level version of the helper, used by main's token subcommand before
/// any auth is in place.
fn json_or_err<T: for<'de> serde::Deserialize<'de>>(resp: reqwest::blocking::Response) -> Result<T> {
    let status = resp.status();
    if status.is_success() {
        return Ok(resp.json()?);
    }
    let body = resp.text().unwrap_or_default();
    let detail = match serde_json::from_str::<serde_json::Value>(&body) {
        Ok(v) => v["error"].as_str().unwrap_or(&body).to_string(),
        Err(_) => body,
    };
    Err(anyhow!("HTTP {status}: {detail}"))
}

// ----- note commands -----

mod notes_cmd {
    use super::*;
    use ron::client;
    use ron::models::{DraftContent, Note};
    use ron::editor::EditOutcome;

    pub fn add() -> Result<()> {
        // The server is the authority for the default notebook; the local
        // server.json value is only a fallback (server unreachable / no
        // token yet).
        let notebook = client::server_default_notebook()
            .unwrap_or_else(|_| ron::paths::read_default_notebook());
        let mut initial = format!("Title: \nTags: \nNotebook: {notebook}\n\n------\n\n");
        let mut from_draft = false;
        if let Some(d) = resolve_draft("new") {
            eprintln!(
                "prefilling draft saved at {} (discard with `ron draft clear new`)",
                d.updated.format("%Y-%m-%d %H:%M")
            );
            initial = d.buffer;
            from_draft = true;
        }
        let outcome = ron::editor::edit(&initial)?;
        finish_edit_session("new", &outcome, &initial, from_draft, |parsed| {
            let note =
                client::create_note(&parsed.title, parsed.tags, &parsed.notebook, &parsed.body)?;
            Ok(format!("created {}", note.id))
        })
    }

    pub fn edit(target: String) -> Result<()> {
        let id = resolve_target(&target)?;
        let key = format!("note:{id}");
        // Offline fallback: if the note can't be fetched but a cached draft
        // exists, keep working on the draft (it can only be cached back).
        let note = match client::get_note(&id) {
            Ok(n) => n,
            Err(e) => {
                let local = client::drafts_file()
                    .ok()
                    .and_then(|p| client::load_local_draft(&p, &key));
                let Some(local) = local else { return Err(e) };
                eprintln!("warning: server unreachable ({e:#}); opening the cached draft");
                let initial = local.content;
                let outcome = ron::editor::edit(&initial)?;
                return finish_edit_session(&key, &outcome, &initial, true, |_parsed| {
                    Err(anyhow!("update failed: server unreachable ({e:#})"))
                });
            }
        };
        let mut initial = format!(
            "Title: {}\nTags: {}\nNotebook: {}\nRelated: {}\n\n------\n\n{}",
            note.title,
            note.tags.join("; "),
            note.notebook,
            note.related.join("; "),
            note.body,
        );
        let mut from_draft = false;
        if let Some(d) = resolve_draft(&key).filter(|d| d.updated > note.updated) {
            eprintln!(
                "prefilling draft saved at {} (discard with `ron draft clear {key}`)",
                d.updated.format("%Y-%m-%d %H:%M")
            );
            initial = d.buffer;
            from_draft = true;
        }
        let outcome = ron::editor::edit(&initial)?;
        let id = note.id.clone();
        let related = note.related.clone();
        finish_edit_session(&key, &outcome, &initial, from_draft, move |parsed| {
            let updated = client::update_note(
                &id,
                Some(parsed.title),
                Some(parsed.tags),
                Some(parsed.notebook),
                Some(parsed.body),
                Some(related),
            )?;
            Ok(format!("updated {}", updated.id))
        })
    }

    pub fn delete(target: String) -> Result<()> {
        let id = resolve_target(&target)?;
        client::delete_note(&id)?;
        println!("deleted {id}");
        Ok(())
    }

    pub fn view(target: String) -> Result<()> {
        let id = resolve_target(&target)?;
        let note = client::get_note(&id)?;
        let text = format!(
            "Title: {}\nTags: {}\nNotebook: {}\nRelated: {}\nCreated: {}\nUpdated: {}\nID: {}\n\n{}",
            note.title,
            note.tags.join("; "),
            note.notebook,
            note.related.join("; "),
            note.created.format("%F %T"),
            note.updated.format("%F %T"),
            note.id,
            note.body,
        );
        run_cli_viewer(&text)
    }

    /// Pipe `text` through the configured `cli_viewer` command (default
    /// `mdless`; see `server.json`). An empty command prints raw. If the
    /// command can't be spawned (e.g. not installed), fall back to raw
    /// printing with a warning on stderr.
    fn run_cli_viewer(text: &str) -> Result<()> {
        use std::io::Write;
        use std::process::{Command, Stdio};
        let cmd = ron::paths::read_cli_viewer();
        let parts = ron::editor::split_cmd(&cmd);
        let Some((prog, args)) = parts.split_first() else {
            println!("{text}");
            return Ok(());
        };
        let child = Command::new(prog)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn();
        let mut child = match child {
            Ok(c) => c,
            Err(e) => {
                eprintln!("warning: could not spawn cli_viewer {prog:?} ({e}); printing raw note");
                println!("{text}");
                return Ok(());
            }
        };
        // Take stdin before awaiting so the child never blocks on a pipe we
        // still hold; the Option is dropped (closed) after writing.
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(text.as_bytes())?;
        }
        let status = child.wait()?;
        if !status.success() {
            return Err(anyhow!("cli_viewer {prog} exited with {status}"));
        }
        Ok(())
    }

    pub fn list(limit: Option<u32>) -> Result<()> {
        let notes = client::list_notes(limit)?;
        print_note_table(&notes);
        Ok(())
    }

    pub fn search(patterns: &[String], field: &str, ignore_case: bool, whole_word: bool) -> Result<()> {
        // Each pattern may carry a scope prefix (t:/g:/n:/a:). When any do,
        // run a separate search per pattern with its own field and intersect.
        let mut results: Option<Vec<Note>> = None;
        for p in patterns {
            let (fld, q) = split_scope(p);
            let f = if fld == "default" { field } else { fld };
            let hits = client::search_notes(q, f, ignore_case, whole_word)?;
            results = Some(match results {
                None => hits,
                Some(prev) => prev
                    .into_iter()
                    .filter(|n| hits.iter().any(|h| h.id == n.id))
                    .collect(),
            });
        }
        print_note_table(&results.unwrap_or_default());
        Ok(())
    }

    pub fn list_notebooks() -> Result<()> {
        for nb in client::list_notebooks()? {
            println!("{nb}");
        }
        Ok(())
    }

    pub fn relate(id: &str, to: Vec<String>) -> Result<()> {
        let note = client::get_note(id)?;
        let mut related = note.related.clone();
        for t in &to {
            if !related.contains(t) {
                related.push(t.clone());
            }
        }
        let updated = client::update_note(id, None, None, None, None, Some(related))?;
        println!("related on {}: {:?}", updated.id, updated.related);
        Ok(())
    }

    // ---- helpers ----

    struct ParsedNote {
        title: String,
        tags: Vec<String>,
        notebook: String,
        related: Vec<String>,
        body: String,
    }

    fn parse_editor_buffer(text: &str) -> Result<ParsedNote> {
        let mut lines = text.lines();
        let title_line = lines.next().ok_or_else(|| anyhow!("empty buffer"))?;
        let title = strip_field(title_line, "Title:").trim().to_string();
        let tags_line = lines.next().unwrap_or("");
        let tags_str = strip_field(tags_line, "Tags:").trim();
        let tags = split_semis(tags_str);
        let nb_line = lines.next().unwrap_or("");
        let notebook = strip_field(nb_line, "Notebook:").trim().to_string();
        // The optional "Related:" line, when present (edit sessions only).
        let related_line = lines.next().unwrap_or("");
        let related = if related_line.starts_with("Related:") {
            split_semis(strip_field(related_line, "Related:").trim())
        } else {
            Vec::new()
        };
        // Skip anything else before the divider.
        let mut body_lines: Vec<&str> = Vec::new();
        let mut saw_fence = false;
        for line in lines {
            if !saw_fence {
                if line.trim_start().starts_with("------") {
                    saw_fence = true;
                }
                continue;
            }
            body_lines.push(line);
        }
        if body_lines.first().map_or(false, |l| l.is_empty()) {
            body_lines.remove(0);
        }
        Ok(ParsedNote {
            title,
            tags,
            notebook,
            related,
            body: body_lines.join("\n"),
        })
    }

    fn split_semis(s: &str) -> Vec<String> {
        if s.is_empty() {
            Vec::new()
        } else {
            s.split(';').map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect()
        }
    }

    // ---- draft cache ----

    /// The newest usable draft for `key`, as an editor buffer. Consults the
    /// server first (canonical), drops watermarked local copies (already
    /// saved as a note elsewhere), pushes a newer local copy up
    /// (sync-on-touch), and falls back to the local copy offline.
    pub struct ResolvedDraft {
        pub buffer: String,
        pub updated: chrono::NaiveDateTime,
    }

    pub fn resolve_draft(key: &str) -> Option<ResolvedDraft> {
        let local_path = client::drafts_file().ok()?;
        let mut local = client::load_local_draft(&local_path, key);
        let info = match client::get_draft(key) {
            Ok(info) => info,
            Err(_) => {
                // Server unreachable: local copy is all we have.
                return local.map(|l| ResolvedDraft { buffer: l.content, updated: l.saved_at });
            }
        };

        // Watermark: a local copy at or below the consumed timestamp was
        // already saved as a note (possibly from another machine) — drop it.
        if let Some(wm) = info.consumed_updated {
            if local.as_ref().map_or(false, |l| l.saved_at <= wm) {
                let _ = client::drop_local_draft(&local_path, key);
                local = None;
            }
        }

        // Sync-on-touch: a local copy newer than the server's live draft
        // goes up so other devices can see it.
        let mut server_draft = info.draft;
        if let Some(l) = &local {
            let newer = server_draft.as_ref().map_or(true, |d| l.saved_at > d.updated);
            if newer {
                if let Ok(parsed) = parse_editor_buffer(&l.content) {
                    let content = DraftContent {
                        title: parsed.title,
                        tags: parsed.tags,
                        notebook: parsed.notebook,
                        related: parsed.related,
                        body: parsed.body,
                    };
                    if let Ok(pushed) = client::save_draft(key, &content) {
                        server_draft = Some(pushed);
                        local = None;
                    }
                }
            }
        }

        match (server_draft, local) {
            (Some(d), Some(l)) if l.saved_at > d.updated => {
                Some(ResolvedDraft { buffer: l.content, updated: l.saved_at })
            }
            (Some(d), _) => Some(ResolvedDraft {
                buffer: draft_to_buffer(&d.content),
                updated: d.updated,
            }),
            (None, Some(l)) => Some(ResolvedDraft { buffer: l.content, updated: l.saved_at }),
            (None, None) => None,
        }
    }

    /// Render structured draft content back into the editor-buffer format.
    pub fn draft_to_buffer(c: &DraftContent) -> String {
        format!(
            "Title: {}\nTags: {}\nNotebook: {}\nRelated: {}\n\n------\n\n{}",
            c.title,
            c.tags.join("; "),
            c.notebook,
            c.related.join("; "),
            c.body,
        )
    }

    /// Cache `buffer` as the draft for `key`: locally always (offline
    /// safety net), to the server best-effort (cross-device reach).
    /// Returns the local save timestamp.
    pub fn save_draft_everywhere(key: &str, buffer: &str) -> chrono::NaiveDateTime {
        let ts = client::drafts_file()
            .and_then(|p| client::store_local_draft(&p, key, buffer))
            .unwrap_or_else(|e| {
                eprintln!("warning: local draft cache failed: {e:#}");
                chrono::Local::now().naive_local()
            });
        if let Ok(parsed) = parse_editor_buffer(buffer) {
            let content = DraftContent {
                title: parsed.title,
                tags: parsed.tags,
                notebook: parsed.notebook,
                related: parsed.related,
                body: parsed.body,
            };
            if let Err(e) = client::save_draft(key, &content) {
                eprintln!("note: draft not pushed to the server ({e:#}); cached locally");
            }
        }
        ts
    }

    /// Drop this machine's local copy of the draft. The server side is
    /// consumed automatically by the successful note write.
    pub fn drop_local_draft_quietly(key: &str) {
        if let Ok(p) = client::drafts_file() {
            if let Err(e) = client::drop_local_draft(&p, key) {
                eprintln!("warning: dropping local draft failed: {e:#}");
            }
        }
    }

    /// Paste-ready command that resumes this draft: the `ron draft list`
    /// resume column and the recovery hints render it (backticked, via
    /// `recover_hint`).
    pub fn resume_command(key: &str) -> String {
        if key == "new" {
            "ron add".to_string()
        } else {
            format!("ron edit {}", key.strip_prefix("note:").unwrap_or(key))
        }
    }

    /// How to pick this draft back up, for user-facing hints.
    fn recover_hint(key: &str) -> String {
        format!("`{}`", resume_command(key))
    }

    /// Shared tail of `add`/`edit`: classify the editor outcome and either
    /// no-op (empty buffer / untouched fresh template), cache a draft
    /// (`:cq`-style exit or empty title), or submit through `save`. An
    /// untouched *draft* prefill that already has a title is submitted —
    /// quitting the editor means "yes, save it as a note". On submit
    /// failure the buffer is cached as a draft and the error gains a
    /// recovery hint.
    fn finish_edit_session(
        key: &str,
        outcome: &EditOutcome,
        initial: &str,
        from_draft: bool,
        save: impl FnOnce(ParsedNote) -> Result<String>,
    ) -> Result<()> {
        let text = outcome.text();
        if text.trim().is_empty() {
            return Ok(());
        }
        let unchanged = text.trim() == initial.trim();
        if unchanged {
            // Fresh template untouched: silent no-op.
            if !from_draft || matches!(outcome, EditOutcome::ExitedNonzero(_)) {
                return Ok(());
            }
            // Untouched draft prefill: committing it as-is is the natural
            // "recover and save" gesture — but only when it has a title.
            let parsed = parse_editor_buffer(text)?;
            if parsed.title.trim().is_empty() {
                return Ok(());
            }
            return submit_or_cache(key, text, parsed, save);
        }
        let parsed = parse_editor_buffer(text)?;
        let wants_draft =
            matches!(outcome, EditOutcome::ExitedNonzero(_)) || parsed.title.trim().is_empty();
        if wants_draft {
            let ts = save_draft_everywhere(key, text);
            println!(
                "draft saved ({key}) at {} — continue with {}, discard with `ron draft clear {key}`",
                ts.format("%Y-%m-%d %H:%M"),
                recover_hint(key),
            );
            return Ok(());
        }
        submit_or_cache(key, text, parsed, save)
    }

    fn submit_or_cache(
        key: &str,
        text: &str,
        parsed: ParsedNote,
        save: impl FnOnce(ParsedNote) -> Result<String>,
    ) -> Result<()> {
        match save(parsed) {
            Ok(msg) => {
                drop_local_draft_quietly(key);
                println!("{msg}");
                Ok(())
            }
            Err(e) => {
                let ts = save_draft_everywhere(key, text);
                Err(anyhow!(
                    "{e:#}\ndraft cached ({key}) at {} — rerun {} to recover it once the server is reachable",
                    ts.format("%Y-%m-%d %H:%M"),
                    recover_hint(key),
                ))
            }
        }
    }

    fn strip_field<'a>(line: &'a str, prefix: &str) -> &'a str {
        line.strip_prefix(prefix).unwrap_or(line)
    }

    fn split_scope(pattern: &str) -> (&'static str, &str) {
        let Some(rest) = pattern.split_once(':') else {
            return ("default", pattern);
        };
        match rest.0 {
            "t" => ("title", rest.1),
            "g" => ("tags", rest.1),
            "n" => ("notebook", rest.1),
            "a" => ("content", rest.1),
            _ => ("default", pattern),
        }
    }

    /// A "target" is either an ID (contains `-`) or a 1-based index of the
    /// last `list`/`search` result. P3 only resolves indices via a fresh
    /// `list_notes(50)` call; the old pickle cache is gone. A draft key
    /// (`note:<id>`, as printed by `ron draft list`) is accepted as its ID.
    fn resolve_target(target: &str) -> Result<String> {
        let target = target.strip_prefix("note:").unwrap_or(target);
        if target.contains('-') {
            return Ok(target.to_string());
        }
        let idx: usize = target
            .parse()
            .map_err(|_| anyhow!("invalid target {target:?}; pass a note ID or 1-based index"))?;
        if idx == 0 {
            return Err(anyhow!("indices are 1-based"));
        }
        let notes = client::list_notes(Some(50))?;
        notes
            .get(idx - 1)
            .map(|n| n.id.clone())
            .ok_or_else(|| anyhow!("no note at index {idx}"))
    }

    pub fn print_note_table(notes: &[Note]) {
        if notes.is_empty() {
            println!("(no notes)");
            return;
        }
        // Columns: idx, updated, ID, notebook, title, tags
        println!("{:>2}  {:<10}  {:<24}  {:<12}  {:<30}  {}", "#", "updated", "id", "notebook", "title", "tags");
        for (i, n) in notes.iter().enumerate() {
            let tags = n.tags.join(";");
            println!(
                "{:>2}  {:<10}  {:<24}  {:<12}  {:<30}  {}",
                i + 1,
                n.updated.format("%Y-%m-%d").to_string(),
                n.id,
                truncate(&n.notebook, 12),
                truncate(&n.title, 30),
                tags,
            );
        }
    }

    pub fn truncate(s: &str, n: usize) -> String {
        if s.chars().count() <= n {
            s.to_string()
        } else {
            let mut out: String = s.chars().take(n.saturating_sub(1)).collect();
            out.push('…');
            out
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn content() -> DraftContent {
            DraftContent {
                title: "Hello world".into(),
                tags: vec!["a".into(), "b".into()],
                notebook: "nb".into(),
                related: vec!["note-1".into(), "note-2".into()],
                body: "# heading\n\nsome text".into(),
            }
        }

        #[test]
        fn draft_buffer_round_trip() {
            let buffer = draft_to_buffer(&content());
            let parsed = parse_editor_buffer(&buffer).unwrap();
            assert_eq!(parsed.title, "Hello world");
            assert_eq!(parsed.tags, vec!["a".to_string(), "b".to_string()]);
            assert_eq!(parsed.notebook, "nb");
            assert_eq!(parsed.related, vec!["note-1".to_string(), "note-2".to_string()]);
            assert_eq!(parsed.body, "# heading\n\nsome text");
        }

        #[test]
        fn parser_handles_add_template_without_related_line() {
            // The `ron add` template has no Related: line.
            let parsed =
                parse_editor_buffer("Title: t\nTags: x\nNotebook: nb\n\n------\n\nbody").unwrap();
            assert!(parsed.related.is_empty());
            assert_eq!(parsed.body, "body");
        }

        #[test]
        fn empty_title_and_untouched_template_classify_as_noop_or_draft() {
            // Mirror the classification in finish_edit_session.
            let initial = "Title: \nTags: \nNotebook: default\n\n------\n\n";
            let unchanged = EditOutcome::Saved(initial.to_string());
            assert!(unchanged.text().trim() == initial.trim()); // noop branch

            let typed_no_title =
                EditOutcome::Saved("Title: \nTags: \nNotebook: default\n\n------\n\nthoughts".into());
            let parsed = parse_editor_buffer(typed_no_title.text()).unwrap();
            assert!(parsed.title.trim().is_empty()); // would take the draft branch
            assert!(!typed_no_title.text().trim().is_empty());
        }

        #[test]
        fn resolve_target_accepts_draft_key() {
            // `ron draft list` prints keys like `note:<id>`; pasting one into
            // edit/view/delete must resolve to the note ID, not 404.
            assert_eq!(resolve_target("note:note-1").unwrap(), "note-1");
            assert_eq!(resolve_target("note-1").unwrap(), "note-1");
        }

        #[test]
        fn resume_command_matches_key_shape() {
            assert_eq!(resume_command("new"), "ron add");
            assert_eq!(resume_command("note:note-1"), "ron edit note-1");
            // Recovery hints render the same command backticked.
            assert_eq!(recover_hint("note:note-1"), "`ron edit note-1`");
        }
    }
}

// ----- draft commands -----

mod drafts_cmd {
    use super::notes_cmd::{draft_to_buffer, resolve_draft, save_draft_everywhere};
    use super::*;
    use ron::client;

    pub fn edit(key: String) -> Result<()> {
        if !ron::models::valid_draft_key(&key) {
            return Err(anyhow!("invalid draft key {key:?}; use `new` or `note:<id>`"));
        }
        let initial = match resolve_draft(&key) {
            Some(d) => d.buffer,
            None => {
                let nb = client::server_default_notebook()
                    .unwrap_or_else(|_| ron::paths::read_default_notebook());
                draft_to_buffer(&ron::models::DraftContent {
                    notebook: nb,
                    ..Default::default()
                })
            }
        };
        let outcome = ron::editor::edit(&initial)?;
        let text = outcome.text();
        if text.trim().is_empty() || text.trim() == initial.trim() {
            println!("(no changes; draft untouched)");
            return Ok(());
        }
        // Any exit saves the draft here — that's the whole point of the
        // command; there is no note to create.
        let ts = save_draft_everywhere(&key, text);
        println!(
            "draft saved ({key}) at {} — resume with `ron add`/`ron edit`, discard with `ron draft clear {key}`",
            ts.format("%Y-%m-%d %H:%M"),
        );
        Ok(())
    }

    pub fn list() -> Result<()> {
        let server = client::list_drafts().unwrap_or_else(|e| {
            eprintln!("(server unreachable: {e:#}; showing local drafts only)");
            Vec::new()
        });
        let local_path = client::drafts_file()?;
        let local = client::load_all_local_drafts(&local_path);
        let mut keys: Vec<String> = server.iter().map(|d| d.key.clone()).collect();
        for k in local.keys() {
            if !keys.contains(k) {
                keys.push(k.clone());
            }
        }
        keys.sort();
        if keys.is_empty() {
            println!("(no drafts)");
            return Ok(());
        }
        println!(
            "{:<28}  {:<19}  {:<13}  {:<24}  {}",
            "key", "saved", "where", "title", "resume"
        );
        for k in &keys {
            let sd = server.iter().find(|d| &d.key == k);
            let ld = local.get(k);
            let Some((ts, whr, title)) = sd.map(|d| (d.updated, "server", d.content.title.clone())).or_else(|| {
                ld.map(|l| (l.saved_at, "local", title_of_buffer(&l.content)))
            }) else {
                continue;
            };
            let whr = if sd.is_some() && ld.is_some() { "server+local" } else { whr };
            println!(
                "{:<28}  {:<19}  {:<13}  {:<24}  {}",
                k,
                ts.format("%Y-%m-%d %H:%M:%S"),
                whr,
                notes_cmd::truncate(&title, 24),
                notes_cmd::resume_command(k),
            );
        }
        Ok(())
    }

    fn title_of_buffer(buf: &str) -> String {
        buf.lines()
            .next()
            .unwrap_or("")
            .strip_prefix("Title: ")
            .unwrap_or("")
            .trim()
            .to_string()
    }

    pub fn clear(key: Option<String>) -> Result<()> {
        let local_path = client::drafts_file()?;
        match &key {
            Some(k) => {
                if !ron::models::valid_draft_key(k) {
                    return Err(anyhow!("invalid draft key {k:?}; use `new` or `note:<id>`"));
                }
                let removed_local = client::drop_local_draft(&local_path, k)?;
                let removed_server = client::delete_draft(k).is_ok();
                if removed_local || removed_server {
                    println!("cleared {k}");
                } else {
                    println!("(no draft for {k})");
                }
            }
            None => {
                client::clear_local_drafts(&local_path)?;
                let live = client::list_drafts().unwrap_or_default();
                for d in live {
                    let _ = client::delete_draft(&d.key);
                }
                println!("cleared all drafts");
            }
        }
        Ok(())
    }
}

// ----- pulse commands -----

mod pulses_cmd {
    use super::*;
    use ron::client;
    use ron::models::Pulse;

    pub fn add(sub: &clap::ArgMatches) -> Result<()> {
        let topic = sub.get_one::<String>("topic").unwrap().clone();
        let interval = sub.get_one::<String>("interval").unwrap().clone();
        let pulse = client::create_pulse(&topic, &interval)?;
        println!("created {} ({})", pulse.id, pulse.interval);
        Ok(())
    }

    pub fn set_check(sub: &clap::ArgMatches, checked: bool) -> Result<()> {
        let id = sub.get_one::<String>("id").unwrap().clone();
        let on = sub.get_one::<String>("on").map(|s| s.as_str());
        let pulse = client::set_pulse_slot(&id, on, checked)?;
        let current = pulse
            .interval
            .current_slot(chrono::Local::now().naive_local());
        let state = pulse.get_slot(&current).unwrap_or(false);
        println!("{}: {} slot {} = {}", pulse.id, pulse.topic, current, if state { "✓" } else { "✗" });
        Ok(())
    }

    pub fn list(sub: &clap::ArgMatches) -> Result<()> {
        let active = *sub.get_one::<bool>("active").unwrap_or(&false);
        let pulses = client::list_pulses(active)?;
        if pulses.is_empty() {
            println!("(no pulses)");
            return Ok(());
        }
        println!("{:<24}  {:<8}  {:<6}  {}", "id", "interval", "today", "topic");
        for p in &pulses {
            let today = p.interval.current_slot(chrono::Local::now().naive_local());
            let state = if p.get_slot(&today).unwrap_or(false) { "✓" } else { " " };
            println!("{:<24}  {:<8}  {:<6}  {}", p.id, p.interval, state, p.topic);
        }
        Ok(())
    }

    pub fn delete(sub: &clap::ArgMatches) -> Result<()> {
        let id = sub.get_one::<String>("id").unwrap().clone();
        client::delete_pulse(&id)?;
        println!("deleted {id}");
        Ok(())
    }

    pub fn edit(sub: &clap::ArgMatches) -> Result<()> {
        let id = sub.get_one::<String>("id").unwrap().clone();
        let topic = sub.get_one::<String>("topic").cloned();
        let interval = sub.get_one::<String>("interval").cloned();
        if topic.is_none() && interval.is_none() {
            return Err(anyhow!(
                "pedit needs at least one of --topic or --interval (nothing to change)"
            ));
        }
        let pulse = client::update_pulse(&id, topic, interval)?;
        println!("updated {} ({}: {})", pulse.id, pulse.interval, pulse.topic);
        Ok(())
    }

    #[allow(dead_code)]
    fn _unused(_p: Pulse) {}
}

// ----- metric commands -----

mod metrics_cmd {
    use super::*;
    use ron::client;

    pub fn add(sub: &clap::ArgMatches) -> Result<()> {
        let topic = sub.get_one::<String>("topic").unwrap().clone();
        let metric = client::create_metric(&topic)?;
        println!("created {}", metric.id);
        Ok(())
    }

    pub fn log(sub: &clap::ArgMatches) -> Result<()> {
        let id = sub.get_one::<String>("id").unwrap().clone();
        let value: f64 = sub
            .get_one::<String>("value")
            .unwrap()
            .parse()
            .map_err(|_| anyhow!("value must be a number"))?;
        let ts = sub.get_one::<String>("ts").map(|s| s.as_str());
        let metric = client::append_metric_point(&id, value, ts)?;
        println!("appended to {} ({} points total)", metric.id, metric.points.len());
        Ok(())
    }

    pub fn stats(sub: &clap::ArgMatches) -> Result<()> {
        let id = sub.get_one::<String>("id").unwrap().clone();
        let from = sub.get_one::<String>("from").map(|s| s.as_str());
        let to = sub.get_one::<String>("to").map(|s| s.as_str());
        let s = client::metric_stats(&id, from, to)?;
        println!("{} ({})", id, s.topic);
        println!("  count:  {}", s.count);
        println!("  mean:   {:.3}", s.mean);
        println!("  median: {:.3}", s.median);
        println!("  min:    {:.3}", s.min);
        println!("  max:    {:.3}", s.max);
        Ok(())
    }

    pub fn list() -> Result<()> {
        let metrics = client::list_metrics()?;
        if metrics.is_empty() {
            println!("(no metrics)");
            return Ok(());
        }
        println!("{:<24}  {:<10}  {}", "id", "points", "topic");
        for m in metrics {
            println!("{:<24}  {:<10}  {}", m.id, m.points.len(), m.topic);
        }
        Ok(())
    }

    pub fn delete(sub: &clap::ArgMatches) -> Result<()> {
        let id = sub.get_one::<String>("id").unwrap().clone();
        client::delete_metric(&id)?;
        println!("deleted {id}");
        Ok(())
    }

    pub fn edit(sub: &clap::ArgMatches) -> Result<()> {
        let id = sub.get_one::<String>("id").unwrap().clone();
        let topic = sub.get_one::<String>("topic").cloned();
        if topic.is_none() {
            return Err(anyhow!("medit needs --topic (nothing to change)"));
        }
        let metric = client::update_metric(&id, topic)?;
        println!("updated {} ({})", metric.id, metric.topic);
        Ok(())
    }
}

// ----- admin commands -----

mod admin_cmd {
    use super::*;
    use ron::client;

    pub fn export() -> Result<()> {
        let r = client::export()?;
        println!(
            "exported notes={} pulses={} metrics={} (committed={})",
            r.notes, r.pulses, r.metrics, r.committed
        );
        Ok(())
    }

    pub fn import() -> Result<()> {
        let r = client::import()?;
        println!("imported {} items", r.items);
        Ok(())
    }

    pub fn backup(dry_run: bool) -> Result<()> {
        let r = client::backup(dry_run)?;
        if let Some(st) = &r.status {
            print_backup_status(st);
        } else {
            println!("pushed");
        }
        Ok(())
    }

    /// Human rendering of the `--dry-run` status report, with hints on
    /// what to do next (backup / sync / manual divergence recovery).
    fn print_backup_status(st: &client::BackupStatus) {
        let Some(url) = &st.remote_url else {
            println!("no remote configured; add one to enable backup/sync:");
            println!("  git -C ~/.local/share/ron/repo remote add origin <url>");
            return;
        };
        println!("remote: {url} (origin/master)");
        if st.fetched {
            println!("fetch: ok");
        } else {
            println!("fetch: failed (counts may be stale)");
        }
        if st.dirty {
            println!("warning: working tree dirty; run `ron export` to commit the whole tree");
        }
        println!(
            "ahead {}, behind {}",
            st.ahead, st.behind
        );
        if !st.to_push.is_empty() {
            println!("to push:");
            for c in &st.to_push {
                println!("  {} {}", c.hash, c.subject);
            }
        }
        if !st.to_pull.is_empty() {
            println!("to pull:");
            for c in &st.to_pull {
                println!("  {} {}", c.hash, c.subject);
            }
        }
        if st.ahead > 0 && st.behind > 0 {
            println!(
                "warning: local and origin/master have diverged (ahead {}, behind {})",
                st.ahead, st.behind
            );
            println!("  `ron sync` will fail (--ff-only); resolve manually, then reload:");
            println!("    1. git -C ~/.local/share/ron/repo pull --rebase origin master");
            println!("       (resolve YAML conflicts: git add <file> && git rebase --continue)");
            println!("    2. ron import     # rebuild SQLite from the reconciled YAML");
            println!("    3. ron backup     # push the reconciled history");
        } else if st.behind > 0 {
            println!("hint: run `ron sync` to pull");
        } else if st.ahead > 0 {
            println!("hint: run `ron backup` to push");
        } else {
            println!("up to date");
        }
    }

    pub fn sync() -> Result<()> {
        let r = client::sync()?;
        if r.changed_files.is_empty() {
            println!("up to date");
        } else {
            println!("changed files:");
            for f in &r.changed_files {
                println!("  {f}");
            }
        }
        println!("loaded {} items", r.items_loaded);
        Ok(())
    }

    /// Print the viewer passphrase (if any) from `~/.config/ron/server.json`.
    /// Local file read only — no server contact, no token required.
    pub fn viewer_key() -> Result<()> {
        let paths = ron::Paths::detect()?;
        let cfg = ron::ServerConfig::load(&paths)?;
        match &cfg.viewer_secret {
            Some(secret) => {
                println!("{secret}");
                println!(
                    "(from {})",
                    paths.server_config.display()
                );
                Ok(())
            }
            None => {
                eprintln!(
                    "no viewer_secret set in {}",
                    paths.server_config.display()
                );
                eprintln!("add \"viewer_secret\": \"<passphrase>\" to enable the phone gate");
                std::process::exit(1);
            }
        }
    }
}
