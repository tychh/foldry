# Foldry

Foldry is a cross-platform desktop application and CLI for packaging folders while
excluding reproducible runtime and build artifacts through reusable filtering
profiles.

The core, persistence, scheduler, CLI, and desktop workflows are implemented.
Release preparation and the remaining native operating-system smoke checks follow
the [implementation plan](notes/plan.md).

## Repository layout

```text
crates/
├── foldry-core/          Domain core
├── foldry-application/   Use cases and application ports
├── foldry-storage/       Storage adapters
├── foldry-cli/           English CLI executable: foldry
└── foldry-tauri/         Tauri desktop adapter
frontend/                 React, TypeScript, Vite, and Mantine
resources/                Shipped profiles and presets
tests/fixtures/           Versioned public-format compatibility fixtures
docs/                     Architecture and acceptance criteria
notes/                    Product task, decisions, and implementation plan
```

See the [system map](docs/architecture/system-map.md) for dependency rules.

## Prerequisites

The pinned Rust, Node.js, pnpm, and Windows/macOS/Linux system prerequisites are
listed in [the build and run guide](docs/running.md).

## Setup

```bash
corepack enable
pnpm install
```

## Development

Run the browser frontend:

```bash
pnpm dev
```

Run the Tauri desktop shell:

```bash
pnpm desktop:dev
```

Run the CLI:

```bash
cargo run -p foldry-cli --bin foldry
```

## Quality commands

```bash
pnpm format:check
pnpm lint
pnpm typecheck
pnpm contracts:check
pnpm test
pnpm build
pnpm desktop:build
```

Native bundles are written to `target/release/bundle/`; the standalone release CLI
is built with `cargo build --release -p foldry-cli --bin foldry`.

Run the complete local gate with:

```bash
pnpm check
```

## Architecture and scope

- [Architecture decisions](docs/architecture/README.md)
- [Acceptance checklist](docs/acceptance-checklist.md)
- [Public data formats v1](docs/contracts/formats-v1.md)
- [`.packignore` syntax v1](docs/contracts/packignore-v1.md)
- [Rust/TypeScript transport contracts](docs/contracts/transport.md)
- [Filesystem scanner and preview contract](docs/contracts/scanner-preview.md)
- [Archive planning and execution v1](docs/contracts/archive-execution-v1.md)
- [Persistence and recovery v1](docs/contracts/persistence-v1.md)
- [Scheduler v1](docs/contracts/scheduler-v1.md)
- [CLI v1](docs/cli.md)
- [How Foldry works](docs/how-it-works.md)
- [Build and run guide](docs/running.md)
- [Troubleshooting and data locations](docs/troubleshooting.md)
- [Release process](docs/releasing.md)
- [Cross-platform smoke matrix](notes/multiplatform-smoke.md)
- [Implementation plan](notes/plan.md)
- [Open decisions](notes/decisions.md)

Foldry has no telemetry or remote crash upload. Profiles, paths, history, logs, and
crash reports remain local by contract.
