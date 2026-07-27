# Archive planning and execution v1

## Immutable planning boundary

Every run creates a fresh execution manifest. Preview manifests are never reused as
authoritative input. The execution plan snapshots source root, archive action and
bounded scan totals; entries remain streamed from the manifest.

Before writing, Foldry:

1. validates source and output directory;
2. resolves the format extension and conflict policy;
3. creates a same-directory reservation sidecar with `create_new`;
4. creates one run-owned `.part` archive in the output directory.

`skip` returns before creating either file when the target exists. `increment`
reserves `name (N)` atomically, so cooperating processes cannot select the same
name. `overwrite` keeps the old target until the fully finished archive is
atomically installed. Dropping an unpublished reservation removes only its own
temp and sidecar.

The final path, temp path and reservation path are excluded during execution when
the output directory is inside the source.

## Archive writers

One backend interface implements ZIP, TAR.GZ and TAR.ZST. Semantic compression
mapping version 1 is:

| Format  | Fast | Balanced | Maximum |
| ------- | ---: | -------: | ------: |
| ZIP     |    1 |        6 |       9 |
| TAR.GZ  |    1 |        6 |       9 |
| TAR.ZST |    1 |        3 |      19 |

Directories, regular files and symlinks are explicit entries. ZIP symlinks use the
Unix-compatible representation and produce a portability warning. Junction/reparse
points and special files are skipped with typed warnings.

## Source consistency and control

The private manifest is an optimization, not a trusted authorization boundary.
Before opening every entry, the executor normalizes its archive-relative path,
rejects absolute and parent-traversal components, and verifies that the native
path resolves lexically to the same relative path beneath the planned source root.
An entry that fails this binding is never read, even if the manifest file was
modified after planning.

Files are copied through a bounded temporary spool in the output filesystem.
Regular files are opened without following a final symlink or reparse point. The
opened handle must describe a regular non-link file, and its identity, size, and
modification time are checked again after the copy. This closes the scan-to-open
replacement window and lets an unreadable, replaced, or changed file be discarded
before an archive entry starts. `fail` aborts the run; `warn_and_skip` publishes
the remaining entries with a typed warning.

Pause is observed between entries and waits on a condition variable without source
I/O or CPU work. Stop wakes a paused worker and is checked between entries and
during every large-file read chunk. Any stop/error drops the writer and reservation,
leaving an existing target unchanged.

## Finalization

The codec is explicitly finished and the temp file is synced. Structural
verification always opens the resulting container. Full verification additionally
reads every entry payload. SHA-256 is computed only when requested, after
verification and before atomic publication. Source content hashes are never
computed.
