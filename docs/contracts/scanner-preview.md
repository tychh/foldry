# Filesystem scanner and preview contract

This document fixes the v1 behavior shared by the desktop application and CLI.
The manifest format itself is private and may change without migration.

## Path boundaries

- Native `Path` values are used for filesystem I/O.
- Only source-relative paths normalized to `/` are passed to the profile matcher
  and later used as archive entry names.
- Absolute paths and parent traversal never cross the matcher boundary.
- Source-filesystem case behavior is probed read-only from an existing path when
  possible. Windows/macOS or Linux defaults are used only when no suitable name can
  be probed, and the result carries its confidence.

## Object classification and traversal

| Object                         | Preview                       | Traversal                    |
| ------------------------------ | ----------------------------- | ---------------------------- |
| Regular file                   | included/excluded with reason | never treated as a directory |
| Directory                      | included/excluded with reason | only when included/readable  |
| Symlink                        | separate entry                | never                        |
| Junction/other reparse point   | separate entry                | never                        |
| Special file                   | skipped plus notice           | never                        |
| Unreadable/disappeared entry   | skipped plus notice           | never                        |
| Accessible mount/network mount | flagged directory             | like a regular directory     |

Traversal is iterative. It retains open frames and active ancestor identities, not
the complete list of discovered paths. Repeated directory identity in the active
path is reported as a cycle and not traversed.

Linux browser nodes below `/proc`, `/sys`, and `/dev` are visibly marked as platform
special and unavailable. Mount points are detected from device boundaries and, on
Linux, `/proc/self/mountinfo`; common network filesystem types get a separate flag.
Other platforms use best-effort metadata.

## Lazy filesystem browser

- Root descriptors contain Home, filesystem roots/drives, and favorites without
  scanning descendants.
- macOS also exposes mounted directories below `/Volumes`; Windows exposes existing
  drive letters.
- Expanding a directory loads and sorts only its direct children.
- Node IDs are stable SHA-256 hashes of native path bytes/code units.
- A request registry assigns generations. Starting a replacement request cancels
  its predecessor; results are accepted only while their generation is current.
- Availability, object kind, mount/network, and platform-special state are explicit
  fields and must not be communicated by color alone.

## Streaming scan and manifest

The scanner sends each `ScannedEntry` and `ScanNotice` to a `ScanSink`. Its in-memory
summary contains counters and byte totals only. The storage adapter serializes each
record immediately as a newline-delimited private manifest in the cache/temp
directory.

An unfinished writer owns its file and removes it on drop. `finish` flushes and
syncs the file before returning a handle. Explicit invalidation removes the finished
manifest. A synthetic one-million-entry test exercises the same serializer with a
fixed 16 KiB buffer.

## Preview

- Pages contain at most 1,000 entries and use an opaque byte cursor.
- Filters are `all`, `included`, `excluded`, and `skipped`.
- Every filtered entry carries the exact last matching profile rule when one
  exists: profile ID, line, original rule, and optional preset ID.
- A snapshot exposes creation time, profile hash, and bounded scan summary.
- The cache key hashes the exact profile text, observed source-root metadata, and
  serialized action specification.
- File watcher/application events explicitly invalidate the task snapshot when
  nested source metadata changes. Profile/action changes produce a different key.
- Preview is advisory. Execution always creates a new immutable plan and manifest.

Scanner and page reads check a cooperative cancellation token. Cancellation returns
no partial page; dropping the unfinished manifest leaves no background artifact.
