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
                        .value_parser(clap::value_parser!(u8))
                        .default_value("5"),
                ),
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
        .get_matches()
}
fn main() {
    ron::run(parse_args());
}