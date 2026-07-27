# Troubleshooting Foldry

## Find the active files

Run:

```bash
foldry config path
```

This is authoritative even when XDG variables or recovery overrides are in use.
The default layout is:

| Platform | Config                                             | Local data                                         | Cache                                         |
| -------- | -------------------------------------------------- | -------------------------------------------------- | --------------------------------------------- |
| Linux    | `$XDG_CONFIG_HOME/foldry` or `~/.config/foldry`    | `$XDG_DATA_HOME/foldry` or `~/.local/share/foldry` | `$XDG_CACHE_HOME/foldry` or `~/.cache/foldry` |
| Windows  | `%APPDATA%\Foldry\config`                          | `%LOCALAPPDATA%\Foldry\data`                       | `%LOCALAPPDATA%\Foldry\cache`                 |
| macOS    | `~/Library/Application Support/app.foldry.desktop` | same application-support directory                 | `~/Library/Caches/app.foldry.desktop`         |

Config contains `settings.yaml`, `active.packplan.yaml`, `profiles/`, and
`presets/`. Local data contains `app.db` and `crash-reports/`. Cache contains
streaming manifests that Foldry owns and may clean up.

## The application does not start

On Linux, confirm WebKitGTK 4.1 and the dependencies from
[running.md](running.md) are installed. Start the executable from a terminal to
capture loader errors. An AppImage may need:

```bash
chmod +x Foldry_*.AppImage
./Foldry_*.AppImage
```

On Windows, install the Evergreen WebView2 Runtime. On macOS, an unsigned test build
may be blocked by Gatekeeper; official release candidates must be signed and
notarized rather than asking users to disable security controls.

## A profile is invalid

Open Profiles and inspect the line diagnostics. Invalid text is intentionally
saved so edits are not lost, but preview and new runs using that profile remain
blocked. Compare the file with its `*.packignore.previous-good` sibling if one
exists. The syntax reference is [here](contracts/packignore-v1.md).

## A task cannot be added

Foldry accepts directories, not individual files. The same canonical source can
appear only once in the active plan; adding it again focuses the existing task.
Permission-denied, disconnected network, and missing directories remain visible
as errors instead of being replaced or removed silently.

## A run failed or skipped files

Open the run history and detailed logs. The default `unreadable_policy: fail`
stops when a planned file disappears, changes, or cannot be read. The explicit
`warn_and_skip` policy publishes the remaining entries and reports
success-with-warnings.

Check:

- source and output permissions;
- free space on the output filesystem;
- locks held by another process;
- whether the output directory is a disconnected network/removable mount;
- the exact warning/error code in exported JSONL logs.

Foldry leaves the previous archive unchanged when failure occurs before
publication. A `.part` or reservation that survives a hard crash is reconciled on
the next startup only after ownership, age, and process-liveness checks.

## An archive already exists

- `skip` returns without writing;
- `increment` reserves `name (N)` atomically;
- `overwrite` keeps the old archive until the replacement has finished and passed
  verification.

Never delete reservation sidecars while another Foldry GUI or CLI process may be
running. For recovery, stop all Foldry processes first and retain a copy of the
directory before manual cleanup.

## History is larger or smaller than expected

By default, run metadata is retained for no more than one year and 10,000 runs.
Detailed logs are retained for no more than 90 days and 1,000 runs. The stricter
age/count boundary wins. `unlimited` disables the corresponding cleanup.

Retention never deletes final archive files. Deleting `app.db` manually removes
history and logs, so close Foldry and back it up first. Profiles, settings, and the
active plan live in the separate config directory.

## Browser build works but the desktop build fails

Run:

```bash
corepack pnpm tauri info
pnpm desktop:build
```

The first command reports missing platform libraries and toolchains. Keep Node,
pnpm, Rust, and Tauri versions pinned as described in
[running.md](running.md). On Windows MSI builds, also verify the VBSCRIPT optional
feature.

## Reporting a reproducible problem

Include the Foldry version, OS build and architecture, filesystem type, archive
format and policies, exact steps, and exported logs. State whether an old archive,
`.part`, or reservation remained. Do not attach source files, profiles, paths, or
logs containing secrets unless you have reviewed and redacted them.
