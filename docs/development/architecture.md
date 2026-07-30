# Architecture

Foldry is a Rust workspace with a React/Tauri desktop adapter and a standalone CLI.
Dependencies point inward so filesystem and archive behavior can be tested without
a webview.

## Workspace layers

```text
React frontend
      │ typed Tauri IPC
      ▼
foldry-tauri ─────────┐
                     ▼
foldry-cli ──► foldry-application ──► foldry-core
                           │
                           ▼
                    foldry-storage
```

### `foldry-core`

Owns domain primitives that do not require persistence or UI:

- UUID identities;
- Ignore Profile parsing and matching;
- filesystem scanning and manifest types;
- archive formats, output validation, reservations, and publication;
- Folder, Action, Run, and settings value objects.

It must not depend on Tauri, React/Node, SQLite, or adapter DTOs.

### `foldry-application`

Owns use cases and ports:

- loading and repairing application state;
- folder/action/profile/preset operations;
- effective profile resolution and Default fallback;
- Preview preparation;
- immutable Run snapshots;
- FIFO scheduling and pause/resume/stop semantics;
- retention and history queries;
- request/response transport contracts.

### `foldry-storage`

Implements application ports:

- atomic YAML and `.packignore` repositories;
- SQLite history and log repositories;
- packaged resource initialization;
- startup reconciliation and cleanup;
- streaming scanner manifests;
- archive execution and verification.

### Adapters

`foldry-cli` and `foldry-tauri` call the same application services. The CLI adds
human/JSON output and cooperative Ctrl+C. Tauri validates webview requests and
exposes bounded typed commands.

The React frontend owns presentation state only. It does not implement archive,
profile, scheduler, or persistence rules.

## Domain model

### Folder

A Folder has a canonical source path, stable `FolderId`, visible/listed state,
group-run enabled state, default profile, and ordered actions. Canonical source
paths are unique.

Unlisting preserves configuration. Forgetting removes only the current
configuration; historical Run snapshots remain.

### Action

An Action has a stable `ActionId`, enabled state, typed specification, and optional
profile override. Archive is the only action type in 0.1.2.

Unknown future action types are preserved by supported document versions but
cannot be previewed or executed.

### Run

A Run stores `FolderId + ActionId` plus an immutable snapshot of:

- source path;
- complete action specification;
- effective Ignore Profile text and hash;
- execution settings.

Current runs use current state. Historical repeat uses the saved snapshot even
after current configuration changes.

## Primary flows

### Preview

1. Resolve Folder and Action.
2. Resolve the override, inherited profile, or Default fallback.
3. Detect source filesystem case behavior.
4. Scan without following links.
5. Write a bounded temporary manifest.
6. Return summary and paged entries with rule provenance.

Preview is advisory. Execution always creates a fresh manifest.

### Archive run

1. Create and persist the immutable queued Run.
2. Dispatch through the FIFO scheduler.
3. Rescan the source into a streaming manifest.
4. Revalidate every source-relative path.
5. Reserve the output name atomically.
6. Write a same-filesystem `.part`.
7. Finalize, sync, verify, and optionally checksum.
8. Publish atomically and persist the terminal summary.

### Startup

1. Resolve platform directories.
2. Ensure required directories exist.
3. initialize or restore packaged profiles and presets.
4. Load and validate settings and the active plan.
5. Run contiguous database migrations.
6. Mark unfinished historical runs Interrupted.
7. Remove only provably owned stale temporary artifacts.

## Extension rule

A new operation, including synchronization, should be a new typed Action
specification and executor. It should reuse Folder identity, profile selection,
Preview/plan concepts where applicable, scheduler controls, immutable snapshots,
history, logs, and typed adapter boundaries.
