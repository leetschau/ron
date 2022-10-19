mod config;

use std::{fmt, fs};
use std::path::{PathBuf, };
use std::process::Command as SysCmd;
use clap::ArgMatches;
use chrono::{Local, NaiveDateTime};
use glob::glob;
use config::{load_configs, };
use serde::{Serialize, Deserialize};
use crate::config::Config;
use std::collections::BTreeMap;

const TEMP_NOTE: &str = "/tmp/dsnote-tmp.md";
const CACHE_FILE: &str = ".notes-cache";

#[derive(Debug, Clone, Serialize, Deserialize, )]
struct Note {
    title: String,
    tags: Vec<String>,
    notebook: String,
    created: NaiveDateTime,
    updated: NaiveDateTime,
    body: String,
    filepath: PathBuf,
}

impl fmt::Display for Note {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let updated = self.updated.format("%Y-%m-%d");
        let created = self.created.format("%Y-%m-%d");
        let tagstr = self.tags.join("; ");
        write!(f, "{} {}: {} {} [{}]",
               updated,
               self.notebook,
               self.title,
               created,
               tagstr)
     }
}

fn parse_note(inp: PathBuf) -> Note {
    let raw = fs::read_to_string(&inp).expect("Reading file failed");
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

    Note { title, tags, notebook, created, updated, body, filepath: inp}
}

/// Save a note to file specified by 'path' in text format
fn save_note(note: Note, path: &PathBuf) {
    let tagstr = note.tags.join("; ");
    let content = format!(
        "Title: {}\nTags: {}\nNotebook: {}\nCreated: {}\nUpdated: {}\n------\n{}",
        note.title, tagstr, note.notebook, note.created, note.updated, note.body);
    fs::write(path, content)
        .unwrap_or_else(|_| panic!("Writing note file {} failed", path.display()));
}

/// Load all markdown file from the 'repo_path', sort with updated time, and take first num elements
fn most_recent_notes(repo_path: &PathBuf, num: u16) -> Vec<Note> {
    let files = glob(repo_path.join("*.md").to_str().unwrap()).unwrap();
    let mut notes = Vec::new();
    for item in files {
        match item {
            Ok(path) => { let note = parse_note(path); notes.push(note) },
            Err(e) => println!("{:?}", e),
        }
    }
    notes.sort_by(|a, b| b.updated.cmp(&a.updated));
    notes[..num as usize].to_vec()
}

/// Save 'notes' to disk and display them to console
fn save_display(notes: Vec<Note>, conf: Config) {
    let mut notes_dict: BTreeMap<u16, &Note> = BTreeMap::new();
    let mut index: u16 = 0;
    for note in &notes {
        index += 1;
        notes_dict.insert(index, note);
    }
    let serialized_notes = serde_pickle::to_vec(&notes_dict, Default::default()).unwrap();
    fs::write(conf.app_home.join(CACHE_FILE),
              serialized_notes).unwrap();

    println!("No.   Updated, Notebook, Title, Created, Tags");
    for (index, note) in &notes_dict {
        println!("{:2}. {}", index, note);
    }
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
            let new_note = parse_note(PathBuf::from(TEMP_NOTE));
            let timestamp = Local::now().format("%y%m%d%H%M%S");
            let note_rel_path = format!("repo/note{timestamp}.md");
            save_note(new_note, &confs.app_home.join(note_rel_path));
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
            let num = args.get_one::<u16>("number").unwrap();
            let confs = load_configs();
            let notes = most_recent_notes(
                &confs.app_home.join("repo"),
                *num);
            save_display(notes, confs)
        },
        Some(("search", args)) => {
            let ptns: Vec<String>  = args.get_many("patterns").unwrap().cloned().collect();
            println!("Patterns are: {:?}", ptns)
        },
        _ => unreachable!("Exhausted list of subcommands and subcommand_required prevents `None`"),
    }
}
