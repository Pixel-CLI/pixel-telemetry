// SPDX-FileCopyrightText: The Pixel contributors
// SPDX-License-Identifier: MIT

//! `list-errors` and `action-log` against the built-in commands of Pixel core
//! (goldens recorded from `ad788194`, `origin/main` when they were ported).

use pixel_actionlog::{ActionEvent, ActionLog, Outcome, now_ms};
use pixel_telemetry_plugin::testing::{Setup, assert_parity, git_fixture};

const BIN: &str = env!("CARGO_BIN_EXE_pixel-telemetry");

/// A setup whose error store lives in its own scratch directory, so the
/// built-in and the plugin never see each other's (or the user's) records.
fn sniper(args: &[&str]) -> Setup {
    let mut setup = Setup::new(git_fixture(), args);
    let state = setup.scratch();
    setup
        .env("PIXEL_SNIPER_STATE_ROOT", state)
        .scrub_pattern(r"run-OID-\d+", "run-RUN")
        .scrub_pattern(r#""pid":\d+"#, r#""pid":PID"#)
        .scrub_pattern(r"pid \d+", "pid PID")
        .scrub_pattern(r"\d+s ago", "Ns ago")
}

/// `args` after one wrapped command that failed with `exit 9` and wrote
/// `boom` to stderr.
fn after_a_failed_run(args: &[&str]) -> Setup {
    let mut setup = sniper(args);
    setup = setup.then_failing(&[
        "run",
        "--label",
        "demo",
        "--",
        "/bin/sh",
        "-c",
        "printf 'boom\\n' >&2; exit 9",
    ]);
    setup
}

fn parity(case: &str, make: &dyn Fn() -> Setup) {
    // The command is `list-errors` or `action-log`: the case name starts with it.
    let command = if case.starts_with("action-log") {
        "action-log"
    } else {
        "list-errors"
    };
    assert_parity(env!("CARGO_MANIFEST_DIR"), BIN, command, case, make);
}

// --- list-errors -------------------------------------------------------------

#[test]
fn list_errors_last_of_an_empty_store_as_json() {
    parity("list-errors-last-empty-json", &|| {
        sniper(&["last", "--json"])
    });
}

#[test]
fn list_errors_last_of_an_empty_store() {
    parity("list-errors-last-empty", &|| sniper(&["last"]));
}

#[test]
fn a_failed_wrapped_command_is_what_last_reads_back() {
    parity("list-errors-last-json", &|| {
        after_a_failed_run(&["last", "--json"])
    });
}

#[test]
fn last_renders_a_listing_with_a_cursor_footer() {
    parity("list-errors-last", &|| after_a_failed_run(&["last"]));
}

#[test]
fn last_can_be_limited_to_one_surface() {
    parity("list-errors-last-surface-json", &|| {
        after_a_failed_run(&["last", "--surface", "run-wrapper", "-n", "1", "--json"])
    });
}

#[test]
fn an_unknown_surface_is_an_error() {
    parity("list-errors-last-bad-surface", &|| {
        sniper(&["last", "--surface", "nope"])
    });
}

#[test]
fn since_a_cursor_lists_the_newer_errors() {
    parity("list-errors-since-json", &|| {
        after_a_failed_run(&["since", "0", "--json"])
    });
}

#[test]
fn since_needs_exactly_one_of_a_cursor_and_a_window() {
    parity("list-errors-since-neither", &|| sniper(&["since"]));
}

#[test]
fn since_a_time_window_lists_recent_errors() {
    parity("list-errors-since-ts-json", &|| {
        after_a_failed_run(&["since", "--ts", "5m", "--json"])
    });
}

#[test]
fn a_bad_window_is_an_error() {
    parity("list-errors-since-bad-ts", &|| {
        sniper(&["since", "--ts", "soon"])
    });
}

#[test]
fn show_prints_one_error_in_full() {
    parity("list-errors-show-json", &|| {
        after_a_failed_run(&["show", "1", "--json"])
    });
}

#[test]
fn show_renders_one_error_as_text() {
    parity("list-errors-show", &|| after_a_failed_run(&["show", "1"]));
}

#[test]
fn show_of_an_unknown_id_is_an_error() {
    parity("list-errors-show-missing", &|| sniper(&["show", "99"]));
}

#[test]
fn cursor_is_the_highest_error_id() {
    parity("list-errors-cursor-json", &|| {
        after_a_failed_run(&["cursor", "--json"])
    });
    parity("list-errors-cursor", &|| after_a_failed_run(&["cursor"]));
}

#[test]
fn gc_applies_retention() {
    parity("list-errors-gc-json", &|| {
        after_a_failed_run(&["gc", "--vacuum", "--json"])
    });
}

#[test]
fn run_mirrors_the_wrapped_exit_code_and_tees_its_output() {
    parity("list-errors-run-exit", &|| {
        sniper(&[
            "run",
            "--",
            "/bin/sh",
            "-c",
            "echo out; echo err >&2; exit 3",
        ])
    });
}

#[test]
fn run_of_a_passing_command_records_nothing() {
    parity("list-errors-run-pass", &|| {
        sniper(&["run", "--", "/bin/sh", "-c", "echo fine"])
    });
}

// --- action-log --------------------------------------------------------------

/// A repository whose action log holds one slow ok command, one failing one
/// and one with a recorded volume, 3 hours and 30 minutes old (so the
/// relative time prints `3h ago` however long the test takes).
fn with_log() -> tempfile::TempDir {
    let repo = git_fixture();
    let event = |command: &str, args: &str, outcome: Outcome, error: Option<&str>, age: i64| {
        let mut event = ActionEvent::new(command, args);
        event.invocation_id = Some(format!("inv-{command}-{age}"));
        event.pid = 7;
        event.cwd = "/work".to_string();
        event.ts_ms = now_ms() - age * 60_000;
        event.duration_ms = 12 + u64::try_from(age).unwrap_or(0);
        event.outcome = outcome;
        event.error = error.map(str::to_string);
        event
    };
    let events = [
        event(
            "search-content",
            "search-content foo",
            Outcome::Ok,
            None,
            210,
        ),
        event(
            "diff",
            "diff main",
            Outcome::Error,
            Some("unknown ref main"),
            209,
        ),
        event("repo-state", "repo-state --json", Outcome::Ok, None, 208),
    ];
    let path = ActionLog::path_for_root(&repo.path().canonicalize().expect("canonical"));
    std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    let lines: Vec<String> = events
        .iter()
        .map(|e| serde_json::to_string(e).expect("serialize"))
        .collect();
    std::fs::write(path, lines.join("\n") + "\n").expect("write log");
    repo
}

#[test]
fn an_empty_log_says_nothing_was_recorded() {
    parity("action-log-empty", &|| Setup::new(git_fixture(), &[]));
}

#[test]
fn the_log_lists_the_recent_actions() {
    parity("action-log-table", &|| Setup::new(with_log(), &[]));
}

#[test]
fn the_log_can_be_limited() {
    parity("action-log-limit", &|| {
        Setup::new(with_log(), &["--limit", "1"])
    });
}

#[test]
fn the_log_can_show_errors_only() {
    parity("action-log-errors-only", &|| {
        Setup::new(with_log(), &["--errors-only"])
    });
}

#[test]
fn the_log_as_json_is_one_event_per_line() {
    parity("action-log-json", &|| Setup::new(with_log(), &["--json"]));
}

#[test]
fn clear_deletes_the_log() {
    parity("action-log-clear", &|| Setup::new(with_log(), &["--clear"]));
}

#[test]
fn clear_without_a_log_says_so() {
    parity("action-log-clear-absent", &|| {
        Setup::new(git_fixture(), &["--clear"])
    });
}
