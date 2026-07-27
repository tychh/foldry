# Foldry public data formats v1

This document is the concrete stage-two schema reference for ADR-0002. Rust types in
`foldry-core` and `foldry-application` are authoritative; fixtures live under
`tests/fixtures/formats/`.

## Identifiers

- `ProfileId`, `TaskId`, and `RunId` are UUIDv7 strings. Other UUID versions are
  rejected during deserialization.
- `PresetId` is a stable lowercase ASCII slug such as `python` or
  `test-artifacts`. It is 1–64 characters and may contain single internal hyphens.
- Renaming a profile or task does not change its identifier.

## Profile metadata

Profile metadata is stored in leading `.packignore` comments:

```text
# @profile-id 0190f5f0-7f8b-7d80-a120-4f4f9fe95c20
# @profile-version 1
# @profile-name Default
```

`ProfileFormatVersion::CURRENT` is `1`. The parser model includes rules, original
source spans, optional preset provenance, structured diagnostics, and explainable
match results. Parsing and matching behavior is implemented in stage 3.

## Settings v1

The canonical example is
[`settings.yaml`](../../tests/fixtures/formats/v1/settings.yaml). Required top-level
fields are:

- `version: 1`;
- `locale: en | ru`;
- `appearance: system | light | dark`;
- optional `default_profile_id`;
- `archive_defaults`;
- `execution.max_parallel_runs`, currently constrained to `1..=64`;
- separate `history.runs` and `history.logs` retention policies.

A finite retention policy requires non-zero `max_age_days` and `max_entries`.
`unlimited: true` disables both limits. Defaults are 365 days/10,000 runs and
90 days/1,000 detailed logs.

## Plan v1

The canonical example is
[`plan.packplan.yaml`](../../tests/fixtures/formats/v1/plan.packplan.yaml).

- A plan has `version: 1`, a non-empty name, and zero or more tasks.
- Task and source values must be unique within the serialized plan. Canonical
  filesystem identity is additionally checked by the application when adding or
  resolving a source; loading a plan never requires an offline source to exist.
- Each v1 task has exactly one ordered action step.
- The known `archive` action has its own `version: 1`.
- Archive format is `zip`, `tar_gz`, or `tar_zst`.
- Compression is semantic: `fast`, `balanced`, or `maximum`.
- Conflict behavior is `skip`, `overwrite`, or `increment`.
- Unreadable entry behavior is `fail` or `warn_and_skip`.
- Verification is `structural` or `full`; final checksum is `none` or `sha256`.

An unknown action `type` or future action version is decoded without losing its
payload. The plan remains editable and saveable, but `execution_blockers()` prevents
the unsupported step from running. A malformed known `archive` action is an error;
it is never silently downgraded to an unknown action.

## Forward compatibility

Unknown fields in a compatible major version are preserved in ordered extension
maps at every public document level. Saving is blocked if an extension attempts to
shadow a known field. A future document major version is rejected before typed
deserialization and is never rewritten.

All writers produce UTF-8 YAML with LF line endings and a final newline. YAML keys
must be strings and values must be JSON-compatible so the same extensions can cross
the IPC boundary safely.

## Validation and migrations

Syntax failures report line and column. Typed failures report a JSONPath-like field
location, for example `$.tasks[0].steps[0]`. Structural validation returns all
issues, including duplicate IDs and sources, instead of stopping after the first.

Every document is routed through a sequential `MigrationRegistry`. Version 1 is the
first released schema and therefore has no predecessor migration. Future migrations
must be registered as contiguous `N -> N+1` steps; gaps and downgrade attempts fail
without modifying the source.
