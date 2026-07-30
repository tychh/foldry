# User guide

Foldry remembers folders and applies independent actions to them. Version 0.1.2
implements local Archive actions; direct network synchronization is future scope.

## Folders workspace

Each Folder card shows:

- the source name and path;
- its default Ignore Profile;
- the number of enabled actions;
- the most recent run status;
- controls for Preview, Run, and removing the folder from the visible list.

Selecting a card opens Folder settings. A folder may be disabled without losing
its configuration. Disabled folders are skipped by **Run all enabled**, but their
actions may still be run manually.

### Add folders

The folder browser provides:

- platform Locations;
- Favorites and Recent paths;
- lazy Tree and paged List views;
- keyboard navigation;
- optional folder-size calculation.

Files are shown for context but only folders can be added. Foldry does not follow
symlinks, junctions, reparse points, or mounted subtrees while calculating size.
Unreadable entries produce a partial result instead of an invented total.

Adding the same canonical path again restores its existing card instead of
creating a duplicate.

### Remove and forget

**Remove from Folders** hides the folder but retains its actions and run history.
The remembered-folders dialog can restore it or permanently forget its current
configuration. Historical runs remain available because they contain immutable
snapshots.

## Archive actions

A folder can have multiple ordered actions. Each action has:

- an enabled switch;
- an optional Ignore Profile override;
- output location and filename template;
- ZIP, TAR.GZ, or TAR.ZST format;
- fast, balanced, or maximum compression;
- skip, safe replace, or increment conflict handling;
- root-folder inclusion;
- unreadable-file policy;
- structural or full verification;
- optional SHA-256 checksum.

An action without an override inherits the folder's profile. Disabled actions are
skipped by group runs but remain manually runnable.

Output cannot equal the source or be placed inside it. Foldry applies the format
extension exactly once.

### Archive formats

| Format  | Best fit                                     | Important limitation                              |
| ------- | -------------------------------------------- | ------------------------------------------------- |
| ZIP     | Maximum compatibility, especially on Windows | Unix metadata and symlink extraction vary by tool |
| TAR.GZ  | Broad macOS/Linux support                    | Usually slower or larger than Zstandard           |
| TAR.ZST | Fast, compact archives                       | Built-in extractors support it less often         |

Foldry stores symlinks as links and never follows them. It currently does not
preserve hard-link relationships, sparse allocation, ownership, ACLs, extended
attributes, resource forks, or Windows alternate data streams.

## Preview

Preview is calculated for one Folder and one Action. It shows:

- the effective Ignore Profile;
- source size before filtering when the scan is complete;
- included and excluded bytes;
- included, excluded, and skipped entries;
- the profile line and preset responsible for a decision.

Entries are paged and virtualized for large trees. Changing the source, action, or
effective profile invalidates the Preview.

A real run never trusts Preview as its execution list. It rescans the source and
creates a fresh streaming manifest.

## Queue and controls

Actions enter a FIFO queue. Foldry runs up to the configured parallel limit.

- **Pause all** prevents new dispatch and pauses active work at safe entry
  boundaries.
- **Resume all** continues active work and queue dispatch.
- **Stop all** stops active work, clears queued work, and clears the global pause.
- Starting work that is already queued or active does not create duplicate runs.

An active action displays its state and progress. Folder status returns to Ready
after the last action has been terminal for 30 seconds. Restarting Foldry converts
unfinished persisted runs to Interrupted and starts folder cards in a consistent
Ready state when no active work remains.

## Run history

History survives changes to current folder and action configuration. It records:

- source, action, settings, and exact Ignore Profile snapshot;
- state, timings, entry counts, warnings, and output artifact;
- optional checksum;
- paged logs.

**Repeat snapshot** runs the historical configuration unchanged. **Run current
settings** uses the current folder and action only when they still exist.

## Ignore Profiles

Ignore Profiles are editable `.packignore` files. References use stable UUIDs, so
renaming does not break folders or actions.

The Default profile is editable but cannot be deleted through the normal
interface. Foldry recreates it when missing and falls back to it when a referenced
profile no longer exists.

Read [Ignore Profiles](ignore-profiles.md) for syntax and preset behavior.

## Appearance and accessibility

Foldry supports English and Russian, light and dark themes, keyboard navigation,
and a minimum native window size of 1024×700. The Tree/List folder-browser choice
is remembered between launches.

## Privacy

All source paths, profiles, settings, history, logs, and crash reports stay on the
local machine. Foldry 0.1.2 contains no telemetry, cloud client, account system,
remote crash upload, or automatic network transfer.
