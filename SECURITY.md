# Security policy

Foldry processes local folders that may contain private files, credentials, and
other sensitive data. Please report security problems privately.

## Supported versions

Security fixes are provided for the latest published 0.1.x release. Pre-release
builds and older development snapshots are not supported.

## Report a vulnerability

Use a
[private GitHub Security Advisory](https://github.com/tychh/foldry/security/advisories/new).
Include the affected version, operating system, reproduction steps, and expected
impact. Use a disposable test tree and remove personal paths, credentials, and
real secrets from reports and attachments.

Please do not open a public issue until a fix or safe disclosure plan is ready.
You can expect an initial response within seven days.

## Scope

Reports about path traversal, symlink or reparse-point handling, archive
publication, unsafe cleanup, privilege boundaries, unintended network access, or
exposure of local state are especially useful.

The current trust boundaries and security assumptions are documented in the
[security model](docs/development/security.md).
