mod config;

use std::{fmt, fs};
use std::path::PathBuf;
use std::process::Command as SysCmd;
use clap::ArgMatches;
use chrono::{Local, NaiveDateTime};
use glob::glob;
use config::{load_configs, print_config, set_config};
use serde::{Serialize, Deserialize};
use crate::config::Config;
use std::collections::{BTreeMap, BTreeSet};

const TEMP_NOTE: &str = "/tmp/dsnote-tmp.md";
const PATCH_PREFIX: &str = "/tmp/donno-patch";
const PATCH_EXT: &str = "tgz";
const CACHE_FILE: &str = ".notes-cache";
const REPO_DIR: &str = "repo";
const NOTE_PREFIX: &str = "repo/note";

enum SearchItem {
    Title(String),
    Tag(String),
    Notebook(String),
    Created(NaiveDateTime),
    Updated(NaiveDateTime),
    Content(String),  // all texts including title, tags, etc
}

struct TextMatch {
    ignore_case: bool,
    match_whole_word: bool,
}
enum SearchFlag {
    Text(TextMatch),
    Time(bool),
}

struct SearchTerm {
    text: SearchItem,
    flag: SearchFlag,
}

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
    fn matches(&self, term: SearchTerm) -> bool {
        match term.text {
            SearchItem::Title(pattern)  => {
                let mut target: String = self.title.clone();
                match term.flag {
                    SearchFlag::Text(tflag) => {
                        let mut ptn: String = pattern.clone();
                        if tflag.ignore_case {
                            target = target.to_lowercase();
                            ptn = ptn.to_lowercase();
                        }
                        if tflag.match_whole_word {
                            let targets: Vec<&str> = target.split_whitespace().collect();
                            targets.contains(&ptn.as_str())
                        } else {
                            target.contains(&ptn)
                        }
                    },
                    _ => panic!("You can't use b/B on Title"),
                }
            },
            SearchItem::Tag(pattern)  => {
                let mut tagstr: String = self.tags.join("; ");
                match term.flag {
                    SearchFlag::Text(tflag) => {
                        let mut ptn: String = pattern.clone();
                        if tflag.ignore_case {
                            tagstr = tagstr.to_lowercase();
                            ptn = ptn.to_lowercase();
                        }
                        if tflag.match_whole_word {
                            let targets: Vec<&str> = tagstr.split("; ").collect();
                            targets.contains(&ptn.as_str())
                        } else {
                            tagstr.contains(&ptn)
                        }
                    }
                    _ => panic!("You can't use b/B on Tags")
                }
            },
            SearchItem::Notebook(pattern)  => {
                let mut target = self.notebook.clone();
                match term.flag {
                    SearchFlag::Text(tflag) => {
                        let mut ptn: String = pattern.clone();
                        if tflag.ignore_case {
                            target = target.to_lowercase();
                            ptn = ptn.to_lowercase();
                        }
                        if tflag.match_whole_word {
                            let targets: Vec<&str> = target.split_whitespace().collect();
                            targets.contains(&ptn.as_str())
                        } else {
                            target.contains(&ptn)
                        }
                    }
                    _ => panic!("You can't use b/B on Notebook")
                }
            },
            SearchItem::Created(pattern)  => {
                match term.flag {
                    SearchFlag::Text(_) => panic!("Add flag B/b to your search pattern"),
                    SearchFlag::Time(is_before) => if is_before {
                        self.created < pattern
                    } else {
                        self.created >= pattern
                    },
                }
            },
            SearchItem::Updated(pattern)  => {
                match term.flag {
                    SearchFlag::Text(_) => panic!("Add flag B/b to your search pattern"),
                    SearchFlag::Time(is_before) => if is_before {
                        self.updated < pattern
                    } else {
                        self.updated >= pattern
                    },
                }
            },
            SearchItem::Content(pattern)  => {
                let mut content: String = format!("{}\n{}\n{}\n{}\n{}\n{}",
                    self.title,
                    self.tags.join("; "),
                    self.notebook,
                    self.created.format("%F %T"),
                    self.updated.format("%F %T"),
                    self.body);
                match term.flag {
                    SearchFlag::Text(tflag) => {
                        let mut ptn: String = pattern.clone();
                        if tflag.ignore_case {
                            content = content.to_lowercase();
                            ptn = ptn.to_lowercase();
                        }
                        if tflag.match_whole_word {
                            let targets: Vec<&str> = content.split_whitespace().collect();
                            targets.contains(&ptn.as_str())
                        } else {
                            content.contains(&ptn)
                        }
                    }
                    _ => panic!("You can't use b/B on Note contents")
                }
            },
        }
    }
}

fn get_git_head(git_root: &str) -> String {
    let cmdout = SysCmd::new("git")
        .args(["-C", git_root, "rev-parse", "--verify", "--short", "HEAD"])
        .output()
        .expect("Get git HEAD hash failed")
        .stdout;
    let rawstr = String::from_utf8(cmdout).expect("Unexpected characters in output");
    String::from(rawstr.trim())
}

fn parse_datetime(datetime: &str) -> NaiveDateTime {
    let fulldt: String = match datetime.len() {
        4 => format!("{}{}", datetime, "-01-01 00:00:00"),
        7 => format!("{}{}", datetime, "-01 00:00:00"),
        10 => format!("{}{}", datetime, " 00:00:00"),
        13 => format!("{}{}", datetime, ":00:00"),
        16 => format!("{}{}", datetime, ":00"),
        19 => String::from(datetime),
        _ => panic!("Invalid DateTime format: {}", datetime),
    };
    NaiveDateTime::parse_from_str(fulldt.as_str(), "%Y-%m-%d %H:%M:%S").unwrap()
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
    if notes.len() == 0 {
        return;
    }
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

fn build_search_term(ptn: &str) -> SearchTerm {
    let elements: Vec<&str> = ptn.split(":" as &str).collect();
    match elements.len() {
        1 => SearchTerm {
            text: SearchItem::Content(String::from(ptn)),
            flag: SearchFlag::Text(TextMatch {ignore_case: true, match_whole_word: false}),
        },
        2 => SearchTerm {
            text: match elements[0] {
                "ti" => SearchItem::Title(String::from(elements[1])),
                "ta" => SearchItem::Tag(String::from(elements[1])),
                "nb" => SearchItem::Notebook(String::from(elements[1])),
                "cr" => SearchItem::Created(parse_datetime(elements[1])),
                "up" => SearchItem::Updated(parse_datetime(elements[1])),
                _ => panic!("Invalid key name: {}", elements[0]),
            },
            flag: SearchFlag::Text(TextMatch {ignore_case: true, match_whole_word: false}),
        },
        3 => SearchTerm {
            text: match elements[0] {
                "ti" => SearchItem::Title(String::from(elements[1])),
                "ta" => SearchItem::Tag(String::from(elements[1])),
                "nb" => SearchItem::Notebook(String::from(elements[1])),
                "cr" => SearchItem::Created(parse_datetime(elements[1])),
                "up" => SearchItem::Updated(parse_datetime(elements[1])),
                _ => panic!("Invalid key name: {}", elements[0]),
            },
            flag: match elements[2] {
                "B" => SearchFlag::Time(false),
                "b" => SearchFlag::Time(true),
                "w" | "wi" | "iw" => SearchFlag::Text(TextMatch {ignore_case: true, match_whole_word: true}),
                "I" | "IW" | "WI" => SearchFlag::Text(TextMatch {ignore_case: false, match_whole_word: false}),
                "iW" | "Wi" | "i" | "W" => SearchFlag::Text(TextMatch {ignore_case: true, match_whole_word: false}),
                "Iw" | "wI" => SearchFlag::Text(TextMatch {ignore_case: false, match_whole_word: true}),
                _ => panic!("Invalid flags: {}", elements[2]),
            },
        },
        _ => panic!("Bad search pattern format!")
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
            let note_rel_path = format!("{NOTE_PREFIX}{timestamp}.md");
            save_note(new_note, &confs.app_home.join(note_rel_path));
        },
        Some(("backup", args)) => {
            let msg: &String = args.get_one("message").unwrap();
            match msg.len() {
                0 => { SysCmd::new("git")
                    .arg("-C")
                    .arg(confs.app_home.join(REPO_DIR))
                    .arg("status")
                    .spawn()
                    .expect("run `git status` failed"); },
                _ => { SysCmd::new("git")
                    .arg("-C")
                    .arg(confs.app_home.join(REPO_DIR))
                    .arg("add")
                    .arg("-A")
                    .spawn()
                    .expect("run `git add` failed")
                    .wait()
                    .expect("Error: Editor returned a non-zero status");
                    SysCmd::new("git")
                    .arg("-C")
                    .arg(confs.app_home.join(REPO_DIR))
                    .arg("commit")
                    .arg("-m")
                    .arg(msg)
                    .spawn()
                    .expect("run `git commit` failed")
                    .wait()
                    .expect("Error: Editor returned a non-zero status");
                    SysCmd::new("git")
                    .arg("-C")
                    .arg(confs.app_home.join(REPO_DIR))
                    .arg("push")
                    .arg("origin")
                    .arg("master")
                    .spawn()
                    .expect("run `git push origin master` failed") 
                    .wait()
                    .expect("Error: Editor returned a non-zero status"); },
            }
        },
        Some(("backup-patch", _)) => {
            let cmdout = SysCmd::new("git")
                .arg("-C")
                .arg(confs.app_home.join(REPO_DIR))
                .arg("status")
                .arg("-s")
                .output()
                .expect("run `git status -s` failed")
                .stdout;
            let rawstr = String::from_utf8(cmdout).expect("Unexpected characters in output");
            let lines = rawstr.lines().collect::<Vec<&str>>();
            let changed_files = lines.iter().map(|x| &x[3..]).collect::<Vec<&str>>();
            let git_head_hash = get_git_head(confs.app_home.join(REPO_DIR).to_str().unwrap());
            let patch_filename = format!("{PATCH_PREFIX}-{git_head_hash}.{PATCH_EXT}");
            SysCmd::new("tar")
                .args(["-cvzf", patch_filename.as_str(), "-C"])
                .arg(confs.app_home.join(REPO_DIR))
                .args(&changed_files)
                .spawn()
                .expect("Create tar file failed")
                .wait()
                .expect("Error: tar command returned a non-zero status");
        },
        Some(("config", args)) => {
            let ck: &String = args.get_one("get").unwrap();
            let ckv: Vec<String> = args.get_many("set").unwrap().cloned().collect();
            match ck.as_str() {
                "" if ckv.len() == 1 => print_config("all"),
                "all" => print_config("all"),
                "" if ckv.len() == 2 => set_config(ckv),
                _ => print_config(ck),
            }
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
            let target_path: &str = notes_dict[idx].filepath.to_str().unwrap();
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
        Some(("import-patch", args)) => {
            let imported_path = args.get_one::<String>("patch_filepath").unwrap();
            let imported_file = PathBuf::from(imported_path);
            let imported_fn = imported_file.file_name().unwrap().to_str().unwrap();

            let git_head_hash = get_git_head(confs.app_home.join(REPO_DIR).to_str().unwrap());
            let target_path = format!("{PATCH_PREFIX}-{git_head_hash}.{PATCH_EXT}");
            let target_file = PathBuf::from(target_path);
            let target_fn = target_file.file_name().unwrap().to_str().unwrap();

            if !(target_fn.eq(imported_fn)) {
                println!("Git head hash mismatch: current repo is: {}", git_head_hash);
                return;
            }
            SysCmd::new("tar")
                .args(["-xvf", imported_path.as_str(), "-C"])
                .arg(confs.app_home.join(REPO_DIR))
                .spawn()
                .expect("Extract tar file failed")
                .wait()
                .expect("Error: tar command returned a non-zero status");
        },
        Some(("list", args)) => {
            let num: &u16 = args.get_one::<u16>("number").unwrap();
            let confs: Config = load_configs();
            let all_notes: Vec<Note> = load_notes(&confs.app_home.join(REPO_DIR));
            let recent_notes = all_notes[.. *num as usize].to_vec();
            save_display(recent_notes, confs)
        },
        Some(("list-notebook", _)) => {
            let all_notes: Vec<Note> = load_notes(&confs.app_home.join(REPO_DIR));
            let mut notebooks = BTreeSet::new();
            for note in all_notes {
                notebooks.insert(note.notebook);
            }
            for nb in notebooks {
                println!("{}", nb)
            }
        },
        Some(("search", args)) => {
            let ptns: Vec<String>  = args.get_many("patterns").unwrap().cloned().collect();
            let all_notes: Vec<Note> = load_notes(&confs.app_home.join(REPO_DIR));
            let mut matched: Vec<Note> = Vec::new();
            for note in all_notes {
                let mut all_match: bool = true;
                for ptn in &ptns {
                    // if ! (note.matches(&ptn, "all", true, false, true)) {
                    let search_term = SearchTerm {
                        text: SearchItem::Content(String::from(ptn)),
                        flag: SearchFlag::Text(TextMatch {ignore_case: true, match_whole_word: false}),
                    };
                    if ! (note.matches(search_term)) {
                        all_match = false;
                        break
                    }
                }
                if all_match { matched.push(note); }
            }
            save_display(matched, confs);
        },
        Some(("search-complex", args)) => {
            let ptns: Vec<String>  = args.get_many("patterns").unwrap().cloned().collect();
            let all_notes: Vec<Note> = load_notes(&confs.app_home.join(REPO_DIR));
            let mut matched: Vec<Note> = Vec::new();
            for note in all_notes {
                let mut all_match: bool = true;
                for ptn in &ptns {
                    if !(note.matches(build_search_term(ptn))) {
                        all_match = false;
                        break
                    }
                }
                if all_match { matched.push(note); }
            }
            save_display(matched, confs);
        },
        Some(("sync", _)) => {
            SysCmd::new("git")
                .arg("-C")
                .arg(confs.app_home.join(REPO_DIR))
                .arg("pull")
                .arg("origin")
                .arg("master")
                .spawn()
                .expect("run `git pull origin master` failed")
                .wait()
                .expect("Failed to sync with remote repo");
        },
        Some(("view", args)) => {
            let idx: &u16 = args.get_one::<u16>("index").unwrap();
            let cache: Vec<u8> = fs::read(confs.app_home.join(CACHE_FILE))
                .expect("Unable to read cache file, run `l` or `s` command to fix this.");
            let notes_dict: BTreeMap<u16, Note> = serde_pickle::from_slice(
                &cache, Default::default()).unwrap();
            let target_path: &str = notes_dict[idx].filepath.to_str().unwrap();
            let viewr_cmd: Vec<&str> = confs.viewer.split_whitespace().collect();
            match viewr_cmd.len() {
                1 => { SysCmd::new(confs.viewer)
                    .arg(target_path)
                    .spawn()
                    .expect("nvim met an error")
                    .wait()
                    .expect("Error: Editor returned a non-zero status");},
                2 => { SysCmd::new(viewr_cmd[0])
                    .arg(viewr_cmd[1])
                    .arg(target_path)
                    .spawn()
                    .expect("nvim met an error")
                    .wait()
                    .expect("Error: Editor returned a non-zero status");},
                _ => unreachable!("There're at most 1 argument permitted in viewer command"),
            }
        },
        _ => unreachable!("Exhausted list of subcommands and subcommand_required prevents `None`"),
    }
}
