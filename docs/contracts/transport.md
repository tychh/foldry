# Rust and TypeScript transport contracts

Transport DTOs are defined in
`crates/foldry-application/src/transport.rs`. They are the source for the generated
frontend file:

```text
frontend/src/shared/contracts/generated.ts
```

Generate bindings after changing a DTO:

```bash
pnpm contracts:generate
```

Verify that the committed file is current:

```bash
pnpm contracts:check
```

`pnpm typecheck` and CI run the drift check before TypeScript. Editing the generated
file by hand is unsupported.

Top-level settings, plans, profiles, actions, and run events carry explicit version
fields. Paths, UUIDs, timestamps, and preset IDs cross IPC as strings. Rust `u64`
counters, byte sizes, durations, and event sequences cross as decimal strings to
avoid precision loss in JSON and JavaScript.

Unknown compatible fields use a recursive `JsonValue`. Unknown plan actions have an
explicit `action_type`, optional version, and preserved field map in transport DTOs;
the Rust domain decides whether they can execute.
