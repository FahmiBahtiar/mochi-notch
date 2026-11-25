use std::process::ExitCode;

use clap::ArgMatches;

mod activities;

pub fn dispatch(command: &str, matches: &ArgMatches) -> Option<ExitCode> {
    match command {
        "mochi-cli" | "activity" | "help" | "hook" | "skill" => {
            Some(activities::run(command, matches))
        }
        _ => None,
    }
}
