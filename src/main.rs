use clap::{command, Arg, ArgAction, Command, ArgMatches};

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
                        .help("pattern(s) to be searched")
                        .action(ArgAction::Append),
                ),
        )
        .subcommand(
            Command::new("search-complex")
                .visible_alias("sc")
                .about("search complex pattern(s) in notes")
                .arg(
                    Arg::new("patterns")
                        .help("pattern(s) to be searched")
                        .action(ArgAction::Append),
                ),
        )
        .subcommand(
            Command::new("sync")
                .visible_alias("syn")
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
