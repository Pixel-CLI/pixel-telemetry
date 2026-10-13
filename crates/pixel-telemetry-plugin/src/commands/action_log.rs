// SPDX-FileCopyrightText: The Pixel contributors
// SPDX-License-Identifier: MIT

//! `pixel action-log` — the self-assessment surface over the async action log
//! every pixel invocation writes to `<root>/.pixel/actions.jsonl`.
//!
//! Ported from `run_log` and its helpers in `crates/pixel/src/main.rs` of
//! Pixel-CLI/pixel (MIT). Core keeps *writing* the log (`pixel-actionlog`,
//! the library this reads); only the viewer lives here.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;

use crate::env::resolve_path;
use crate::root::discover_root;
use crate::run::{HostFlags, run};

/// Self-assessment: pixel's own action log (what ran, what went wrong).
/// Reads <path>/.pixel/actions.jsonl, written asynchronously by every
/// pixel invocation.
#[derive(Parser)]
#[command(name = "action-log", bin_name = "pixel action-log")]
struct ActionLogCli {
    #[command(flatten)]
    _host: HostFlags,
    #[arg(default_value = ".")]
    path: PathBuf,
    /// Most recent entries to show.
    #[arg(long, default_value_t = 20)]
    limit: usize,
    /// Only show entries that ended in an error.
    #[arg(long)]
    errors_only: bool,
    #[arg(long)]
    json: bool,
    /// Delete the action log for this root and exit.
    #[arg(long)]
    clear: bool,
}

/// Entry point of `pixel action-log` (the arguments after the command name).
pub fn action_log(args: Vec<OsString>) -> ExitCode {
    run::<ActionLogCli>("action-log", args, |cli| {
        run_log(
            &resolve_path(&cli.path),
            cli.limit,
            cli.errors_only,
            cli.json,
            cli.clear,
        )
    })
}

/// How many trailing log entries `pixel log` reads. Over-fetch when
/// filtering to errors so `limit` still means "the last N errors", not "the
/// last N entries, some of which happen to be errors".
fn log_fetch_count(limit: usize, errors_only: bool) -> usize {
    if errors_only {
        limit.max(1) * 20
    } else {
        limit.max(1)
    }
}

/// The last `limit` of the tailed entries, errors only when asked.
fn select_log_events(
    mut events: Vec<pixel_actionlog::ActionEvent>,
    limit: usize,
    errors_only: bool,
) -> Vec<pixel_actionlog::ActionEvent> {
    if errors_only {
        events.retain(|e| e.outcome == pixel_actionlog::Outcome::Error);
    }
    if events.len() > limit {
        let start = events.len() - limit;
        events.drain(0..start);
    }
    events
}

/// `pixel log` — the self-assessment surface over the async action log every
/// pixel invocation writes to `<root>/.pixel/actions.jsonl`.
fn run_log(
    path: &Path,
    limit: usize,
    errors_only: bool,
    json: bool,
    clear: bool,
) -> Result<(), String> {
    let root = discover_root(path)?;
    let log_path = pixel_actionlog::ActionLog::path_for_root(&root);
    if clear {
        return match std::fs::remove_file(&log_path) {
            Ok(()) => {
                println!("action log cleared: {}", log_path.display());
                Ok(())
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                println!("no action log at {}", log_path.display());
                Ok(())
            }
            Err(e) => Err(format!("remove {}: {e}", log_path.display())),
        };
    }
    let tailed = pixel_actionlog::tail(&log_path, log_fetch_count(limit, errors_only))
        .map_err(|e| format!("read {}: {e}", log_path.display()))?;
    let events = select_log_events(tailed, limit, errors_only);
    if json {
        for e in &events {
            println!("{}", serde_json::to_string(e).map_err(|e| e.to_string())?);
        }
        return Ok(());
    }
    if events.is_empty() {
        println!("no recorded actions at {}", log_path.display());
        return Ok(());
    }
    let now = pixel_actionlog::now_ms();
    for e in &events {
        let when = relative_time(e.ts_ms, now);
        match e.outcome {
            pixel_actionlog::Outcome::Ok => {
                println!(
                    "{when:>8}  ok     {:<10} {} ({} ms)",
                    e.command, e.args, e.duration_ms
                );
            }
            pixel_actionlog::Outcome::Error => {
                println!(
                    "{when:>8}  ERROR  {:<10} {} ({} ms) — {}",
                    e.command,
                    e.args,
                    e.duration_ms,
                    e.error.as_deref().unwrap_or("?")
                );
            }
        }
        if let Some(line) = pixel_actionlog::format_metrics_line(e) {
            println!("  {line}");
        }
    }
    Ok(())
}

fn relative_time(ts_ms: i64, now_ms: i64) -> String {
    let delta_ms = (now_ms - ts_ms).max(0);
    let secs = delta_ms / 1000;
    if secs < 5 {
        "just now".to_string()
    } else if secs < 60 {
        format!("{secs}s ago")
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86400)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pixel_actionlog::{ActionEvent, Outcome};

    fn event(command: &str, outcome: Outcome) -> ActionEvent {
        let mut event = ActionEvent::new(command, "");
        event.outcome = outcome;
        event
    }

    #[test]
    fn an_errors_only_read_over_fetches_so_limit_means_the_last_n_errors() {
        assert_eq!(log_fetch_count(5, false), 5);
        assert_eq!(log_fetch_count(0, false), 1);
        assert_eq!(log_fetch_count(5, true), 100);
    }

    #[test]
    fn the_last_limit_events_are_kept_and_errors_only_filters_first() {
        let events = vec![
            event("a", Outcome::Error),
            event("b", Outcome::Ok),
            event("c", Outcome::Error),
            event("d", Outcome::Ok),
        ];
        let last_two: Vec<_> = select_log_events(events.clone(), 2, false)
            .into_iter()
            .map(|e| e.command)
            .collect();
        assert_eq!(last_two, ["c", "d"]);
        let errors: Vec<_> = select_log_events(events, 5, true)
            .into_iter()
            .map(|e| e.command)
            .collect();
        assert_eq!(errors, ["a", "c"]);
    }

    #[test]
    fn a_time_is_shown_relative_to_now() {
        assert_eq!(relative_time(1_000, 1_000), "just now");
        assert_eq!(relative_time(0, 4_999), "just now");
        assert_eq!(relative_time(0, 5_000), "5s ago");
        assert_eq!(relative_time(0, 60_000), "1m ago");
        assert_eq!(relative_time(0, 3_600_000), "1h ago");
        assert_eq!(relative_time(0, 86_400_000), "1d ago");
        // A timestamp in the future is "just now", not a negative age.
        assert_eq!(relative_time(10_000, 0), "just now");
    }
}
