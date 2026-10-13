// SPDX-FileCopyrightText: The Pixel contributors
// SPDX-License-Identifier: MIT

//! Repository root discovery.
//!
//! Ported from `discover_root` in `crates/pixel/src/main.rs` (Pixel-CLI/pixel,
//! MIT): that function is private to the `pixel` binary crate.

use std::path::{Path, PathBuf};

use pixel_index::index::{SHARD_DIR, SHARD_FILE};

/// The repository containing `path`.
///
/// The nearest `.git` defines the boundary and always wins: a nested `.pixel`
/// left behind by indexing a subdirectory must never shadow the real root.
/// `.pixel` anchors a non-git tree only when it holds a shard: a journal-only
/// `.pixel` (the global `$HOME/.pixel` state directory) must never anchor, or
/// every gitless invocation below it would re-root to that ancestor.
pub fn discover_root(path: &Path) -> Result<PathBuf, String> {
    let abs = path
        .canonicalize()
        .map_err(|e| format!("bad path {}: {e}", path.display()))?;
    let start = if abs.is_file() {
        abs.parent().map_or_else(|| abs.clone(), Path::to_path_buf)
    } else {
        abs.clone()
    };
    let mut nearest_index: Option<PathBuf> = None;
    let mut cur = start.clone();
    loop {
        if cur.join(".git").exists() {
            return Ok(cur);
        }
        if nearest_index.is_none() && cur.join(SHARD_DIR).join(SHARD_FILE).is_file() {
            nearest_index = Some(cur.clone());
        }
        match cur.parent() {
            Some(parent) => cur = parent.to_path_buf(),
            None => return Ok(nearest_index.unwrap_or(start)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_nearest_git_directory_is_the_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::create_dir_all(root.join("a/b")).unwrap();
        assert_eq!(discover_root(&root.join("a/b")).unwrap(), root);
    }

    #[test]
    fn a_missing_path_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let err = discover_root(&dir.path().join("nope")).unwrap_err();
        assert!(err.starts_with("bad path"), "{err}");
    }
}
