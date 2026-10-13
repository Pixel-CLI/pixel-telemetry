// SPDX-FileCopyrightText: The Pixel contributors
// SPDX-License-Identifier: MIT

//! The `telemetry` plugin of Pixel: five commands behind one `pixel-telemetry`
//! binary (plugin API 1).
//!
//! A plugin is a separate executable that Pixel core runs for `pixel <name> …`.
//! This one provides many names, so [`dispatch`] picks the command from the
//! first argument (`pixel-telemetry list-errors …`) or from the name the binary was
//! started under (`pixel-list-errors`, a link made by `pixel-telemetry install-links`).
//!
//! Most of the code is ported from `crates/pixel/src/` of Pixel-CLI/pixel
//! (MIT), where it was private to the `pixel` binary or deleted from it; each
//! module says where it came from. What several commands need from the host
//! lives in [`env`], [`root`], [`output`] and [`run`].

pub mod commands;
pub mod engine;
pub mod env;
pub mod links;
pub mod output;
pub mod root;
pub mod run;
#[cfg(any(test, feature = "testing"))]
pub mod testing;

use std::ffi::OsString;
use std::path::Path;
use std::process::ExitCode;

/// The graph store's file name inside `.pixel/`. Core spells it
/// `pixel_daemon::api::GRAPH_DB_FILE`; `tests/core_drift.rs` fails when the two
/// differ, so a pin bump that renames the file cannot pass unnoticed.
pub const GRAPH_DB_FILE: &str = "graph.v2.db";

/// What `pixel-telemetry --help` prints.
fn usage() -> String {
    let mut text = String::from(
        "pixel-telemetry: the telemetry commands of Pixel\n\n\
         Usage: pixel-telemetry <command> [args]...\n       \
         pixel-<command> [args]...        (after `pixel-telemetry install-links`)\n       \
         pixel <command> [args]...        (the host runs the line above)\n\n\
         Commands:\n",
    );
    for c in &commands::COMMANDS {
        text.push_str(&format!("  {:<16}{}\n", c.name, c.about));
    }
    text.push_str(
        "\nOther:\n  install-links   Link pixel-<command> to this binary so `pixel <command>` finds it on PATH\n\n\
         `pixel-telemetry <command> --help` shows a command's own options.\n",
    );
    text
}

/// The command an executable name selects: `pixel-list-errors` is `list-errors`, while
/// `pixel-telemetry` itself selects none.
pub fn command_of_program(program: &Path) -> Option<&'static commands::Command> {
    let stem = program.file_name()?.to_str()?;
    commands::find(stem.strip_prefix("pixel-")?)
}

/// Run the plugin for `args` (the whole argv, program name first).
pub fn dispatch(args: &[OsString]) -> ExitCode {
    let Some(program) = args.first() else {
        return ExitCode::FAILURE;
    };
    if let Some(command) = command_of_program(Path::new(program)) {
        return command.run(&args[1..]);
    }
    let Some(first) = args.get(1) else {
        eprint!("{}", usage());
        return ExitCode::from(2);
    };
    match first.to_str() {
        Some("-h" | "--help" | "help") => {
            print!("{}", usage());
            ExitCode::SUCCESS
        }
        Some("-V" | "--version") => {
            println!("pixel-telemetry {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some("install-links") => links::run(&args[2..]),
        Some(name) => match commands::find(name) {
            Some(command) => command.run(&args[2..]),
            None => {
                eprintln!("pixel-telemetry: unknown command `{name}`\n");
                eprint!("{}", usage());
                ExitCode::from(2)
            }
        },
        None => {
            eprintln!("pixel-telemetry: the command name is not valid UTF-8");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_program_name_selects_its_command_and_pixel_telemetry_none() {
        assert_eq!(
            command_of_program(Path::new("/x/bin/pixel-list-errors")).map(|c| c.name),
            Some("list-errors")
        );
        assert_eq!(
            command_of_program(Path::new("pixel-action-log")).map(|c| c.name),
            Some("action-log")
        );
        assert!(command_of_program(Path::new("/x/pixel-telemetry")).is_none());
        assert!(command_of_program(Path::new("list-errors")).is_none());
        assert!(command_of_program(Path::new("pixel-nope")).is_none());
    }

    #[test]
    fn the_usage_lists_every_command() {
        let text = usage();
        for c in &commands::COMMANDS {
            assert!(text.contains(c.name), "{}", c.name);
        }
    }
}
