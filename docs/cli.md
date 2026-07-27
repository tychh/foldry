# Foldry CLI v1

The executable is named `foldry`. Commands and machine-facing values are English.
Run `foldry <command> --help` for the complete flag list.

Build the standalone binary with:

```bash
cargo build --release -p foldry-cli --bin foldry
```

Release candidates publish it separately from the desktop installer, so scripts do
not need to extract a GUI bundle.

## Commands

```text
foldry profile list
foldry profile show <profile-id>
foldry profile create --name <name> [--filename <file.packignore>]
foldry profile edit <profile-id> --from <file>
foldry profile delete <profile-id>
foldry profile validate <file>

foldry preset list
foldry preset install <preset-id>
foldry preset remove <preset-id>

foldry preview <source> [--profile <id-or-filename>]
foldry archive <source> [archive options]

foldry plan validate [plan.packplan.yaml]
foldry plan run [plan.packplan.yaml]

foldry history list [--limit <n>] [--offset <n>]
foldry history show <run-id>

foldry config show
foldry config path
```

`archive` supports ZIP, TAR.GZ, and TAR.ZST; semantic compression levels; conflict
policies `skip`, `overwrite`, and `increment`; root flattening; full verification;
and SHA-256. Defaults come from `settings.yaml`.

`plan run` executes enabled tasks through the same scheduler and archive runner as a
single `archive` command. Profile selection uses an explicit ID/filename, then the
configured default, then the shipped Default working copy.

## Human, TTY, and JSON output

Human output is the default. When stderr is a terminal, run state and bounded
progress are printed while an archive is running. Redirected/CI execution stays in
quiet line mode and prints only final results and errors.

`--json` suppresses progress and emits exactly one object:

```json
{ "version": 1, "ok": true, "data": {} }
```

Errors use:

```json
{
  "version": 1,
  "ok": false,
  "error": { "code": "validation_error", "message": "..." }
}
```

JSON field names and enum values are stable English machine contracts.

## Exit codes

|  Code | Meaning                                      |
| ----: | -------------------------------------------- |
|   `0` | Success                                      |
|   `2` | Command-line usage error                     |
|   `3` | Validation or unknown identifier             |
|   `4` | I/O or complete execution failure            |
|   `5` | Partial success or success with warnings     |
|   `6` | Configuration/persistence incompatibility    |
|  `10` | Internal scheduler/serialization failure     |
| `130` | Cancelled by Ctrl+C through cooperative stop |

Ctrl+C uses the scheduler's normal stop path. Existing archives remain untouched,
and owned temporary/reservation files are removed.

## Development and recovery overrides

Hidden global flags `--config-dir`, `--data-dir`, and `--cache-dir` support tests,
development, and recovery. They do not constitute a supported portable mode.
`FOLDRY_RESOURCE_DIR` may point development or packaged builds at the resource
directory.
