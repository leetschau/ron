mod config;

use std::fs;
use std::path::{Path, PathBuf, };
use std::process::Command as SysCmd;
use clap::ArgMatches;
use chrono::{Local, NaiveDateTime};
use config::{load_configs, };

const TEMP_NOTE: &str = "/tmp/dsnote-tmp.md";

#[derive(Debug)]
struct Note {
    title: String,
    tags: Vec<String>,
    notebook: String,
    created: NaiveDateTime,
    updated: NaiveDateTime,
    body: String,
}

fn parse_note(inp: &Path) -> Note {
    let raw = fs::read_to_string(inp).expect("Reading file failed");
    let mut lines = raw.lines();

    let titleline = String::from(lines.next().unwrap());
    let title = String::from(&titleline[7..]);

    let tagline = String::from(lines.next().unwrap());
    let tagstr =&tagline[6..];
    let tags = tagstr.split("; ").map(str::to_string).collect();

    let nbline = String::from(lines.next().unwrap());
    let notebook = String::from(&nbline[10..]);

    let crline = String::from(lines.next().unwrap());
    let crstr = &crline[9..];
    let created = NaiveDateTime::parse_from_str(
        crstr, "%Y-%m-%d %H:%M:%S").unwrap();

    let upline = String::from(lines.next().unwrap());
    let upstr = &upline[9..];
    let updated = NaiveDateTime::parse_from_str(
        upstr, "%Y-%m-%d %H:%M:%S").unwrap();

    let body = lines.skip(3).collect::<Vec<&str>>().join("\n");

    Note { title, tags, notebook, created, updated, body, }
}

fn save_note(note: Note, path: PathBuf) {

}

pub fn run(args: ArgMatches) {
    let confs = load_configs();
    match args.subcommand() {
        Some(("add", _)) => {
            let now = Local::now().format("%F %T");
            let note_header: String = format!( "Title: \nTags: \nNotebook: {}\nCreated: {}\nUpdated: {}\n\n------\n\n",
                confs.default_notebook, now, now);
            fs::write(TEMP_NOTE, note_header).expect("Write note header failed!");
            SysCmd::new(confs.editor)
                .arg(TEMP_NOTE)
                .spawn()
                .expect("nvim met an error")
                .wait()
                .expect("Error: Editor returned a non-zero status");
            let new_note = parse_note(Path::new(TEMP_NOTE));
            let timestamp = Local::now().format("%y%m%d%H%M%S");
            let note_rel_path = format!("repo/note{timestamp}.md");
            save_note(new_note, confs.app_home.join(note_rel_path));
        },
        Some(("delete", args)) => {
            let idx = args.get_one::<u16>("index").unwrap();
            println!("delete note: #{}", idx)
        },
        Some(("edit", args)) => {
            let idx = args.get_one::<u16>("index").unwrap();
            println!("edit note: #{}", idx)
        },
        Some(("list", args)) => {
            let num = args.get_one::<u8>("number").unwrap();
            println!("list {} most recent notes", num)
        },
        Some(("search", args)) => {
            let ptns: Vec<String>  = args.get_many("patterns").unwrap().cloned().collect();
            println!("Patterns are: {:?}", ptns)
        },
        _ => unreachable!("Exhausted list of subcommands and subcommand_required prevents `None`"),
    }

}
