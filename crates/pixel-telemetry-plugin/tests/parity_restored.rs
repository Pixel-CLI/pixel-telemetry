// SPDX-FileCopyrightText: The Pixel contributors
// SPDX-License-Identifier: MIT

//! The commands core no longer has (`token-savings`, `record-event`, `space`)
//! against the built-ins of `87e17b41^` (`dcbe7156`), the last core that had
//! them; goldens were recorded from that build.

use pixel_actionlog::{ActionEvent, ActionLog, now_ms};
use pixel_telemetry_plugin::testing::{Setup, assert_parity, git_fixture, write};

const BIN: &str = env!("CARGO_BIN_EXE_pixel-telemetry");

fn parity(command: &str, case: &str, make: &dyn Fn() -> Setup) {
    assert_parity(env!("CARGO_MANIFEST_DIR"), BIN, command, case, make);
}

// --- token-savings -----------------------------------------------------------

fn savings(case: &str, make: &dyn Fn() -> Setup) {
    parity("token-savings", case, make);
}

/// An action log with two retrieval-shaped commands (one of them twice, the
/// older call two days ago), one command that recorded no volumes, and an
/// empty pool.
fn fixture_with_log() -> tempfile::TempDir {
    let repo = git_fixture();
    let event = |command: &str, age_hours: i64, volumes: Option<(u64, u64)>| {
        let mut event = ActionEvent::new(command, "");
        event.invocation_id = Some(format!("inv-{command}-{age_hours}"));
        event.pid = 1;
        event.cwd = "/work".to_string();
        event.ts_ms = now_ms() - age_hours * 3_600_000;
        if let Some((snippet, pool)) = volumes {
            event.snippet_cap_chars = Some(snippet);
            event.pool_chars = Some(pool);
        }
        event
    };
    let events = [
        event("search", 48, Some((1_000, 10_000))),
        event("search", 1, Some((500, 5_000))),
        event("context", 1, Some((2_000, 4_000))),
        event("status", 1, None),
        event("resolve", 1, Some((0, 0))),
    ];
    let path = ActionLog::path_for_root(repo.path());
    std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    let lines: Vec<String> = events
        .iter()
        .map(|e| serde_json::to_string(e).expect("serialize"))
        .collect();
    std::fs::write(path, lines.join("\n") + "\n").expect("write log");
    repo
}

#[test]
fn an_empty_log_reports_nothing_saved() {
    savings("token-savings-empty", &|| Setup::new(git_fixture(), &[]));
}

#[test]
fn a_log_with_retrieval_events_is_aggregated_per_command() {
    savings("token-savings-table", &|| {
        Setup::new(fixture_with_log(), &[])
    });
}

#[test]
fn the_json_report_carries_the_totals_and_the_legacy_basis() {
    savings("token-savings-json", &|| {
        Setup::new(fixture_with_log(), &["--json"])
    });
}

#[test]
fn since_hours_drops_older_events() {
    savings("token-savings-since-hours-json", &|| {
        Setup::new(fixture_with_log(), &["--json", "--since-hours", "24"])
    });
}

// --- record-event ------------------------------------------------------------

fn journal(case: &str, args: &'static [&'static str]) {
    parity("record-event", case, &|| {
        Setup::new(git_fixture(), args)
            // The journal records the event time and ids.
            .scrub_pattern(r#""(id|event_id)": ?\d+"#, r#""$1": N"#)
    });
}

#[test]
fn an_event_is_journaled() {
    journal(
        "record-event-edit",
        &["edit", "--file", "src/lib.rs", "--detail", "x"],
    );
}

#[test]
fn an_event_is_journaled_as_json() {
    journal("record-event-json", &["edit", "--json"]);
}

#[test]
fn an_event_needs_a_kind() {
    journal("record-event-no-kind", &[]);
}

// --- space -------------------------------------------------------------------

/// A scratch tree of projects, each with a `.pixel/base.shard` of the given
/// size, plus one project whose `.pixel` holds only housekeeping.
fn tree_with_shards(args: &[&str]) -> Setup {
    let mut setup = Setup::new(git_fixture(), &[]);
    let tree = setup.scratch();
    for (project, bytes) in [("one", 100), ("two", 200), ("deep/three", 1_024)] {
        write(
            &tree,
            &format!("{project}/.pixel/base.shard"),
            &"x".repeat(bytes),
        );
    }
    write(&tree, "journaled/.pixel/actions.jsonl", "notes");
    write(&tree, "node_modules/pkg/.pixel/base.shard", "ignored");
    setup.args = args
        .iter()
        .map(|a| (*a).to_string())
        .chain([tree.display().to_string()])
        .collect();
    setup
}

#[test]
fn space_lists_each_project_and_the_total() {
    parity("space", "space-table", &|| tree_with_shards(&[]));
}

#[test]
fn space_as_json() {
    parity("space", "space-json", &|| tree_with_shards(&["--json"]));
}

#[test]
fn space_of_an_empty_tree() {
    parity("space", "space-empty", &|| {
        let mut setup = Setup::new(git_fixture(), &[]);
        let tree = setup.scratch();
        setup.args = vec![tree.display().to_string()];
        setup
    });
}

#[test]
fn space_delete_with_yes_removes_the_shards() {
    parity("space", "space-delete-yes", &|| {
        tree_with_shards(&["--delete", "--yes"])
    });
}

#[test]
fn space_delete_without_a_yes_aborts_when_nothing_answers() {
    // Standard input is empty, which counts as a no.
    parity("space", "space-delete-abort", &|| {
        tree_with_shards(&["--delete"])
    });
}

#[test]
fn space_of_a_missing_path_is_an_error() {
    parity("space", "space-missing", &|| {
        Setup::new(git_fixture(), &["/nonexistent/path/for/space"])
    });
}
