// SPDX-FileCopyrightText: The Pixel contributors
// SPDX-License-Identifier: MIT

//! `pixel token-savings`.
//!
//! The aggregation and printing are ported from `crates/pixel/src/main.rs` of
//! Pixel-CLI/pixel (MIT), where they were private to the `pixel` binary crate
//! (and from the first plugin form, Pixel-CLI/pixel-plugins). The action log
//! and its summary are core's `pixel-actionlog` library.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;

use crate::env::resolve_path;
use crate::root::discover_root;
use crate::run::{HostFlags, run};

/// Token-savings report: for retrieval-shaped commands (search/query/
/// context/resolve) that recorded snippet-vs-pool volumes, aggregate the
/// fraction of the candidate pool the agent did NOT have to read. A
/// measured counter to semble's '99% fewer tokens' claim — own numbers,
/// same format.
#[derive(Parser)]
#[command(name = "token-savings", bin_name = "pixel token-savings")]
struct Cli {
    #[command(flatten)]
    _host: HostFlags,
    #[arg(default_value = ".")]
    path: PathBuf,
    #[arg(long)]
    json: bool,
    /// Only consider events from the last N hours.
    #[arg(long)]
    since_hours: Option<u64>,
}

/// Entry point of `pixel token-savings` (the arguments after the command name).
pub fn token_savings(args: Vec<OsString>) -> ExitCode {
    run::<Cli>("token-savings", args, |cli| {
        run_savings(&cli.path, cli.json, cli.since_hours)
    })
}

/// Per-command aggregate of `pixel savings`: invocations, pool chars,
/// snippet chars.
#[derive(Debug, Default, PartialEq, Eq)]
struct SavingsAgg {
    count: u64,
    pool: u64,
    snippet: u64,
}

/// The events at or after `cutoff_ms` (all of them without a cutoff).
fn savings_window(
    events: &[pixel_actionlog::ActionEvent],
    cutoff_ms: Option<i64>,
) -> Vec<pixel_actionlog::ActionEvent> {
    events
        .iter()
        .filter(|e| cutoff_ms.is_none_or(|c| e.ts_ms >= c))
        .cloned()
        .collect()
}

/// Pool and snippet chars per command, over the events that recorded both.
fn savings_by_command(
    events: &[pixel_actionlog::ActionEvent],
) -> std::collections::BTreeMap<String, SavingsAgg> {
    let mut by_cmd = std::collections::BTreeMap::<String, SavingsAgg>::new();
    for e in events {
        let (Some(snippet), Some(pool)) = (e.snippet_cap_chars, e.pool_chars) else {
            continue; // not retrieval-shaped (or volumes not recorded)
        };
        let agg = by_cmd.entry(e.command.clone()).or_default();
        agg.count += 1;
        agg.pool = agg.pool.saturating_add(pool);
        agg.snippet = agg.snippet.saturating_add(snippet);
    }
    by_cmd
}

/// The share of the pool the snippets did not return; 0 for an empty pool.
fn savings_ratio(snippet: u64, pool: u64) -> f64 {
    if pool > 0 {
        1.0 - (snippet as f64 / pool as f64)
    } else {
        0.0
    }
}

fn run_savings(path: &Path, json: bool, since_hours: Option<u64>) -> Result<(), String> {
    let root = discover_root(&resolve_path(path))?;
    let log_path = pixel_actionlog::ActionLog::path_for_root(&root);
    // Over-fetch; savings is a lightweight aggregate read.
    let events = pixel_actionlog::tail(&log_path, 1_000_000)
        .map_err(|e| format!("read {}: {e}", log_path.display()))?;
    let cutoff_ms = since_hours.map(|h| {
        pixel_actionlog::now_ms().saturating_sub(
            i64::try_from(h)
                .unwrap_or(i64::MAX)
                .saturating_mul(3_600_000),
        )
    });
    let filtered = savings_window(&events, cutoff_ms);
    let workflow_metrics = pixel_actionlog::summarize_metrics(&filtered);
    let by_cmd = savings_by_command(&filtered);

    let tot_pool: u64 = by_cmd.values().map(|a| a.pool).sum();
    let tot_snippet: u64 = by_cmd.values().map(|a| a.snippet).sum();
    let overall = savings_ratio(tot_snippet, tot_pool);

    if json {
        let rows: Vec<serde_json::Value> = by_cmd
            .iter()
            .map(|(cmd, a)| {
                let ratio = savings_ratio(a.snippet, a.pool);
                serde_json::json!({
                    "command": cmd,
                    "calls": a.count,
                    "pool_chars": a.pool,
                    "snippet_chars": a.snippet,
                    "savings": ratio,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::json!({
                "overall_savings": overall,
                "total_pool_chars": tot_pool,
                "total_snippet_chars": tot_snippet,
                "by_command": rows,
                "legacy_basis": "legacy snippet/pool byte comparison; not a workflow estimate",
                "workflow_metrics": workflow_metrics,
            })
        );
        return Ok(());
    }

    println!("workflow metrics (measured bytes/duration; versioned byte-based token estimates)");
    println!(
        "{}",
        serde_json::to_string_pretty(&workflow_metrics).map_err(|e| e.to_string())?
    );
    println!("legacy savings (snippet vs candidate-pool chars; not a workflow estimate)");
    println!(
        "{:<14} {:>5}  {:>12}  {:>14}  {:>7}",
        "command", "calls", "pool_chars", "snippet_chars", "savings"
    );
    for (cmd, a) in &by_cmd {
        let ratio = savings_ratio(a.snippet, a.pool);
        println!(
            "{:<14} {:>5}  {:>12}  {:>14}  {:>6.1}%",
            cmd,
            a.count,
            a.pool,
            a.snippet,
            ratio * 100.0
        );
    }
    println!(
        "{:<14} {:>5}  {:>12}  {:>14}  {:>6.1}%",
        "TOTAL",
        by_cmd.values().map(|a| a.count).sum::<u64>(),
        tot_pool,
        tot_snippet,
        overall * 100.0
    );
    Ok(())
}
