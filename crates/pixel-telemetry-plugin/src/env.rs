// SPDX-FileCopyrightText: The Pixel contributors
// SPDX-License-Identifier: MIT

//! The environment Pixel core gives a plugin (API 1).
//!
//! | Variable | Meaning |
//! | --- | --- |
//! | `PIXEL_API` | plugin API version the host speaks (`1`) |
//! | `PIXEL_REPO_ROOT` | repository root of the caller's working directory |
//! | `PIXEL_GRAPH_DB` | the graph store: a path, or a bare file name under `.pixel/` |
//! | `PIXEL_BIN` | path of the core `pixel` binary that started the plugin |
//!
//! Every variable is optional: run as a bare `pixel-<name>` (or by
//! `cargo install`), a command behaves as the built-in command did, from the
//! current directory.

use std::path::{Path, PathBuf};

use pixel_index::index::SHARD_DIR;

use crate::GRAPH_DB_FILE;

/// A non-empty environment variable, read through `get` so the rule is
/// testable without touching the process environment.
fn non_empty(get: &dyn Fn(&str) -> Option<String>, name: &str) -> Option<String> {
    get(name).filter(|value| !value.is_empty())
}

/// The repository root the host resolved, if it passed one.
pub fn repo_root() -> Option<PathBuf> {
    repo_root_from(&|name| std::env::var(name).ok())
}

fn repo_root_from(get: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    non_empty(get, "PIXEL_REPO_ROOT").map(PathBuf::from)
}

/// The `[PATH]` argument of a command, with the host's root standing in for
/// the default.
///
/// Every ported command declared `#[arg(default_value = ".")] path`. Run from
/// the caller's directory, `.` meant "the repository around the current
/// directory", which is what the host's `PIXEL_REPO_ROOT` already is, so the
/// default is swapped for it and an explicit path is left alone. Passing `.`
/// explicitly is the same request as the default and is treated the same.
pub fn resolve_path(path: &Path) -> PathBuf {
    resolve_path_with(path, repo_root())
}

fn resolve_path_with(path: &Path, host_root: Option<PathBuf>) -> PathBuf {
    match host_root {
        Some(root) if path == Path::new(".") => root,
        _ => path.to_path_buf(),
    }
}

/// Where the graph store of `root` lives: `PIXEL_GRAPH_DB` when the host
/// passed one (absolute, or relative to `root/.pixel`), else the file core
/// names.
pub fn graph_db(root: &Path) -> PathBuf {
    graph_db_from(root, &|name| std::env::var(name).ok())
}

fn graph_db_from(root: &Path, get: &dyn Fn(&str) -> Option<String>) -> PathBuf {
    match non_empty(get, "PIXEL_GRAPH_DB") {
        Some(value) => {
            let given = PathBuf::from(value);
            if given.is_absolute() {
                given
            } else {
                root.join(SHARD_DIR).join(given)
            }
        }
        None => root.join(SHARD_DIR).join(GRAPH_DB_FILE),
    }
}

/// The core `pixel` binary, for a plugin that has to ask core for something:
/// the host's `PIXEL_BIN`, else `pixel` from `PATH`.
pub fn pixel_bin() -> PathBuf {
    non_empty(&|name| std::env::var(name).ok(), "PIXEL_BIN")
        .map_or_else(|| "pixel".into(), PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_of<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| (*value).to_string())
        }
    }

    #[test]
    fn the_default_path_becomes_the_host_root_and_an_explicit_one_stays() {
        let host = Some(PathBuf::from("/repo"));
        assert_eq!(
            resolve_path_with(Path::new("."), host.clone()),
            Path::new("/repo")
        );
        assert_eq!(resolve_path_with(Path::new("sub"), host), Path::new("sub"));
        assert_eq!(resolve_path_with(Path::new("."), None), Path::new("."));
    }

    #[test]
    fn an_empty_variable_is_unset() {
        assert_eq!(repo_root_from(&env_of(&[("PIXEL_REPO_ROOT", "")])), None);
        assert_eq!(
            repo_root_from(&env_of(&[("PIXEL_REPO_ROOT", "/r")])),
            Some(PathBuf::from("/r"))
        );
    }

    #[test]
    fn the_graph_db_is_a_path_a_bare_name_or_core_default() {
        let root = Path::new("/r");
        assert_eq!(
            graph_db_from(root, &env_of(&[("PIXEL_GRAPH_DB", "/x/g.db")])),
            Path::new("/x/g.db")
        );
        assert_eq!(
            graph_db_from(root, &env_of(&[("PIXEL_GRAPH_DB", "g.db")])),
            Path::new("/r/.pixel/g.db")
        );
        assert_eq!(
            graph_db_from(root, &env_of(&[])),
            Path::new("/r/.pixel/graph.v2.db")
        );
    }
}
