# Development documentation

These documents describe the current 0.1.2 implementation. They are design
constraints for future work, not a compatibility promise for discarded
pre-release models.

- [Architecture](architecture.md) — layers, domain model, and primary flows.
- [Data and contracts](data-and-contracts.md) — local state, versioning, and
  transport.
- [Security model](security.md) — trust boundaries and archive safety.
- [Testing and performance](testing.md) — quality gate, test placement, and
  benchmark budgets.
- [Release process](releasing.md) — versioning, packages, signing, and promotion.

## Current product boundary

Foldry 0.1.2 is a local single-user application. Folder identity, Ignore Profiles,
Actions, Runs, Preview, queueing, archives, history, and the CLI are implemented.
LAN, SSH, and server synchronization are intentionally outside this release.

Any future network transport should extend the Action model without replacing
Folder identity, profile resolution, immutable Run snapshots, or history.
