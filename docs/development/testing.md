# Testing and performance

## Complete quality gate

```bash
pnpm check
```

This runs:

1. Prettier and `cargo fmt --check`;
2. ESLint and strict workspace Clippy;
3. Rust-to-TypeScript contract drift checks;
4. TypeScript type checking;
5. release metadata validation;
6. frontend and Rust tests;
7. frontend and workspace production builds.

For native configuration, capabilities, icons, resources, or desktop integration:

```bash
pnpm desktop:build
```

## Narrow commands

```bash
pnpm format:check
pnpm lint
pnpm typecheck
pnpm contracts:check
pnpm test
pnpm build
cargo test -p foldry-cli
```

## Test placement

- `foldry-core`: profile matching, scanner rules, output planning, archive writers;
- `foldry-application`: validation, effective profiles, queue/state transitions;
- `foldry-storage`: persistence, migrations, recovery, Archive executor;
- `foldry-cli`: end-to-end commands, JSON envelope, exit codes, cancellation;
- frontend: component behavior, localization, themes, folder browser, queue controls;
- Tauri: explicit command surface, path validation, startup bootstrap.

Platform-specific native behavior still needs manual validation as described in
[Platform support](../platform-support.md).

## Performance smoke

Run:

```bash
cargo run --release -p foldry-core --example performance_smoke
```

The default workload exercises:

- 1,000,000 matcher decisions;
- a 5,000-file scan across 100 directories;
- each archive format with 5,000 small entries and one streamed 64 MiB entry.

Environment overrides:

- `FOLDRY_BENCH_MATCHES`;
- `FOLDRY_BENCH_SMALL_FILES`;
- `FOLDRY_BENCH_LARGE_BYTES`.

Review thresholds, not product promises:

- matcher at least 250,000 decisions/s;
- scanner at least 40,000 entries/s on the default fixture;
- ZIP under 8 seconds;
- TAR.GZ and TAR.ZST under 3 seconds;
- benchmark peak RSS under 64 MiB;
- IPC, Preview, history, and log pages remain bounded.

Compare release builds on the same runner and storage. Do not compare debug and
release timings.

## Release-scale manual data

Before a stable release, test:

- at least 100,000 small files;
- one incompressible file of at least 4 GiB;
- a Unicode-heavy and deep directory tree;
- local SSD and representative network/removable outputs.

The acceptance conditions are bounded memory, responsive cancellation, no partial
published archive, and no unexplained regression on a repeatable runner.
