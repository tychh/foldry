# Internal CLI reference

The `foldry-cli` crate is a development and automated-testing adapter. Version
0.1.2 does not include it in desktop installers or publish it as a standalone
download, and its command surface is not yet a supported public interface or
compatibility promise.

`foldry` exposes the same application services as the desktop interface. Commands
and machine-facing values are English.

```bash
cargo run -p foldry-cli --bin foldry -- --help
cargo run -p foldry-cli --bin foldry -- <command> --help
```

The shorter `foldry` form used below assumes a locally built development binary.

## One-shot archive

The shortest useful workflow does not modify the remembered-folder list:

```bash
foldry archive ./project \
  --profile Default \
  --output ./packages \
  --name project-backup \
  --format tar-zst \
  --compression balanced \
  --conflict increment \
  --full-verify \
  --checksum
```

`--profile` accepts a profile UUID, profile file name, or exact profile name.
Output defaults to the source folder's parent. The archive name defaults to the
source folder name.

Use `--no-include-root` to place source contents directly at the archive root.

## Configured workflow

Add a folder and inspect the generated Archive action:

```bash
foldry folder add ./project --profile Default
foldry folder list
foldry action list <folder-id>
foldry preview <folder-id> <action-id>
foldry action enable <folder-id> <action-id>
foldry action run <folder-id> <action-id>
```

Group runs:

```bash
foldry run folder <folder-id>
foldry run all
```

`run folder` runs enabled actions for the selected folder even if the folder is
disabled. `run all` includes only listed, enabled folders and enabled actions.

## Folders and actions

```text
foldry folder list
foldry folder add <source> [--profile <selector>]
foldry folder unlist <folder-id>
foldry folder remembered
foldry folder forget <folder-id>...
foldry folder enable|disable <folder-id>

foldry action list <folder-id>
foldry action add <folder-id> [--enabled] [--profile <selector>]
foldry action remove <folder-id> <action-id>
foldry action update <folder-id> <action-id> --from <action.json>
foldry action enable|disable <folder-id> <action-id>
foldry action run <folder-id> <action-id>
foldry action reorder <folder-id> <action-id>...
```

Unlisting a folder keeps its current configuration. Forgetting permanently removes
an unlisted configuration; historical runs remain.

`action update` accepts the complete `FolderAction` object returned in JSON mode.
The file must preserve its action ID and unknown extension fields.

## Profiles and presets

```text
foldry profile list
foldry profile show <profile-id>
foldry profile create --name <name> [--filename <file.packignore>]
foldry profile edit <profile-id> --from <file.packignore>
foldry profile delete <profile-id>
foldry profile validate <file.packignore>

foldry preset list
foldry preset install <preset-id>
foldry preset remove <preset-id>
```

Editing a profile requires a complete `.packignore` file that preserves its
`@profile-id`.

## History

```text
foldry history list [--folder <id>] [--action <id>] [--limit <n>] [--offset <n>]
foldry history show <run-id>
foldry history logs <run-id> [--limit <n>] [--offset <n>]
foldry run repeat <run-id>
```

Repeat uses the immutable historical source, action, profile text, and settings
snapshot.

## Configuration paths

```bash
foldry config show
foldry config path
```

The hidden `--config-dir`, `--data-dir`, and `--cache-dir` flags are intended for
tests and recovery. They allow an isolated state layout without changing the
normal platform directories.

## JSON output

Place the global `--json` option before or after a command group. Success produces
exactly one versioned object:

```json
{ "version": 1, "ok": true, "data": {} }
```

Errors use the same envelope:

```json
{
  "version": 1,
  "ok": false,
  "error": {
    "code": "validation_error",
    "message": "archive source must be a directory"
  }
}
```

Progress is suppressed in JSON mode, which makes stdout safe for scripts.

## Exit codes

|  Code | Meaning                                         |
| ----: | ----------------------------------------------- |
|   `0` | success                                         |
|   `2` | command-line usage error                        |
|   `3` | validation error or unknown identifier          |
|   `4` | I/O error or complete execution failure         |
|   `5` | partial success or success with warnings        |
|   `6` | incompatible configuration or persistence state |
|  `10` | internal scheduler or serialization failure     |
| `130` | cooperative Ctrl+C cancellation                 |

Ctrl+C requests a cooperative stop. Foldry removes its run-owned temporary file
and does not publish an incomplete final archive.
