mod config;

use std::fs;
use std::path::{Path, PathBuf, };
use std::process::Command as SysCmd;
use clap::ArgMatches;
use chrono::{DateTime, Local};
use config::{load_configs, };

const TEMP_NOTE: &str = "/tmp/dsnote-tmp.md";

struct Note {
    title: String,
    tags: Vec<String>,
    notebook: String,
    created: DateTime<Local>,
    updated: DateTime<Local>,
}

fn parse_note(inp: &Path) -> Note {
    let mut raw = fs::read_to_string(inp).expect("Unable to read file").lines();
    let Some(title) = raw.next();
    let tags = raw.next().unwrap().split(";").collect();
    let Some(notebook) = raw.next();
    let Some(created) = raw.next();
    let Some(updated) = raw.next();
}

fn save_note(content: String, path: PathBuf) {

}
pub fn run(args: ArgMatches) {
    let confs = load_configs();
    println!("{:?}", confs);
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
            let note = parse_note(Path::new(TEMP_NOTE));

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
