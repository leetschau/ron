//! ron CLI (Phase 1 build).
//!
//! Only `migrate` is wired up here. The full client CLI (list, search, add,
//! ...) lands in Phase 3 once the HTTP server (Phase 2) exists.

use std::path::PathBuf;

use clap::{command, ArgMatches, Command};

fn parse_args() -> ArgMatches {
    command!()
        .propagate_version(true)
        .subcommand_required(true)
        .subcommand(
            Command::new("migrate")
                .about("migrate notes from the 1.x markdown format to 2.x YAML")
                .arg(clap::Arg::new("src").required(true).help("1.x notes directory (.md files)"))
                .arg(clap::Arg::new("dst").required(true).help("2.x output directory (.yaml files)")),
        )
        .get_matches()
}

fn main() -> anyhow::Result<()> {
    let args = parse_args();
    match args.subcommand() {
        Some(("migrate", sub)) => {
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
        _ => unreachable!("subcommand_required prevents None"),
    }
}
