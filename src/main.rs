//! ron CLI.
//!
//! Subcommand groups:
//!   - `serve`                            run the HTTP server
//!   - `migrate <src> <dst>`              1.x -> 2.x YAML migration (P1)
//!   - `token grant|list|revoke`          bearer-token management
//!   - Notes:   add / edit / delete / view / list / search / relate
//!   - Pulses:  padd / pcheck / puncheck / plist / pdel
//!   - Metrics: madd / mlog / mstats / mlist / mdel

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
        Some(("pdel", sub)) => pulses_cmd::delete(sub),
        Some(("madd", sub)) => metrics_cmd::add(sub),
        Some(("mlog", sub)) => metrics_cmd::log(sub),
        Some(("mstats", sub)) => metrics_cmd::stats(sub),
        Some(("mlist", _)) => metrics_cmd::list(),
        Some(("mdel", sub)) => metrics_cmd::delete(sub),
        Some(("export", _)) => admin_cmd::export(),
        Some(("import", _)) => admin_cmd::import(),
        Some(("backup", _)) => admin_cmd::backup(),
        Some(("sync", _)) => admin_cmd::sync(),
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
                .arg(Arg::new("dst").required(true)),
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
        // ---- admin ----
        .subcommand(
            Command::new("export")
                .about("rewrite all YAML files from the DB; git add+commit"),
        )
        .subcommand(
            Command::new("import").about("reload the DB from the YAML files on disk"),
        )
        .subcommand(
            Command::new("backup").about("git push origin master (the repo must have a remote)"),
        )
        .subcommand(
            Command::new("sync")
                .about("git pull --ff-only origin master, then reload DB from YAML"),
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
    let report = ron::migrate::migrate_dir(&src, &dst);
    if let Some(fatal) = report.fatal {
        eprintln!("fatal: {fatal}");
        std::process::exit(1);
    }
    println!("migrated {} note(s)", report.succeeded.len());
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
    use ron::models::Note;

    pub fn add() -> Result<()> {
        let body = ron::editor::edit("Title: \nTags: \nNotebook: default\n\n------\n\n")?;
        let parsed = parse_editor_buffer(&body)?;
        let note = client::create_note(&parsed.title, parsed.tags, &parsed.notebook, &parsed.body)?;
        println!("created {}", note.id);
        Ok(())
    }

    pub fn edit(target: String) -> Result<()> {
        let id = resolve_target(&target)?;
        let note = client::get_note(&id)?;
        let initial = format!(
            "Title: {}\nTags: {}\nNotebook: {}\nRelated: {}\n\n------\n\n{}",
            note.title,
            note.tags.join("; "),
            note.notebook,
            note.related.join("; "),
            note.body,
        );
        let new_text = ron::editor::edit(&initial)?;
        let parsed = parse_editor_buffer(&new_text)?;
        let updated = client::update_note(
            &note.id,
            Some(parsed.title),
            Some(parsed.tags),
            Some(parsed.notebook),
            Some(parsed.body),
            Some(note.related),
        )?;
        println!("updated {}", updated.id);
        Ok(())
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
        println!("Title: {}", note.title);
        println!("Tags: {}", note.tags.join("; "));
        println!("Notebook: {}", note.notebook);
        println!("Related: {}", note.related.join("; "));
        println!("Created: {}", note.created.format("%F %T"));
        println!("Updated: {}", note.updated.format("%F %T"));
        println!("ID: {}", note.id);
        println!();
        println!("{}", note.body);
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
        body: String,
    }

    fn parse_editor_buffer(text: &str) -> Result<ParsedNote> {
        let mut lines = text.lines();
        let title_line = lines.next().ok_or_else(|| anyhow!("empty buffer"))?;
        let title = strip_field(title_line, "Title:").trim().to_string();
        let tags_line = lines.next().unwrap_or("");
        let tags_str = strip_field(tags_line, "Tags:").trim();
        let tags = if tags_str.is_empty() {
            Vec::new()
        } else {
            tags_str.split(";").map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
        };
        let nb_line = lines.next().unwrap_or("");
        let notebook = strip_field(nb_line, "Notebook:").trim().to_string();
        // Skip the optional "Related:" line if present, and the divider.
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
            body: body_lines.join("\n"),
        })
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
    /// `list_notes(50)` call; the old pickle cache is gone.
    fn resolve_target(target: &str) -> Result<String> {
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

    fn truncate(s: &str, n: usize) -> String {
        if s.chars().count() <= n {
            s.to_string()
        } else {
            let mut out: String = s.chars().take(n.saturating_sub(1)).collect();
            out.push('…');
            out
        }
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

    pub fn backup() -> Result<()> {
        client::backup()?;
        println!("pushed");
        Ok(())
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
}
