// SPDX-FileCopyrightText: The Pixel contributors
// SPDX-License-Identifier: MIT

//! `pixel record-event`: journal a session event.
//!
//! Restored from `87e17b41^` of Pixel-CLI/pixel (MIT), where core deleted the
//! command in #898: the command line and handler from
//! `crates/pixel/src/main.rs`. The journal write is core's `Request::Journal`,
//! run in this process like every engine request of a plugin
//! ([`crate::engine`]).

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use pixel_daemon::api::Request;

use crate::engine::execute;
use crate::env::resolve_path;
use crate::output::print_data;
use crate::run::{HostFlags, run};

/// Journal a session event (fire-and-forget).
#[derive(Parser)]
#[command(name = "record-event", bin_name = "pixel record-event")]
struct RecordEventCli {
    #[command(flatten)]
    _host: HostFlags,
    kind: String,
    #[arg(default_value = ".")]
    path: PathBuf,
    /// Repo-relative path the event concerns.
    #[arg(long)]
    file: Option<String>,
    #[arg(long)]
    detail: Option<String>,
    #[arg(long)]
    json: bool,
}

/// Entry point of `pixel record-event` (the arguments after the command name).
pub fn record_event(args: Vec<OsString>) -> ExitCode {
    run::<RecordEventCli>("record-event", args, |cli| {
        let data = execute(
            &resolve_path(&cli.path),
            Request::Journal {
                kind: cli.kind,
                path: cli.file,
                detail: cli.detail,
            },
        )?;
        print_data(&data, cli.json)
    })
}
