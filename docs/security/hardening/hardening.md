# Security Hardening Review: Foldry v1

## Evidence Basis

I inspected the archive, scanner, manifest, recovery, IPC, capability, and CSP
boundaries listed in [context.md](context.md). The working tree is a new rewrite
rather than an immutable revision, so this review is bound to the recorded
ten-file digest and marks source drift as present. We also exercised the
relevant regression, generated-input, dependency, and browser accessibility
checks.

The detailed trust-boundary analysis is in
[threat-review.md](../threat-review.md). No reportable remote attack surface was
found: the application has no runtime network client or telemetry path, and the
webview receives only core Tauri permissions while privileged desktop actions
remain validated Rust commands.

## Constraints

We use a balanced v1 profile: local single-user desktop operation, no service or
process isolation, bounded-memory streaming, and preservation of symlinks as
archive entries without following them. The OS account that owns the config,
source, and output paths remains trusted. Archive consumers and unrelated local
processes are outside Foldry's control.

## Opportunity Portfolio

No structural hardening opportunity qualified. Scanner, executor, output
publication, persistence, and desktop integration each already have a single
owned control boundary. Introducing a broker process or capability filesystem
layer would add deployment and compatibility cost without evidence of a remote
or cross-user boundary that needs that isolation in v1.

Two assumptions inside the existing executor did warrant local remediation:
manifest paths are now rebound to the source root before use, and regular files
are opened without following a link introduced after scan. Both changes are in
the existing choke point and have direct regression coverage.

## Recommendation Summary

I recommend keeping the current architecture and treating the implemented
executor guards as the proportionate design. We preserve the fast streaming
path, bounded memory, and platform-neutral scheduler while removing the
credible scan-to-open substitution path. The output reservation protocol,
create-new sidecars, run-owned temp files, verification-before-publication, and
startup reconciliation remain the stronger controls for data-loss prevention.

The main residual concern is a same-user process racing replacement of the
output directory. A directory-handle/capability redesign becomes preferable if
Foldry later runs across privilege boundaries, archives attacker-writable shared
trees, or exposes commands to non-local web content. Until then, the
compatibility and migration cost is disproportionate to the threat model.

## Next Decisions

The remaining release decision is operational rather than architectural:
Windows and macOS manual smoke coverage cannot be performed from the current
Linux workspace. The required matrix and exact commands are in
[platform-validation.md](../../platform-validation.md).
