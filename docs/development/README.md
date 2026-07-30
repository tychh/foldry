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
- [Internal CLI reference](cli.md) — development-only command surface and test
  contract.

## Current product boundary

Foldry 0.1.2 is a local single-user desktop application. Folder identity, Ignore
Profiles, Actions, Runs, Preview, queueing, archives, and history are implemented.
The workspace also contains an internal CLI adapter for development and automated
testing; it is not included in release packages or supported as a public
interface. LAN, SSH, and server synchronization are intentionally outside this
release.

Any future network transport should extend the Action model without replacing
Folder identity, profile resolution, immutable Run snapshots, or history.
