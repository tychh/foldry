# Persistence and recovery contract v1

## Directory ownership

Foldry resolves platform-native config, local-data, and cache directories through
the operating system. Explicit directory overrides exist only for development,
tests, and recovery. They do not define a supported portable mode.

- Config: `settings.yaml`, `active.packplan.yaml`, `profiles/`, `presets/`.
- Local data: `app.db`, `crash-reports/`.
- Cache: `manifests/`.

The application creates missing directories and resource working copies, but never
replaces an existing editable profile or preset during installation.

## User-editable files

Settings and the single active plan are encoded, decoded again for validation, and
then atomically replaced. A read or validation failure is returned to the caller;
loading a corrupt file never silently replaces it with defaults.

Profile text is atomically saved even when parser diagnostics make it invalid.
Before overwriting a valid profile, Foldry stores one
`*.packignore.previous-good` copy. Lenient header extraction keeps an invalid profile
addressable by its valid `@profile-id`; invalid profiles cannot be used for a new
preview or run.

Preset resource installation is create-if-missing. Restoring a preset from packaged
resources is a separate explicit operation.

## SQLite schema v1

`app.db` stores:

- run identity, task identity, state, timestamps, and the immutable run snapshot;
- bounded final summaries plus normalized warning and error rows;
- detailed log rows ordered by a per-run sequence.

Schema changes run in a transaction and update `PRAGMA user_version` only on success.
A database newer than the supported schema is rejected. Updating a run uses an
UPSERT and does not delete its logs.

Finite retention applies both age and count limits. Run metadata defaults to 365
days and 10,000 runs; logs default to 90 days and 1,000 runs. The stricter result is
kept. `unlimited` bypasses the corresponding cleanup. Archive files are never part
of retention deletion.

## Startup reconciliation

At startup, unfinished states (`queued`, `planning`, `running`, `paused`, and
`stopping`) become `interrupted`.

Every output reservation contains versioned JSON ownership metadata:

- `run_id`;
- owning process ID;
- creation time as Unix seconds;
- the exact temporary filename.

Stale cleanup removes a reservation and its exact temp file only when:

1. metadata and filename cross-check successfully;
2. the minimum age has elapsed;
3. the owning process no longer exists.

Legacy, malformed, recent, active, unrelated, directory, and symlinked reservation
entries are retained. Stale manifest cleanup is restricted to old regular
`*.foldry-manifest` files inside Foldry's owned manifest directory.

## Application services

The storage-independent application API provides:

- settings and active-task persistence;
- profile and preset CRUD;
- current-state preview/run preparation;
- historical snapshot repeat;
- paged history and logs;
- retention execution.

Adding or updating a task canonicalizes its source directory and rejects a duplicate
canonical source. A profile referenced by an active task cannot be deleted. A new
run snapshots the exact task, settings, profile text, and profile SHA-256 before it
is inserted into history as `queued`; repeat copies the previous snapshot rather
than current configuration.
