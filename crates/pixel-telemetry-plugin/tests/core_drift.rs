// SPDX-FileCopyrightText: The Pixel contributors
// SPDX-License-Identifier: MIT

//! What this repository restates from Pixel core must still agree with the
//! pinned commit. A pin bump that moves either fails here, not in a user's
//! shell.

use pixel_telemetry_plugin::run::failure_line;
use serde_json::Value;

#[test]
fn the_graph_file_name_is_cores() {
    assert_eq!(
        pixel_telemetry_plugin::GRAPH_DB_FILE,
        pixel_daemon::api::GRAPH_DB_FILE
    );
}

#[test]
fn the_failure_envelope_is_the_one_core_prints() {
    for message in [
        "bad path /nowhere: No such file or directory",
        "NON_FAST_FORWARD: merge-base x is not an ancestor of y",
        "STALE_STATE: expected head a, found b",
        "REF_EXISTS: branch already exists",
        "GIT_FAILED: boom",
        "REFUSED: something",
        "no symbol named foo",
        "repository is busy: lock held",
    ] {
        let ours: Value = serde_json::from_str(failure_line("op", message).trim_end()).unwrap();
        let theirs =
            serde_json::to_value(pixel_daemon::api::failure_response("op", message)).unwrap();
        assert_eq!(ours, theirs, "{message}");
    }
}
