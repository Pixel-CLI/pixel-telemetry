# Contributing

The `telemetry` plugin of [Pixel](https://github.com/Pixel-CLI/pixel). Read the
[README](README.md) first.

## Before a pull request

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
scripts/check-pin.sh
```

CI runs the same on Ubuntu and macOS. Every source file opens with
`SPDX-FileCopyrightText: The Pixel contributors` and
`SPDX-License-Identifier: MIT`.

## A command keeps the built-in's behaviour

A command reproduces the one it replaced: same arguments, same output, same
exit status. `crates/pixel-telemetry-plugin/tests/parity_*.rs` run
`pixel-telemetry` on a temporary git repository (`testing::git_fixture`) and
compare the result with a recorded golden in `tests/golden/`. Goldens are
recorded from the **built-in** command, never from this plugin. Object ids,
timestamps, run ids, pids and relative times are scrubbed; directories become
`ROOT` (the repository) or `EXTRA` (a scratch directory).

Two cores recorded the goldens, because the five commands left core at
different times:

| Test file | Commands | Recorded from |
| --- | --- | --- |
| `parity_current.rs` | `list-errors`, `action-log` | `ad788194` (`origin/main` when they were ported) |
| `parity_restored.rs` | `token-savings`, `record-event`, `space` | `dcbe7156` (`87e17b41^`), the last core that has them |

Re-record after a change, or after moving the pin, with a `pixel` binary that
still has the command (`cargo build -p pixel-cli --no-default-features
--features model2vec` in a checkout of that commit; name the binary `pixel`,
its usage lines print the name it was started under):

```bash
PIXEL_CORE_BIN=/path/to/pixel UPDATE_GOLDEN=1 cargo test --test parity_current
git diff crates/pixel-telemetry-plugin/tests/golden   # every change is a behaviour change: explain it
```

Running a test file with `PIXEL_CORE_BIN` set and without `UPDATE_GOLDEN` also
compares the plugin with that live built-in. Against the wrong core, the
commands that left it fail by design.

A deliberate difference from the built-in has no golden: test it directly and
list it under "Differences from the built-ins" in the README.

## Moving the Pixel pin

The pinned commit lives only in `[workspace.dependencies]` of `Cargo.toml`.
Change `rev` on every entry together, run `cargo update`, then
`cargo test --workspace`. `tests/core_drift.rs` fails when something this
repository restates (the graph file name, the failure-envelope classifier) no
longer matches core.

## Adding a command

1. A `Parser` struct and an entry function in `src/commands/`, with
   `#[command(name = "<name>", bin_name = "pixel <name>")]` and
   `#[command(flatten)] _host: HostFlags`. Run it through `run::run`, resolve
   the `[PATH]` argument with `env::resolve_path`, print with `output::print_data`.
2. One row in `COMMANDS` (`src/commands/mod.rs`) and one name in the `commands`
   list of `pixel-plugin.toml`; `tests/manifest.rs` compares the two.
3. `tests/parity_*.rs` cases and their goldens.
4. When logic lives only inside the `pixel` binary crate, port it (copy, keep
   the SPDX header, say where it came from in the module doc) and list it in
   the README's "Where the code comes from" table.

## Releasing

Bump `version` in `Cargo.toml` (`[workspace.package]`) and `pixel-plugin.toml`
together, merge to `main`, then tag `vX.Y.Z` on `main`. `release.yml` checks
the tag against both files, creates the release and uploads
`pixel-telemetry-<target-triple>` with its `.sha256` for four targets.
