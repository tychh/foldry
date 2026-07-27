# Foldry v1 threat and abuse review

## Scope and trust model

Foldry is a local single-user archive application. Source trees, editable
profiles/YAML, dropped paths, output locations, and webview command arguments
are treated as untrusted inputs. The local OS account and installed Foldry
binary are trusted. Remote multi-user authorization, encrypted-at-rest history,
malicious kernel/filesystem behavior, and safety of third-party extractors are
non-goals for v1.

The review covers archive entry construction, source mutation, symlinks and
reparse points, output publication, private manifests, startup cleanup, desktop
IPC, opener usage, local history, and network/telemetry behavior.

## Trust boundaries and controls

| Boundary                       | Abuse case                                                 | Owned control                                                                                                                         | Result                |
| ------------------------------ | ---------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------- | --------------------- |
| Webview → Rust IPC             | forged IDs, huge pages, arbitrary reveal path              | typed DTO conversion, UUIDv7 parsing, page/collection bounds; reveal loads artifact by run ID and revalidates a regular non-link file | Controlled            |
| Dropped/edited path → task     | file instead of directory, duplicate alias, relative path  | canonical directory validation and canonical-source uniqueness                                                                        | Controlled            |
| Profile/YAML → executable plan | malformed/future input, unknown action, path-like filename | versioned decoding, structural validation, filename component checks, invalid-profile blockers                                        | Controlled            |
| Source tree → scanner          | link/junction traversal, cycles, unreadable/special nodes  | `symlink_metadata`, no traversal, file identity cycle detection, typed notices                                                        | Controlled            |
| Manifest → executor            | tampered absolute/native path or traversal entry           | normalized relative path and source-root/native-path binding before every entry                                                       | Hardened in stage 14  |
| Scan → file open               | regular file replaced by symlink to an external secret     | no-follow/open-reparse-point open, handle metadata and post-copy identity checks                                                      | Hardened in stage 14  |
| Executor → output              | self-archive, collision race, partial overwrite            | canonical output directory, safe single-component filename, create-new reservation/temp, verify then atomic publish                   | Controlled            |
| Crash → cleanup                | delete another process's or a recent run's file            | versioned run-owned sidecar, PID/liveness, age, exact filename and regular-file checks                                                | Controlled            |
| Stored run → opener            | caller supplies arbitrary filesystem path                  | backend resolves artifact from history, rejects missing/link/non-file, invokes reveal only                                            | Controlled            |
| Application → network          | telemetry or path exfiltration                             | no HTTP/WebSocket client, CSP `connect-src` limited to Tauri IPC, no telemetry dependency                                             | No runtime path found |

## Archive path and symlink posture

Every scanner-produced archive name is normalized to a relative `/` path and
rejects absolute or parent-traversal components. Stage 14 additionally rejects
a private manifest entry whose native path does not correspond to that relative
path beneath the selected source root. Output filenames reject separators,
empty components, and `.`/`..`.

Foldry intentionally stores symlink targets without following them. An absolute
or parent-relative symlink target may therefore remain meaningful after
extraction. This is not a write primitive inside Foldry, which never extracts
archives, but it is a property archive consumers must handle with destination
containment. The contract and user documentation must keep this distinction
visible.

## Data-loss and recovery posture

An archive is written to a create-new run-owned temp file in the final
filesystem. The working archive is replaced only after writer finalization,
`sync_all`, requested verification, and atomic publication. Stop/error drops the
reservation and temp file but does not touch the previous archive. Startup
cleanup accepts only exact regular non-link sidecars and temp names, retains
recent or live-process work, and reconciles unfinished history as interrupted.

The existing tests cover stop cleanup, failed overwrite preservation,
concurrent increment reservations, stale/live ownership, corrupted persistence,
source mutation, and startup interruption.

## Desktop and privacy posture

The main window capability grants `core:default` only. Dialog and opener plugins
are called from Rust rather than exposed as ambient frontend permissions.
Production CSP allows scripts only from `self`, objects from nowhere, and
connections only to the Tauri IPC origins. Prototype freezing is enabled.

Paths, warnings, and logs are persisted locally in YAML/SQLite according to the
configured age/count retention policies. There is no runtime fetch,
XMLHttpRequest, WebSocket, beacon, telemetry, or analytics dependency. Exported
logs go only to a path explicitly selected in the native save dialog.

## Residual risks and escalation triggers

- A process running as the same OS user can race or replace an output directory.
  Directory-handle-relative creation is the next hardening step if shared
  attacker-writable output locations enter scope.
- Local config/history confidentiality is inherited from OS account and
  directory permissions; Foldry does not encrypt it.
- A hostile downstream extractor can mishandle stored symlink targets; Foldry's
  no-follow guarantee applies to collection, not extraction.
- OS-specific locked files, network mounts, and packaging/signing behavior
  require the Windows/macOS manual matrix before release.
- Any future remote UI, plugin system, updater, cloud output, or privileged
  background service invalidates this threat model and requires a new review.
