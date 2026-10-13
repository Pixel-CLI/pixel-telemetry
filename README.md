# pixel-telemetry

The `telemetry` plugin for [Pixel](https://github.com/Pixel-CLI/pixel): the
commands that read what Pixel recorded about itself and its host, in one
`pixel-telemetry` binary. These five commands used to be built into `pixel`
(or, for `token-savings`, a first plugin in `Pixel-CLI/pixel-plugins`); they
live here so the core stays small and only the people who use them install
them. Core keeps *writing* its action log and the error store; this plugin is
the viewer.

| Command | What it does |
| --- | --- |
| `pixel list-errors` | one-look error capture: `last`, `since`, `show`, `cursor`, `gc`, and `run` (wrap a command, tee its output, record structured errors on failure) |
| `pixel record-event` | journal a session event (fire-and-forget) |
| `pixel action-log` | pixel's own action log: what ran, what went wrong; `--errors-only`, `--json`, `--clear` |
| `pixel token-savings` | snippet-vs-candidate-pool savings per retrieval command, from the action log |
| `pixel space` | disk taken by `.pixel/` index shards across every project under a tree; `--delete` reclaims it |

Each command reproduces the built-in's arguments, output and exit status.
`pixel-telemetry --help` lists them; `pixel-telemetry <command> --help` shows
one's options.

## Install

With a Pixel that has the plugin host (`pixel install <source>`):

```bash
pixel install github:Pixel-CLI/pixel-telemetry
```

The release assets are `pixel-telemetry-<target-triple>` executables, each with
a `.sha256` file: `aarch64-apple-darwin`, `x86_64-apple-darwin`,
`x86_64-unknown-linux-gnu` and `aarch64-unknown-linux-gnu`. The plugin
declares no network access.

From source, without Pixel's plugin management:

```bash
cargo install --git https://github.com/Pixel-CLI/pixel-telemetry pixel-telemetry-plugin
pixel-telemetry install-links   # makes `pixel list-errors`, `pixel space`, ... find it
```

## How `pixel <command>` finds this plugin

Plugin API 1 resolves `pixel list-errors` to a plugin *named* `list-errors`, or
to an executable `pixel-list-errors` on `PATH`. This repository is **one**
plugin named `telemetry` that answers to five names, so:

- `pixel telemetry <command> …` always works once the plugin is installed (the
  host runs `bin/pixel-telemetry`, which is, or finds, the `pixel-telemetry`
  binary).
- `pixel <command> …` works through `PATH` links. `pixel-telemetry
  install-links` creates `pixel-<command>` symlinks to `pixel-telemetry` next
  to it (or in `--dir <dir>`); the binary runs the command named by the name it
  was started under. `pixel-telemetry install-links --remove` deletes them. It
  never replaces a file it did not make.
- [`pixel-plugin.toml`](pixel-plugin.toml) lists the five names under
  `commands`. API 1 ignores that key. A host that reads it could route every
  listed name to `pixel-telemetry <name>` with no links at all;
  `tests/manifest.rs` keeps the list equal to what the binary has.

## Environment

The host passes `PIXEL_API`, `PIXEL_REPO_ROOT`, `PIXEL_GRAPH_DB` and
`PIXEL_BIN`; all are optional. The `[PATH]` argument of `action-log`,
`token-savings` and `record-event` defaults to `.`, and the default (or an
explicit `.`) is replaced by `PIXEL_REPO_ROOT`. Two commands keep `.` as the
caller's directory: `space` audits a *tree of projects*, which is rarely a
repository, and `list-errors` takes `--repo` and resolves the project root
itself (`PIXEL_SNIPER_STATE_ROOT` moves the error store, as for the built-in).

## Differences from the built-ins

- **No metrics line, no action-log entry.** A plugin is a separate process;
  the invocation accounting belongs to the `pixel` binary, so running one of
  these commands no longer adds a line to the log it reads. `--metrics on|off`
  is still accepted (hidden), so scripts that pass it keep parsing.
- **`record-event` runs the journal in this process.** Core asks the
  repository daemon first and falls back to an in-process `Service`; the
  plugin always uses the `Service` (the answer core gives when no daemon
  serves the request). It never starts or uses a daemon.
- **No old aliases.** `sniper` (`list-errors`), `log` (`action-log`) and
  `journal` (`record-event`) were clap aliases of the built-ins; the plugin
  answers to canonical names only.
- **`record-event` and `space`** were deleted from core (#898, `87e17b41`) and
  are restored from `87e17b41^`.

## Where the code comes from

Library crates of Pixel core (`pixel-actionlog`, `pixel-session`,
`pixel-daemon`, `pixel-index`, `pixel-proto`) come through a git dependency
pinned to one commit of [Pixel-CLI/pixel](https://github.com/Pixel-CLI/pixel),
recorded once in `[workspace.dependencies]` of [`Cargo.toml`](Cargo.toml)
(`scripts/check-pin.sh` enforces a single value). Logic that lived only in the
`pixel` binary crate is ported here, MIT, with its origin in each module's doc:

| Here | From |
| --- | --- |
| `commands/list_errors.rs` | `crates/pixel/src/sniper_cmd.rs` |
| `commands/action_log.rs` | `run_log` and helpers in `crates/pixel/src/main.rs` |
| `commands/token_savings.rs` | the savings report of `main.rs`, via Pixel-CLI/pixel-plugins |
| `commands/record_event.rs` | `main.rs` at `87e17b41^` |
| `commands/space.rs` | `crates/pixel/src/space_cmd.rs` at `87e17b41^` |
| `engine.rs` | `execute` and `unwrap_response` of `main.rs` |
| `env.rs`, `root.rs`, `output.rs`, `run.rs`, `links.rs` | as in Pixel-CLI/pixel-git |

## Versioning

One version (`[workspace.package]`), equal to the `version` in
`pixel-plugin.toml`; a release is a `vX.Y.Z` tag on `main`, which builds the
assets above. `api = 1` changes only when Pixel changes the plugin API. A bump
of the Pixel pin is a minor version when output changes, otherwise a patch.

## License

MIT, see [LICENSE](LICENSE).
