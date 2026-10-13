// SPDX-FileCopyrightText: The Pixel contributors
// SPDX-License-Identifier: MIT

//! How `pixel-telemetry` picks a command: by argument, by the name it was started
//! under, and what it says when neither names one.

use std::path::Path;

use pixel_telemetry_plugin::commands::COMMANDS;
use pixel_telemetry_plugin::testing::{git_fixture, run, write};

const BIN: &str = env!("CARGO_BIN_EXE_pixel-telemetry");

#[test]
fn help_lists_every_command() {
    let seen = run(BIN.as_ref(), &["--help"], Path::new("."), &[]);
    assert_eq!(seen.code, 0);
    for c in &COMMANDS {
        assert!(seen.stdout.contains(c.name), "{}", c.name);
    }
}

#[test]
fn the_version_is_the_package_version() {
    let seen = run(BIN.as_ref(), &["--version"], Path::new("."), &[]);
    assert_eq!(
        seen.stdout,
        format!("pixel-telemetry {}\n", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn an_unknown_command_is_a_usage_error() {
    let seen = run(BIN.as_ref(), &["nope"], Path::new("."), &[]);
    assert_eq!(seen.code, 2);
    assert!(
        seen.stderr.contains("unknown command `nope`"),
        "{}",
        seen.stderr
    );
}

#[test]
fn no_command_is_a_usage_error() {
    let seen = run(BIN.as_ref(), &[], Path::new("."), &[]);
    assert_eq!(seen.code, 2);
    assert!(
        seen.stderr.contains("Usage: pixel-telemetry <command>"),
        "{}",
        seen.stderr
    );
}

#[test]
fn every_command_answers_help_under_the_name_pixel_dash_command() {
    let repo = git_fixture();
    let links = tempfile::tempdir().unwrap();
    let seen = run(
        BIN.as_ref(),
        &["install-links", "--dir", links.path().to_str().unwrap()],
        repo.path(),
        &[],
    );
    assert_eq!(seen.code, 0, "{}", seen.stderr);
    for c in &COMMANDS {
        let link = links.path().join(format!("pixel-{}", c.name));
        let via_link = run(&link, &["--help"], repo.path(), &[]);
        let via_arg = run(BIN.as_ref(), &[c.name, "--help"], repo.path(), &[]);
        assert_eq!(via_link.code, 0, "{}: {}", c.name, via_link.stderr);
        assert_eq!(via_link, via_arg, "{}", c.name);
        assert!(
            via_link.stdout.contains(&format!("pixel {}", c.name)),
            "{}: {}",
            c.name,
            via_link.stdout
        );
    }
}

#[test]
fn a_command_run_through_its_link_works_on_the_repository() {
    let repo = git_fixture();
    let links = tempfile::tempdir().unwrap();
    let seen = run(
        BIN.as_ref(),
        &["install-links", "--dir", links.path().to_str().unwrap()],
        repo.path(),
        &[],
    );
    assert_eq!(seen.code, 0, "{}", seen.stderr);
    let log = pixel_actionlog::ActionLog::path_for_root(&repo.path().canonicalize().unwrap());
    write(repo.path(), ".pixel/actions.jsonl", "");
    let link = links.path().join("pixel-action-log");
    let seen = run(&link, &[], repo.path(), &[]);
    assert_eq!(seen.code, 0, "{}", seen.stderr);
    assert_eq!(
        seen.stdout,
        format!("no recorded actions at {}\n", log.display())
    );
}

#[test]
fn the_host_root_replaces_the_default_path() {
    // The host passes PIXEL_REPO_ROOT; run from elsewhere, `.` means that root.
    let repo = git_fixture();
    let elsewhere = tempfile::tempdir().unwrap();
    let root = repo.path().to_str().unwrap();
    let seen = run(
        BIN.as_ref(),
        &["action-log"],
        elsewhere.path(),
        &[("PIXEL_REPO_ROOT", root), ("PIXEL_API", "1")],
    );
    assert_eq!(seen.code, 0, "{}", seen.stderr);
    let log = pixel_actionlog::ActionLog::path_for_root(&repo.path().canonicalize().unwrap());
    assert_eq!(
        seen.stdout,
        format!("no recorded actions at {}\n", log.display())
    );
}

#[test]
fn install_links_refuses_to_replace_a_foreign_file() {
    let links = tempfile::tempdir().unwrap();
    std::fs::write(links.path().join("pixel-space"), "mine").unwrap();
    let seen = run(
        BIN.as_ref(),
        &["install-links", "--dir", links.path().to_str().unwrap()],
        Path::new("."),
        &[],
    );
    assert_eq!(seen.code, 1);
    assert!(
        seen.stderr.contains("pixel-space exists"),
        "{}",
        seen.stderr
    );
    assert_eq!(
        std::fs::read_to_string(links.path().join("pixel-space")).unwrap(),
        "mine"
    );
    assert!(links.path().join("pixel-list-errors").is_symlink());
}
