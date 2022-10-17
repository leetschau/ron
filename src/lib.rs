mod config;

use std::process::Command as SysCmd;
use clap::ArgMatches;
//use std::path::Path;
use config::{load_configs, };

//const CONF_PATH: &str = "~/.config/donno/config.json"
//const TEMP_DIR: &str = "/tmp"
const TEMP_NOTE: &str = "/tmp/dsnote-tmp.md";
const NOTE_HEADER: &str = "Title: \nTags: \nNotebook: $defNotebook\nCreated: $curTime\nUpdated: $curTime\n\n------\n\n";

pub fn run(args: ArgMatches) {
    let confs = load_configs();
    match args.subcommand() {
        Some(("add", _)) => {
            std::fs::write(TEMP_NOTE, NOTE_HEADER).expect("Write note header failed!");
            SysCmd::new(confs.editor)
                .arg(TEMP_NOTE)
                .spawn()
                .expect("nvim met an error")
                .wait()
                .expect("Error: Editor returned a non-zero status");
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
