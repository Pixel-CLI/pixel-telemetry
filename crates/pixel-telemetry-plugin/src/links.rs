// SPDX-FileCopyrightText: The Pixel contributors
// SPDX-License-Identifier: MIT

//! `pixel-telemetry install-links`: make `pixel <command>` find this plugin.
//!
//! Pixel core resolves `pixel commit` to a plugin named `commit`, or to an
//! executable `pixel-list-errors` on `PATH`. This plugin is one binary, so the
//! second form needs one name per command: a symlink `pixel-<command>` to
//! `pixel-telemetry`, which dispatches on the name it was started under
//! ([`crate::command_of_program`]).

use std::ffi::OsString;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::commands::COMMANDS;

const USAGE: &str = "usage: pixel-telemetry install-links [--dir <dir>] [--remove]\n\
  Link pixel-<command> to this binary in <dir> (default: the directory of this binary).\n\
  --remove deletes the links that point to it.\n";

/// What `install-links` does to one name.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Linked,
    AlreadyLinked,
    Removed,
    Absent,
    /// Something else is there; it is left alone.
    Occupied,
}

/// Link (or unlink) every command in `dir` to `exe`. The link target is the
/// bare file name when `exe` sits in `dir`, so the directory can move.
pub fn sync(dir: &Path, exe: &Path, remove: bool) -> Result<Vec<(&'static str, Outcome)>, String> {
    let exe = exe
        .canonicalize()
        .map_err(|e| format!("cannot resolve {}: {e}", exe.display()))?;
    let dir_real = dir
        .canonicalize()
        .map_err(|e| format!("cannot resolve {}: {e}", dir.display()))?;
    let target: PathBuf = if exe.parent() == Some(dir_real.as_path()) {
        PathBuf::from(exe.file_name().ok_or("the binary has no file name")?)
    } else {
        exe.clone()
    };
    let mut out = Vec::new();
    for c in &COMMANDS {
        let link = dir.join(format!("pixel-{}", c.name));
        let points_here = std::fs::read_link(&link)
            .ok()
            .is_some_and(|t| dir_real.join(&t).canonicalize().ok().as_deref() == Some(&exe));
        let exists = std::fs::symlink_metadata(&link).is_ok();
        let outcome = match (remove, exists, points_here) {
            (true, false, _) => Outcome::Absent,
            (true, true, true) => {
                std::fs::remove_file(&link)
                    .map_err(|e| format!("cannot remove {}: {e}", link.display()))?;
                Outcome::Removed
            }
            (false, true, true) => Outcome::AlreadyLinked,
            (_, true, false) => Outcome::Occupied,
            (false, false, _) => {
                symlink(&target, &link)
                    .map_err(|e| format!("cannot link {}: {e}", link.display()))?;
                Outcome::Linked
            }
        };
        out.push((c.name, outcome));
    }
    Ok(out)
}

pub fn run(args: &[OsString]) -> ExitCode {
    let mut dir: Option<PathBuf> = None;
    let mut remove = false;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.to_str() {
            Some("--remove") => remove = true,
            Some("--dir") => match it.next() {
                Some(value) => dir = Some(PathBuf::from(value)),
                None => {
                    eprint!("pixel-telemetry: --dir needs a value\n{USAGE}");
                    return ExitCode::from(2);
                }
            },
            Some("-h" | "--help") => {
                print!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            _ => {
                eprint!("pixel-telemetry: unexpected argument {arg:?}\n{USAGE}");
                return ExitCode::from(2);
            }
        }
    }
    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(e) => {
            eprintln!("pixel-telemetry: cannot find this binary: {e}");
            return ExitCode::FAILURE;
        }
    };
    let dir = dir.unwrap_or_else(|| {
        exe.parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
    });
    match sync(&dir, &exe, remove) {
        Ok(rows) => {
            let mut occupied = 0;
            for (name, outcome) in &rows {
                if *outcome == Outcome::Occupied {
                    occupied += 1;
                    eprintln!(
                        "pixel-telemetry: {}/pixel-{name} exists and is not a link to pixel-telemetry; left alone",
                        dir.display()
                    );
                }
            }
            let count = |wanted: Outcome| rows.iter().filter(|(_, o)| *o == wanted).count();
            println!(
                "{}: {} linked, {} already linked, {} removed, {} left alone",
                dir.display(),
                count(Outcome::Linked),
                count(Outcome::AlreadyLinked),
                count(Outcome::Removed),
                occupied
            );
            if occupied > 0 {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(e) => {
            eprintln!("pixel-telemetry: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_exe(dir: &Path) -> PathBuf {
        let exe = dir.join("pixel-telemetry");
        std::fs::write(&exe, "#!/bin/sh\n").unwrap();
        exe
    }

    #[test]
    fn it_links_every_command_once_and_removes_only_its_own() {
        let dir = tempfile::tempdir().unwrap();
        let exe = fake_exe(dir.path());
        let rows = sync(dir.path(), &exe, false).unwrap();
        assert_eq!(rows.len(), COMMANDS.len());
        assert!(rows.iter().all(|(_, o)| *o == Outcome::Linked));
        assert_eq!(
            std::fs::read_link(dir.path().join("pixel-list-errors")).unwrap(),
            Path::new("pixel-telemetry")
        );
        // Second run: nothing to do.
        assert!(
            sync(dir.path(), &exe, false)
                .unwrap()
                .iter()
                .all(|(_, o)| *o == Outcome::AlreadyLinked)
        );
        // A foreign file is never replaced or removed.
        std::fs::remove_file(dir.path().join("pixel-space")).unwrap();
        std::fs::write(dir.path().join("pixel-space"), "mine").unwrap();
        let rows = sync(dir.path(), &exe, true).unwrap();
        let state = |n: &str| &rows.iter().find(|(c, _)| *c == n).unwrap().1;
        assert_eq!(*state("space"), Outcome::Occupied);
        assert_eq!(*state("list-errors"), Outcome::Removed);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("pixel-space")).unwrap(),
            "mine"
        );
        assert!(!dir.path().join("pixel-list-errors").exists());
    }

    #[test]
    fn a_link_elsewhere_points_at_the_absolute_binary() {
        let bin = tempfile::tempdir().unwrap();
        let links = tempfile::tempdir().unwrap();
        let exe = fake_exe(bin.path());
        sync(links.path(), &exe, false).unwrap();
        assert_eq!(
            std::fs::read_link(links.path().join("pixel-action-log")).unwrap(),
            exe.canonicalize().unwrap()
        );
    }
}
