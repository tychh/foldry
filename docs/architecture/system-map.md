# Foldry system map

This document is the stage-one map of component ownership. ADR-0001 remains the
authoritative dependency decision.

## Dependency direction

```text
┌──────────────────┐       typed commands/events       ┌──────────────────┐
│ React + Mantine  │ ─────────────────────────────────> │   foldry-tauri    │
└──────────────────┘                                    └────────┬─────────┘
                                                                 │
┌──────────────────┐                                             │
│    foldry CLI    │ ────────────────────────────────────────────┤
└──────────────────┘                                             ▼
                                                     ┌──────────────────────┐
                                                     │ foldry-application   │
                                                     │ use cases / ports    │
                                                     └───────┬──────────────┘
                                                             │
                                     ┌───────────────────────┼─────────────┐
                                     ▼                       ▼             │
                             ┌───────────────┐       ┌────────────────┐     │
                             │  foldry-core  │       │ foldry-storage │ <───┘
                             │ pure domain   │       │ port adapters  │
                             └───────────────┘       └────────────────┘
```

Runtime composition happens in an adapter executable. Dependencies always point
toward application/core contracts; core never calls outward into Tauri or SQLite.

## Crate ownership

| Component            | Owns                                                                                 |
| -------------------- | ------------------------------------------------------------------------------------ |
| `foldry-core`        | IDs, profiles, matcher, scanner, archive specs, execution and writers                |
| `foldry-application` | Plan/settings/run contracts, use cases, scheduler, ports and transport DTOs          |
| `foldry-storage`     | YAML/profile/manifest/SQLite adapters, migrations and startup reconciliation         |
| `foldry-cli`         | English commands, JSON envelope, progress, exit codes and cancellation               |
| `foldry-tauri`       | Desktop composition, validated commands/events, capabilities and native integrations |
| `frontend`           | Localized profile, task, preview, history and settings workflows                     |

## Primary flows

### Preview

```text
GUI/CLI -> preview use case -> profile matcher + scanner
        -> bounded summary + temporary streaming manifest -> cursor pages
        <- paged entries + include/exclude provenance
```

### Archive run

```text
GUI/CLI -> scheduler -> planner -> streaming manifest -> archive writer
             │                                         │
             └-> state/progress/logs                    └-> atomic publish
```

### Persistence

```text
application port -> YAML profile/settings/active-plan adapters
application port -> SQLite run/history/log adapters
```

## Public and private contracts

Public, versioned contracts:

- `.packignore`;
- `.packplan.yaml` and settings YAML;
- CLI flags, JSON, and exit codes;
- Tauri command/event DTOs;
- SQLite migrations between released versions.

Private implementation details:

- streaming manifest format;
- temporary/archive reservation filenames;
- internal task channels and worker layout;
- frontend cache representation.

## Guardrails

- `foldry-core` has no Tauri, frontend, or SQLite dependency.
- A task has one canonical source and ordered action steps.
- CLI and GUI never implement their own matcher/archive behavior.
- Detailed logs are requested; they are not streamed through progress events.
- Unknown action types are preserved when safe, displayed as unsupported, and never
  executed.
