# Contributing to Foldry

## Before changing code

1. Read the [system map](docs/architecture/system-map.md).
2. Check the accepted [ADRs](docs/architecture/README.md).
3. If a change affects a public format, data safety, or observable behavior and the
   answer is not already documented, add the question to `notes/decisions.md` before
   implementing that part.

## Local setup

Install the prerequisites from `README.md`, then:

```bash
corepack enable
pnpm install
pnpm check
```

Use the pinned Rust, Node.js, pnpm, and dependency versions. Update a pin in a
dedicated change together with its lockfile and compatibility checks.

After changing a Rust transport DTO, regenerate and verify frontend contracts:

```bash
pnpm contracts:generate
pnpm contracts:check
```

## Dependency rules

- `foldry-core` must not depend on Tauri, React/Node, SQLite, or transport DTOs.
- `foldry-application` owns use cases and storage ports.
- `foldry-storage` implements application ports.
- CLI and Tauri are adapters to the same application services.
- Frontend business operations cross the typed Tauri boundary; they are not
  reimplemented in TypeScript.

## Quality bar

Before handing off a change, run:

```bash
pnpm format:check
pnpm lint
pnpm typecheck
pnpm test
pnpm build
```

Run `pnpm desktop:build` for changes to Tauri, capabilities, frontend build
configuration, or bundled resources.

The workspace test command excludes the Tauri adapter's empty native test harness
to avoid linking a second WebKit desktop binary. Tauri still passes strict Clippy
and normal/release builds. Add pure adapter logic to a lightweight testable module
before introducing Tauri-specific unit tests.

Tests should be added at the lowest useful level:

- unit tests for pure rules and state transitions;
- integration tests for scanner/planner/archive/storage boundaries;
- component tests for frontend behavior;
- end-to-end/manual evidence for platform-specific behavior.

Use identifiers from `docs/acceptance-checklist.md` in test names or comments when a
test closes a release criterion.

## Formatting

- Rust: `cargo fmt`; warnings are denied by Clippy in CI.
- TypeScript: strict mode, ESLint, and Prettier.
- User-facing text belongs in localization resources once the i18n layer is added.
- Markdown uses short sections, descriptive links, and fenced examples.

## Safe changes

- Never overwrite user settings, profiles, plans, or archives before a replacement
  is fully written and validated.
- Never follow symlink or junction targets while scanning.
- Cleanup may delete only artifacts that Foldry can prove it owns.
- Do not add telemetry or remote crash upload without a new accepted ADR.
