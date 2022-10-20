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
        let tagstr: String = self.tags.join("; ");
        write!(f, "{} {}: {} {} [{}]",
               updated,
               self.notebook,
               self.title,
               created,
               tagstr)
     }
}

impl Note {
    fn matches(&self, pattern: &str, key: &str, ignore_case: bool, match_whole_word: bool, before: bool) -> bool {
        match key {
            "ti" => {
                let mut target = self.title.clone();
                if ignore_case {
                    target = target.to_lowercase();
                }
                if match_whole_word {
                    let targets: Vec<&str> = target.split_whitespace().collect();
                    targets.contains(&pattern)
                } else {
                    target.contains(pattern)
                }
            },
            "ta" => {
                let mut tagstr: String = self.tags.join("; ");
                if ignore_case {
                    tagstr = tagstr.to_lowercase();
                }
                if match_whole_word {
                    let targets: Vec<&str> = tagstr.split("; ").collect();
                    targets.contains(&pattern)
                } else {
                    tagstr.contains(pattern)
                }
            },
            "nb" => {
                let mut target = self.notebook.clone();
                if ignore_case {
                    target = target.to_lowercase();
                }
                if match_whole_word {
                    let targets: Vec<&str> = target.split_whitespace().collect();
                    targets.contains(&pattern)
                } else {
                    target.contains(pattern)
                }
            },
            "cr" => {
                let timestamp = self.created.format("%F %T").to_string();
                timestamp.contains(pattern)
            },
            "up" => {
                let timestamp = self.updated.format("%F %T").to_string();
                timestamp.contains(pattern)
            },
            "all" => {
                let mut content: String = format!("{}\n{}\n{}\n{}\n{}\n{}",
                    self.title,
                    self.tags.join("; "),
                    self.notebook,
                    self.created.format("%F %T"),
                    self.updated.format("%F %T"),
                    self.body);
                if ignore_case {
                    content = content.to_lowercase();
                }
                if match_whole_word {
                    let targets: Vec<&str> = content.split_whitespace().collect();
                    targets.contains(&pattern)
                } else {
                    content.contains(pattern)
                }
            },
            _ => false,
        }
    }
}

fn parse_note(inp: PathBuf) -> Note {
    let raw: String = fs::read_to_string(&inp).expect("Reading file failed");
    let mut lines = raw.lines();

    let titleline: String = String::from(lines.next().unwrap());
    let title: String = String::from(&titleline[7..]);

    let tagline: String = String::from(lines.next().unwrap());
    let tagstr: &str =&tagline[6..];
    let tags: Vec<String> = tagstr.split("; ").map(str::to_string).collect();

    let nbline: String = String::from(lines.next().unwrap());
    let notebook: String = String::from(&nbline[10..]);

    let crline: String = String::from(lines.next().unwrap());
    let crstr: &str = &crline[9..];
    let created: NaiveDateTime = NaiveDateTime::parse_from_str(
        crstr, "%Y-%m-%d %H:%M:%S").unwrap();

    let upline: String = String::from(lines.next().unwrap());
    let upstr: &str = &upline[9..];
    let updated: NaiveDateTime = NaiveDateTime::parse_from_str(
        upstr, "%Y-%m-%d %H:%M:%S").unwrap();

    let body: String = lines.skip(3).collect::<Vec<&str>>().join("\n");

    Note { title, tags, notebook, created, updated, body, filepath: inp}
}

/// Save a note to file specified by 'path' in text format
fn save_note(note: Note, path: &PathBuf) {
    let tagstr: String = note.tags.join("; ");
    let content: String = format!(
        "Title: {}\nTags: {}\nNotebook: {}\nCreated: {}\nUpdated: {}\n\n------\n\n{}",
        note.title,
        tagstr,
        note.notebook,
        note.created.format("%F %T"),
        note.updated.format("%F %T"),
        note.body);
    fs::write(path, content)
        .unwrap_or_else(|_| panic!("Writing note file {} failed", path.display()));
}

/// Load all markdown file from the 'repo_path', sort with updated time, and take first num elements
fn load_notes(repo_path: &PathBuf) -> Vec<Note> {
    let files = glob(repo_path.join("*.md").to_str().unwrap()).unwrap();
    let mut notes: Vec<Note> = Vec::new();
    for item in files {
        match item {
            Ok(path) => { let note = parse_note(path); notes.push(note) },
            Err(e) => println!("{:?}", e),
        }
    }
    notes.sort_by(|a, b| b.updated.cmp(&a.updated));
    notes
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
    let confs: Config = load_configs();
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
            let idx: &u16 = args.get_one::<u16>("index").unwrap();
            let cache: Vec<u8> = fs::read(confs.app_home.join(CACHE_FILE))
                .expect("Unable to read cache file, run `l` or `s` command to fix this.");
            let notes_dict: BTreeMap<u16, Note> = serde_pickle::from_slice(
                &cache, Default::default()).unwrap();
            let target_path: &PathBuf = &notes_dict[idx].filepath;
            fs::remove_file(target_path).unwrap();
            println!("Note #{} deleted", idx)
        },
        Some(("edit", args)) => {
            let idx: &u16 = args.get_one::<u16>("index").unwrap();
            let cache: Vec<u8> = fs::read(confs.app_home.join(CACHE_FILE))
                .expect("Unable to read cache file, run `l` or `s` command to fix this.");
            let notes_dict: BTreeMap<u16, Note> = serde_pickle::from_slice(
                &cache, Default::default()).unwrap();
            let target_path: &str = &notes_dict[idx].filepath.to_str().unwrap();
            let old_note: Note = parse_note(PathBuf::from(target_path));
            let new_note = Note {
                updated: Local::now().naive_local(),
                ..old_note
            };
            save_note(new_note, &PathBuf::from(target_path));
            SysCmd::new(confs.editor)
                .arg(target_path)
                .spawn()
                .expect("nvim met an error")
                .wait()
                .expect("Error: Editor returned a non-zero status");
        },
        Some(("list", args)) => {
            let num: &u16 = args.get_one::<u16>("number").unwrap();
            let confs: Config = load_configs();
            let all_notes: Vec<Note> = load_notes(&confs.app_home.join("repo"));
            let recent_notes = all_notes[.. *num as usize].to_vec();
            save_display(recent_notes, confs)
        },
        Some(("search", args)) => {
            let ptns: Vec<String>  = args.get_many("patterns").unwrap().cloned().collect();
            let all_notes: Vec<Note> = load_notes(&confs.app_home.join("repo"));
            let mut matched: Vec<Note> = Vec::new();
            for note in all_notes {
                let mut all_match: bool = true;
                for ptn in &ptns {
                    if ! (note.matches(&ptn, "all", true, false, true)) {
                        all_match = false;
                        break
                    }
                }
                if all_match { matched.push(note); }
            }
            save_display(matched, confs);
        },
        _ => unreachable!("Exhausted list of subcommands and subcommand_required prevents `None`"),
    }
}
