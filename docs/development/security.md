# Security model

Foldry is a local single-user application. The installed binary and operating
system account are trusted. Source trees, paths, profiles, persisted
YAML/JSON, archive manifests, and webview arguments are untrusted.

Security issues may be reported through
[GitHub Security Advisories](https://github.com/tychh/foldry/security/advisories/new).
Do not include real secrets or personal source paths in a public issue.

## Trust boundaries

| Boundary                      | Principal control                                                      |
| ----------------------------- | ---------------------------------------------------------------------- |
| Webview to Rust               | typed IDs, bounded pages, stored artifact lookup                       |
| Folder browser/drop to source | canonical directory validation and uniqueness                          |
| Source to scanner             | `symlink_metadata`, no link traversal, cycle/device checks             |
| Manifest to executor          | normalized relative paths rebound under the source                     |
| Scan to file open             | no-follow open plus identity/size/mtime checks                         |
| Executor to output            | containment rejection, create-new reservation, verified atomic publish |
| Startup cleanup               | owner/PID/age/path sidecar checks                                      |
| Application to network        | no runtime network client; CSP permits Tauri IPC only                  |

## Filesystem behavior

The scanner never traverses symlink targets. On Windows, junctions and reparse
points are skipped with typed warnings. Special files and mounted subtrees are not
treated as ordinary source content.

The executor treats its fresh manifest as untrusted:

- absolute and parent-traversal paths are rejected;
- every path is rebound beneath the immutable source;
- a regular file replaced by a link after scanning is rejected;
- identity, size, and modification changes follow the selected policy.

Link targets stored inside an archive are not followed by Foldry. Extraction is
performed by third-party tools and remains a separate trust boundary.

## Archive publication

Conflict handling is race-safe. Foldry reserves the destination, writes a
same-filesystem `.part`, finalizes and syncs the codec, performs requested
verification, optionally calculates SHA-256, and only then publishes.

Safe replace keeps the previous archive until the new one is ready. Stop and error
paths remove only run-owned temporary artifacts.

## Privacy

Foldry 0.1.2 has no telemetry, analytics, cloud client, updater, account system, or
remote crash upload. Paths, profiles, history, logs, and crash reports stay local.

Local state is not encrypted. Another process running under the same OS account is
inside the trust boundary.

## Future review triggers

Before implementing any of the following, update this model:

- LAN, SSH, or server synchronization;
- automatic updates;
- plugins or user-provided executable hooks;
- remote UI or multi-user server mode;
- privileged helper/service;
- credential storage;
- telemetry or remote crash reporting.
