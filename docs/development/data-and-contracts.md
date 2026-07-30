# Data and contracts

Foldry persists human-readable configuration separately from operational history.
Writes are atomic and versioned.

## Platform directories

The internal CLI command `foldry config path` can show resolved locations in a
development checkout.

| Area   | Contents                                                         |
| ------ | ---------------------------------------------------------------- |
| Config | `settings.yaml`, `active.packplan.yaml`, profiles, preset copies |
| Data   | `app.db`, crash reports                                          |
| Cache  | streaming Preview/Run manifests                                  |

The desktop and internal CLI adapter use the same directories. Development tests
can override each root with hidden CLI flags.

## Settings

`settings.yaml` version 1 stores:

- locale and appearance;
- default profile ID;
- Archive defaults;
- scheduler parallel limit;
- run/log retention;
- folder-browser Favorites, Recent, and Tree/List mode;
- extension fields.

## Active plan

`active.packplan.yaml` version 2 stores Folders and ordered Actions:

```yaml
version: 2
name: Active plan
folders:
  - id: 0190...
    source: /Users/alice/Code/project
    listed: true
    enabled: true
    default_profile_id: 0190...
    actions:
      - id: 0190...
        enabled: true
        profile_id_override: null
        spec:
          type: archive
          version: 1
          output:
            directory: { mode: parent }
            filename: "{folder}-{date}"
            format: zip
            compression: balanced
            conflict_policy: increment
          include_root: true
          unreadable_policy: fail
          verification:
            mode: structural
            checksum: none
```

Supported versions preserve unknown fields through semantic round trips. Unknown
document versions are rejected. There is no compatibility loader for discarded
pre-release task models.

## Ignore Profiles and presets

Profiles are UTF-8 `.packignore` files with stable UUID metadata. Preset blocks
carry IDs and versions so installed, modified, and outdated states can be
determined without guessing.

The working Default copy is restored from packaged resources when missing. Missing
references resolve to Default before an operation proceeds.

## SQLite

`app.db` stores schema version, Runs, terminal summaries, and paged logs.
Migrations are contiguous and transactional. Startup fails rather than skipping an
unknown migration.

History rows retain immutable snapshots, which allows repeat and inspection after
the current Folder or Action is removed.

## Transport

Rust request/response DTOs are generated into
`frontend/src/shared/contracts/generated.ts` with `ts-rs`.

```bash
pnpm contracts:generate
pnpm contracts:check
```

IPC commands use typed IDs, reject arbitrary filesystem access, and bound pages to
at most 1,000 records. Reveal/export operations resolve stored Run artifacts
instead of accepting an arbitrary path from the webview.

## Compatibility rules

- IDs are UUIDv7 strings; display names are not identity.
- Settings and active plans have explicit document versions.
- Archive Actions have independent type/spec versions.
- Unknown fields in supported versions are preserved.
- Unknown action types are preserved but execution-blocked.
- Database migrations never silently skip a version.
- The internal JSON CLI envelope is version 1.
