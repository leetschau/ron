use chrono::{DateTime, Utc};
use std::path::Path;
use std::fs;

#[derive(Debug)]
pub struct Note {
    title: String,
    tag_list: Vec<String>,
    notebook: String,
    created: DateTime<Utc>,
    updated: DateTime<Utc>,
    content: String,
    file_path: String,
}

impl Note {
    fn parse(&path: &Path) -> Note {

    }

    fn save(&self, &path: &Path) {
    }
}
pub fn edit_note(note_no: u32) {
    println!("Edit note No. {}", note_no);
}

pub fn simple_search(words: &[&str]) -> Vec<Note> {
    vec![Note {
        title: String::from("mytitile"),
        tag_list: vec![String::from("ab"), String::from("cd")],
        notebook: String::from("/Tech/Public"),
        created: Utc::now(),
        updated: Utc::now(),
        content: String::from("my content"),
        file_path: String::from("/home/leo/abcd/xyz.md"),

    }]
}
