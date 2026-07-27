# How Foldry works

Foldry turns folders into ZIP, TAR.GZ, or TAR.ZST archives. It is designed for
repeatable backups of source trees and work folders: profiles decide what to
exclude, preview explains the decision, and every archive is verified before it
replaces an older file.

## Profiles and presets

A profile is an editable UTF-8 `.packignore` file. Its rules resemble
`.gitignore`: later rules override earlier rules, `!` includes something again,
and `*`, `?`, character classes, and `**` are supported. Each profile has a stable
UUID, so renaming it does not disconnect tasks that use it.

Presets are named blocks of common rules for languages, frameworks, IDEs, build
output, and operating-system metadata. Safe presets can be inserted directly.
Presets that may exclude secrets, private media, database dumps, or local
configuration are visibly marked and require confirmation. Foldry never enables a
sensitive preset automatically.

The complete grammar and the differences from Git are documented in
[`.packignore` syntax v1](contracts/packignore-v1.md).

## Tasks and the active plan

A task binds one source folder to one profile and one ordered action list. Version
1 supports one `archive` action, but the plan format already preserves unknown
future action types without executing them.

The desktop application automatically saves one active `.packplan.yaml`. Selecting
the same canonical source twice focuses the existing task instead of creating a
duplicate. Global archive defaults are copied into new tasks; a task can override
them without changing the defaults.

## Preview

Preview scans the source without creating an archive. It is paged and virtualized,
so large trees are not loaded into the webview at once. Every result includes its
state and, when a rule matched, the profile line, pattern, and preset that caused
the decision.

A completed preview records its profile hash and scan time. The cache is reused
only while the profile, source metadata, and archive settings still match. A real
run always creates a new execution manifest; preview is never trusted as the
authoritative file list.

## Runs and the scheduler

Foldry queues enabled tasks in FIFO order up to the configured parallel limit.
Pause finishes the current archive entry and then waits without reading the source
or consuming CPU; it still owns its scheduler slot. Stop wakes a paused worker,
cancels queued work where applicable, and removes only temporary files owned by
that run.

Progress is throttled, while state changes, warnings, errors, and final results are
delivered immediately. If exact totals are not known, the interface shows
indeterminate progress instead of inventing a percentage.

## Archive safety

Planning writes entries to a bounded streaming manifest. The executor treats that
manifest as untrusted: each path is normalized, rebound to the selected source
root, and rejected if it is absolute or escapes through `..`.

Foldry does not traverse symlinks or junctions. A planned regular file is opened
without following a link introduced after the scan, then its identity, size, and
modification time are checked again. TAR stores symlinks natively. ZIP uses the
common Unix-compatible link representation and reports a portability warning
because not every ZIP extractor restores it safely.

The destination filename is reserved atomically between processes. Foldry writes a
run-owned `.part` file in the destination filesystem, finishes and syncs the
codec, verifies the archive, optionally calculates SHA-256, and only then publishes
it atomically. `overwrite` therefore keeps the previous archive until the new one
is complete. `skip` never starts a write when the target exists; `increment`
reserves a unique numbered name without a check-then-create race.

## History and recovery

Run summaries and detailed logs are stored locally in SQLite. Logs are fetched in
bounded pages only when opened. Repeating a historical run uses its immutable
profile/settings snapshot; “run with current settings” creates a new snapshot.

At startup, unfinished runs become `interrupted`. Stale cleanup removes only an
old reservation whose owner is no longer alive and whose metadata names the exact
temporary file. It does not delete archives. Default retention keeps run metadata
for at most one year and 10,000 runs, and detailed logs for at most 90 days and
1,000 runs; the stricter age/count limit wins.

## Privacy and trust

Foldry has no telemetry, remote crash upload, cloud service, or runtime network
client. Profiles, paths, history, logs, and crash-report directories stay on the
local machine. The desktop webview has no ambient filesystem or opener authority:
typed Rust commands validate paths and look up stored artifacts before opening a
folder.

The local operating-system account remains the trust boundary. Foldry does not
encrypt local history, protect against a malicious process running as the same
user, or control how a third-party extractor handles archived symlink targets.
The detailed review is in [the threat and abuse report](security/threat-review.md).
