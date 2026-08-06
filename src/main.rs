//! ron CLI.
//!
//! Subcommands:
//!   - `migrate` (P1): one-shot 1.x -> 2.x YAML migration
//!   - `serve` (P2): run the HTTP server
//!   - `token grant|revoke|list` (P2): manage bearer tokens via the server
//!   - others (P3): notes/pulses/metrics client commands

use std::path::PathBuf;
use std::time::Duration;

use clap::{command, ArgMatches, Command};
use reqwest::blocking::Client;

fn parse_args() -> ArgMatches {
    command!()
        .propagate_version(true)
        .subcommand_required(true)
        .subcommand(
            Command::new("serve")
                .about("run the ron HTTP server (binds localhost)"),
        )
        .subcommand(
            Command::new("migrate")
                .about("migrate notes from the 1.x markdown format to 2.x YAML")
                .arg(clap::Arg::new("src").required(true).help("1.x notes directory (.md files)"))
                .arg(clap::Arg::new("dst").required(true).help("2.x output directory (.yaml files)")),
        )
        .subcommand(
            Command::new("token")
                .about("manage bearer tokens (talks to a running server)")
                .subcommand_required(true)
                .subcommand(
                    Command::new("grant")
                        .about("mint a new bearer token; prints the secret once")
                        .arg(clap::Arg::new("label").default_value("cli")),
                )
                .subcommand(Command::new("list").about("list token ids (no secrets)"))
                .subcommand(
                    Command::new("revoke")
                        .about("revoke a token by id")
                        .arg(clap::Arg::new("id").required(true)),
                ),
        )
        .get_matches()
}

fn main() -> anyhow::Result<()> {
    let args = parse_args();
    match args.subcommand() {
        Some(("migrate", sub)) => run_migrate(sub),
        Some(("serve", _)) => run_serve(),
        Some(("token", sub)) => run_token(sub),
        _ => unreachable!("subcommand_required prevents None"),
    }
}

fn run_migrate(sub: &ArgMatches) -> anyhow::Result<()> {
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

fn run_serve() -> anyhow::Result<()> {
    let paths = ron::Paths::detect()?;
    let cfg = ron::ServerConfig::load(&paths)?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(ron::server::app::run(paths, cfg))?;
    Ok(())
}

fn run_token(sub: &ArgMatches) -> anyhow::Result<()> {
    let base_url = std::env::var("RON_URL").unwrap_or_else(|_| "http://127.0.0.1:7780".to_string());
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;
    let url = format!("{base_url}/api/tokens");
    match sub.subcommand() {
        Some(("grant", m)) => {
            let label = m.get_one::<String>("label").unwrap();
            let resp: serde_json::Value = client
                .post(&url)
                .json(&serde_json::json!({ "label": label }))
                .send()?
                .error_for_status()?
                .json()?;
            println!("id:     {}", resp["id"]);
            println!("label:  {}", resp["label"]);
            println!("secret: {}", resp["secret"]);
            println!();
            println!("Use this as the bearer token. Save it now — it won't be shown again.");
        }
        Some(("list", _)) => {
            let resp: serde_json::Value = client.get(&url).send()?.error_for_status()?.json()?;
            for t in resp.as_array().unwrap_or(&vec![]) {
                println!("{}\t{}\t{}", t["id"], t["label"], t["created"]);
            }
        }
        Some(("revoke", m)) => {
            let id = m.get_one::<String>("id").unwrap();
            client.delete(format!("{url}/{id}")).send()?.error_for_status()?;
            println!("revoked {id}");
        }
        _ => unreachable!("subcommand_required prevents None"),
    }
    Ok(())
}
