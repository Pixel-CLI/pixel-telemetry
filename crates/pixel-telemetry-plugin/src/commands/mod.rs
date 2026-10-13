// SPDX-FileCopyrightText: The Pixel contributors
// SPDX-License-Identifier: MIT

//! The five commands of the `telemetry` plugin and how a name finds its code.

use std::ffi::OsString;
use std::process::ExitCode;

mod action_log;
mod list_errors;
mod record_event;
mod space;
mod token_savings;

/// One command: the name `pixel <name>` is typed with, a line for `--help`,
/// and the entry point, which receives the arguments after the name.
pub struct Command {
    pub name: &'static str,
    pub about: &'static str,
    run: fn(Vec<OsString>) -> ExitCode,
}

impl Command {
    /// Run the command on `args` (the arguments after its name).
    pub fn run(&self, args: &[OsString]) -> ExitCode {
        let mut argv = Vec::with_capacity(args.len() + 1);
        argv.push(OsString::from(format!("pixel {}", self.name)));
        argv.extend_from_slice(args);
        (self.run)(argv)
    }
}

const fn command(
    name: &'static str,
    about: &'static str,
    run: fn(Vec<OsString>) -> ExitCode,
) -> Command {
    Command { name, about, run }
}

/// Every command, in the order `--help` lists them. This list is also the
/// `commands` array of `pixel-plugin.toml` (`tests/manifest.rs` compares the
/// two).
pub const COMMANDS: [Command; 5] = [
    command(
        "list-errors",
        "One-look error capture: query the sniper error sink",
        list_errors::list_errors,
    ),
    command(
        "record-event",
        "Journal a session event (fire-and-forget)",
        record_event::record_event,
    ),
    command(
        "action-log",
        "Self-assessment: pixel's own action log (what ran, what went wrong)",
        action_log::action_log,
    ),
    command(
        "token-savings",
        "Token-savings report over the action log",
        token_savings::token_savings,
    ),
    command(
        "space",
        "Audit how much disk the .pixel index takes across every project under a tree",
        space::space,
    ),
];

/// The command named `name`, if this plugin provides it.
pub fn find(name: &str) -> Option<&'static Command> {
    COMMANDS.iter().find(|c| c.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_unique_and_found() {
        for (i, c) in COMMANDS.iter().enumerate() {
            assert!(find(c.name).is_some_and(|f| f.name == c.name));
            assert!(
                COMMANDS[i + 1..].iter().all(|o| o.name != c.name),
                "{}",
                c.name
            );
        }
        assert!(find("telemetry").is_none());
        assert!(find("").is_none());
    }
}
