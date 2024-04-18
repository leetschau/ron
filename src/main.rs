use clap::{command, Arg, ArgAction, Command, ArgMatches};

const SEARCH_HELP: &str =
"pattern(s) to be searched, format: [<prefix>:]<stem>[:<suffix>]`.
  Prefix (matching scope) can be one of:
    * t: `title` field of the note;
    * g: `tag` field of the note;
    * n: `notebook` field of the note;
    * c: `created` field of the note;
    * u: `updated` field of the note;
    * a: all textx of a note;
  Suffix (matching style) can be one of:
    * b or B (before/after) for prefix 'created' or 'updated',
    * Single or combination of the following items for prefix title/tag/notebook:
      - i|I: ignore/unignore cases;
      - w|W: whole/partial word match.
  Examples:
    `powershell`,
    `t:powershell a:profile:w`,
    `powershell a:PROFILE:I`,
    `powershell a:PROFILE:I a:op:w`,
    `powershell a:op:Iw`,
    `cr:2021:B`,
    `powershell up:2022-01-12:b`";

fn parse_args() -> ArgMatches {
    command!()
        .bin_name("dn")
        .propagate_version(true)
        .infer_subcommands(true)
        .next_line_help(true)
        .subcommand(
            Command::new("add")
                .visible_alias("a")
                .about("add a new note"),
        )
        .subcommand(
            Command::new("backup")
                .visible_alias("b")
                .about("backup notes to remote repo")
                .arg(
                    Arg::new("message")
                        .help("summarization of committed updates.")
                        .default_value(""),
                ),
        )
        .subcommand(
            Command::new("backup-patch")
                .visible_alias("bp")
                .about("backup unversioned notes to patch file \
                    /tmp/donno-patch-<git-hash>.tgz \n\
                    (defined in lib.rs as const string)"),
        )
        .subcommand(
            Command::new("config")
                .visible_alias("conf")
                .about("get/set configurations")
                .arg(
                    Arg::new("get")
                        .short('g')
                        .required(false)
                        .help("print all (or specified) configurations")
                        .num_args(0..=1)
                        .default_missing_value("all")
                        .default_value(""),
                )
                .arg(
                    Arg::new("set")
                        .short('s')
                        .required(false)
                        .conflicts_with("get")
                        .num_args(2)
                        .default_value("")
                        .help("set specified configurations"),
                ),
        )
        .subcommand(
            Command::new("delete")
                .visible_alias("del")
                .about("delete the selected note")
                .arg(
                    Arg::new("index")
                        .help("index of the note to be edited.")
                        .value_parser(clap::value_parser!(u16).range(..30000))
                        .default_value("1"),
                ),
        )
        .subcommand(
            Command::new("edit")
                .visible_alias("e")
                .about("edit the selected note")
                .arg(
                    Arg::new("index")
                        .help("index of the note to be edited.")
                        .value_parser(clap::value_parser!(u16).range(..30000))
                        .default_value("1"),
                ),
        )
        .subcommand(
            Command::new("import-patch")
                .visible_alias("ip")
                .about("import notes from patch file")
                .arg(
                    Arg::new("patch_filepath")
                        .value_name("PATCH_FILE_PATH")
                        .help("path of the patch file to be imported")
                ),
        )
        .subcommand(
            Command::new("list")
                .visible_alias("l")
                .about("list recent updated notes")
                .arg(
                    Arg::new("number")
                        .help("number of notes to be listed.")
                        .value_parser(clap::value_parser!(u16))
                        .default_value("5"),
                ),
        )
        .subcommand(
            Command::new("list-notebook")
                .visible_alias("lnb")
                .about("list notebooks"),
        )
        .subcommand(
            Command::new("search")
                .visible_alias("s")
                .about("search pattern(s) in notes")
                .arg(
                    Arg::new("patterns")
                        .help(SEARCH_HELP)
                        .action(ArgAction::Append),
                ),
        )
        .subcommand(
            Command::new("sync")
                .visible_alias("y")
                .about("sync (pull) notes from remote repo"),
        )
        .subcommand(
            Command::new("view")
                .visible_alias("v")
                .about("view the selected note")
                .arg(
                    Arg::new("index")
                        .help("index of the note to be edited.")
                        .value_parser(clap::value_parser!(u16).range(..30000))
                        .default_value("1"),
                ),
        )
        .get_matches()
}

fn main() {
    ron::run(parse_args());
}
