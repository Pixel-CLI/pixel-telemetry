// SPDX-FileCopyrightText: The Pixel contributors
// SPDX-License-Identifier: MIT

//! Test support for the command tests (feature `testing`).
//!
//! Each command's tests run `pixel-telemetry` against a temporary git repository and
//! check it two ways:
//!
//! - against a **golden** recorded from the built-in command of Pixel core at
//!   the pinned commit, so CI needs no core build;
//! - against the **live** built-in when `PIXEL_CORE_BIN` names a core `pixel`
//!   binary (see CONTRIBUTING.md), which is how the goldens are re-recorded
//!   after a pin bump.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

/// What a finished command left behind, reduced to what a user sees.
#[derive(Debug, PartialEq, Eq)]
pub struct Seen {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl From<Output> for Seen {
    fn from(out: Output) -> Self {
        Seen {
            code: out.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        }
    }
}

/// The author and committer date of every commit the fixtures make.
const FIXTURE_DATE: &str = "2026-01-02T03:04:05+00:00";

/// Run `git` in `dir`, failing the test on a non-zero exit.
pub fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.name=Plugin Test",
            "-c",
            "user.email=test@example.com",
        ])
        .args([
            "-c",
            "commit.gpgsign=false",
            "-c",
            "init.defaultBranch=main",
        ])
        .args(args)
        // Fixed commit dates make every id and timestamp reproducible.
        .env("GIT_AUTHOR_DATE", FIXTURE_DATE)
        .env("GIT_COMMITTER_DATE", FIXTURE_DATE)
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// A git repository with a few commits on `main`, a `feature` branch two
/// commits ahead of it and a Rust and a TypeScript file, in a temp directory.
/// The fixture is deterministic apart from commit ids and the directory name.
pub fn git_fixture() -> TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path();
    git(path, &["init", "-q"]);
    // Commands under test commit (squash-branch) and CI has no global identity.
    git(path, &["config", "user.name", "Plugin Test"]);
    git(path, &["config", "user.email", "test@example.com"]);
    git(path, &["config", "commit.gpgsign", "false"]);
    // Core's own commands write `.pixel/` (the action log) into the repo they
    // run in; ignored here so a built-in and a plugin see the same worktree.
    write(path, ".git/info/exclude", ".pixel/\n");
    write(path, "src/lib.rs", "pub fn alpha() -> u32 {\n    1\n}\n");
    write(path, "web/app.ts", "export const beta = 2;\n");
    write(path, "README.md", "# fixture\n");
    git(path, &["add", "."]);
    git(path, &["commit", "-qm", "initial"]);
    git(path, &["checkout", "-qb", "feature"]);
    write(path, "src/lib.rs", "pub fn alpha() -> u32 {\n    2\n}\n");
    git(path, &["commit", "-qam", "change alpha"]);
    write(path, "src/extra.rs", "pub fn gamma() {}\n");
    git(path, &["add", "."]);
    git(path, &["commit", "-qm", "add gamma"]);
    dir
}

/// A bare repository in `scratch` registered as `origin` of `repo`, with
/// `main` and `feature` already pushed to it.
pub fn add_origin(repo: &Path, scratch: &Path) {
    let bare = scratch.join("origin.git");
    git(
        scratch,
        &["init", "-q", "--bare", bare.to_str().expect("utf-8")],
    );
    git(
        repo,
        &["remote", "add", "origin", bare.to_str().expect("utf-8")],
    );
    git(repo, &["push", "-q", "origin", "main", "feature"]);
    git(repo, &["fetch", "-q", "origin"]);
    git(
        repo,
        &[
            "branch",
            "-q",
            "--set-upstream-to=origin/feature",
            "feature",
        ],
    );
}

/// Write `contents` at `rel` under `dir`, creating directories.
pub fn write(dir: &Path, rel: &str, contents: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    std::fs::write(path, contents).expect("write");
}

/// The core `pixel` binary named by `PIXEL_CORE_BIN`, when set.
pub fn core_bin() -> Option<PathBuf> {
    std::env::var_os("PIXEL_CORE_BIN")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

/// The machine-wide state directory every command under test shares:
/// `pixel-ops` keeps its idempotency records in `$XDG_STATE_HOME/pixel`, and a
/// test must neither read nor write the user's.
fn state_home() -> &'static Path {
    static STATE: std::sync::OnceLock<TempDir> = std::sync::OnceLock::new();
    STATE
        .get_or_init(|| tempfile::tempdir().expect("tempdir"))
        .path()
}

/// Run `bin args…` in `cwd` with the host variables cleared, plus `envs`.
pub fn run(bin: &Path, args: &[&str], cwd: &Path, envs: &[(&str, &str)]) -> Seen {
    let mut command = Command::new(bin);
    command
        .args(args)
        .current_dir(cwd)
        .env_remove("PIXEL_REPO_ROOT")
        .env_remove("PIXEL_GRAPH_DB")
        .env_remove("PIXEL_BIN")
        .env_remove("PIXEL_API")
        .env("XDG_STATE_HOME", state_home())
        .env("PIXEL_METRICS", "0");
    for (key, value) in envs {
        command.env(key, value);
    }
    command.output().expect("command runs").into()
}

// ---------------------------------------------------------------------------
// Parity with the built-in command
// ---------------------------------------------------------------------------

/// One invocation to compare, built fresh for each side so a command that
/// mutates its repository runs against an untouched copy both times.
pub struct Setup {
    /// The working directory (usually the fixture repository).
    pub dir: TempDir,
    /// `before` commands (arguments after `pixel <command>`), run first with
    /// the same implementation, e.g. `flow save …` ahead of `flow get …`.
    /// The flag says the step is expected to fail (a wrapped command that
    /// exits non-zero on purpose).
    pub before: Vec<(Vec<String>, bool)>,
    /// The arguments after `pixel <command>`.
    pub args: Vec<String>,
    /// Extra environment, e.g. `PIXEL_FLOW_DIR`.
    pub envs: Vec<(String, String)>,
    /// Other directories whose paths must be hidden from the comparison.
    pub scrub_paths: Vec<PathBuf>,
    /// Extra volatile text to hide: `(regex, replacement)` pairs applied to
    /// both the plugin's and the built-in's rendering.
    pub scrub_patterns: Vec<(String, String)>,
    /// Scratch directories kept alive until the setup is dropped.
    keep: Vec<TempDir>,
}

impl Setup {
    /// `args` run in `dir`, with nothing before them.
    pub fn new(dir: TempDir, args: &[&str]) -> Self {
        Setup {
            dir,
            before: Vec::new(),
            args: args.iter().map(ToString::to_string).collect(),
            envs: Vec::new(),
            scrub_paths: Vec::new(),
            scrub_patterns: Vec::new(),
            keep: Vec::new(),
        }
    }

    /// A fresh scratch directory that lives as long as the setup and is
    /// hidden from the comparison.
    pub fn scratch(&mut self) -> PathBuf {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().to_path_buf();
        self.scrub_paths.push(path.clone());
        self.keep.push(dir);
        path
    }

    /// A command to run first; it must succeed.
    pub fn then(mut self, before: &[&str]) -> Self {
        self.before
            .push((before.iter().map(ToString::to_string).collect(), false));
        self
    }

    /// A command to run first that is expected to exit non-zero.
    pub fn then_failing(mut self, before: &[&str]) -> Self {
        self.before
            .push((before.iter().map(ToString::to_string).collect(), true));
        self
    }

    /// An environment variable for every command of this setup.
    pub fn env(mut self, key: &str, value: impl AsRef<Path>) -> Self {
        self.envs
            .push((key.to_string(), value.as_ref().display().to_string()));
        self
    }

    /// A directory to hide from the comparison.
    pub fn scrub(mut self, path: &Path) -> Self {
        self.scrub_paths.push(path.to_path_buf());
        self
    }

    /// Volatile text to hide: every match of `pattern` (a regex) becomes
    /// `replacement`.
    pub fn scrub_pattern(mut self, pattern: &str, replacement: &str) -> Self {
        self.scrub_patterns
            .push((pattern.to_string(), replacement.to_string()));
        self
    }
}

/// Replace every run of 7 or more lowercase hex digits that stands alone
/// (object ids, short ids, unix timestamps) with `OID`.
pub fn scrub_ids(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let is_hex = |c: char| c.is_ascii_digit() || ('a'..='f').contains(&c);
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let standalone_start = i == 0 || !chars[i - 1].is_ascii_alphanumeric();
        if is_hex(c) && standalone_start {
            let mut j = i;
            while j < chars.len() && is_hex(chars[j]) {
                j += 1;
            }
            let standalone_end = j == chars.len() || !chars[j].is_ascii_alphanumeric();
            if j - i >= 7 && standalone_end {
                out.push_str("OID");
                i = j;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

/// What a user saw, with the volatile parts replaced: each directory by a
/// label, ids and timestamps by `OID`, blank stderr lines dropped.
fn render(seen: &Seen, setup: &Setup) -> String {
    let mut text = format!(
        "exit: {}\n--- stdout\n{}--- stderr\n{}",
        seen.code,
        seen.stdout,
        seen.stderr
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| format!("{line}\n"))
            .collect::<String>()
    );
    let mut dirs: Vec<(&Path, &str)> = vec![(setup.dir.path(), "ROOT")];
    dirs.extend(
        setup
            .scrub_paths
            .iter()
            .map(|path| (path.as_path(), "EXTRA")),
    );
    for (dir, label) in dirs {
        for form in [dir.canonicalize().ok(), Some(dir.to_path_buf())]
            .into_iter()
            .flatten()
        {
            text = text.replace(&form.display().to_string(), label);
        }
    }
    let mut text = scrub_snapshot_ids(&scrub_ids(&text));
    for (pattern, replacement) in &setup.scrub_patterns {
        let re = regex::Regex::new(pattern).expect("scrub pattern");
        text = re.replace_all(&text, replacement.as_str()).into_owned();
    }
    text
}

/// Replace every snapshot id (`20261013T232719.415573000Z`, a UTC timestamp to
/// the nanosecond) with `SNAPSHOT`.
pub fn scrub_snapshot_ids(text: &str) -> String {
    const SHAPE: &[u8] = b"ddddddddTdddddd.dddddddddZ";
    let bytes = text.as_bytes();
    let fits = |at: usize| {
        bytes.len() >= at + SHAPE.len()
            && SHAPE.iter().enumerate().all(|(i, shape)| match shape {
                b'd' => bytes[at + i].is_ascii_digit(),
                other => bytes[at + i] == *other,
            })
    };
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < bytes.len() {
        if fits(i) {
            out.push_str("SNAPSHOT");
            i += SHAPE.len();
        } else {
            let ch = text[i..].chars().next().expect("char boundary");
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

/// Run `bin` through a [`Setup`]; `prefix` is the subcommand words in front
/// of the arguments: `["commit"]` for both `pixel commit` and `pixel-telemetry commit`.
fn execute(bin: &Path, prefix: &[&str], setup: &Setup) -> String {
    let envs: Vec<(&str, &str)> = setup
        .envs
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    for (before, may_fail) in &setup.before {
        let args: Vec<&str> = prefix
            .iter()
            .copied()
            .chain(before.iter().map(String::as_str))
            .collect();
        let seen = run(bin, &args, setup.dir.path(), &envs);
        if *may_fail {
            assert_ne!(seen.code, 0, "setup step {before:?} should have failed");
        } else {
            assert_eq!(
                seen.code, 0,
                "setup step {before:?} failed: {}",
                seen.stderr
            );
        }
    }
    let args: Vec<&str> = prefix
        .iter()
        .copied()
        .chain(setup.args.iter().map(String::as_str))
        .collect();
    render(&run(bin, &args, setup.dir.path(), &envs), setup)
}

/// The parity check of one case.
///
/// `command` is the built-in's subcommand name. `make` builds a fresh
/// [`Setup`]; it is called once for the plugin and once for the built-in.
///
/// - the plugin's rendering is compared with `tests/golden/<case>.txt`;
/// - with `PIXEL_CORE_BIN` set it is also compared with the live built-in;
/// - with `UPDATE_GOLDEN=1` (which requires `PIXEL_CORE_BIN`) the golden is
///   rewritten from the **built-in**'s rendering, then compared as usual.
pub fn assert_parity(
    manifest_dir: &str,
    plugin_bin: &str,
    command: &str,
    case: &str,
    make: &dyn Fn() -> Setup,
) {
    let plugin = execute(Path::new(plugin_bin), &[command], &make());
    let golden_path = Path::new(manifest_dir)
        .join("tests/golden")
        .join(format!("{case}.txt"));
    let core = core_bin().map(|bin| execute(&bin, &[command], &make()));
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        let core = core.as_ref().expect("UPDATE_GOLDEN needs PIXEL_CORE_BIN");
        std::fs::create_dir_all(golden_path.parent().expect("parent")).expect("mkdir");
        std::fs::write(&golden_path, core).expect("write golden");
    }
    let golden = std::fs::read_to_string(&golden_path).unwrap_or_else(|_| {
        panic!(
            "missing golden {} (record it: PIXEL_CORE_BIN=<core pixel> UPDATE_GOLDEN=1 cargo test)",
            golden_path.display()
        )
    });
    assert_eq!(
        plugin, golden,
        "{case}: plugin differs from the recorded built-in"
    );
    if let Some(core) = core {
        assert_eq!(
            plugin, core,
            "{case}: plugin differs from the live built-in"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_ids_become_a_label() {
        assert_eq!(
            scrub_snapshot_ids("{\"id\":\"20261013T232719.415573000Z\",\"é\":1}"),
            "{\"id\":\"SNAPSHOT\",\"é\":1}"
        );
        assert_eq!(scrub_snapshot_ids("20261013T2327.4Z"), "20261013T2327.4Z");
    }

    #[test]
    fn standalone_hex_runs_of_seven_or_more_become_oid() {
        assert_eq!(
            scrub_ids("at 59cbad6 and 59cbad66bb53960ff2b9719a24b969586680b3e5."),
            "at OID and OID."
        );
        assert_eq!(scrub_ids("t=1791651110;"), "t=OID;");
        // Words and short numbers stay.
        assert_eq!(
            scrub_ids("facade 1234567x abc 123456"),
            "facade 1234567x abc 123456"
        );
    }
}
