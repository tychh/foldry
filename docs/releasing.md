# Foldry release process

## Release invariants

A release candidate must have:

- matching versions in `Cargo.toml`, `frontend/package.json`, and
  `crates/foldry-tauri/tauri.conf.json`;
- a clean `pnpm check` and native bundle build;
- Linux, Windows, and macOS bundles plus a standalone `foldry` CLI;
- an SPDX JSON SBOM and verified `SHA256SUMS`;
- no open product/data-safety decision;
- completed practical checks in `notes/multiplatform-smoke.md`;
- valid Windows/macOS signatures and macOS notarization for a public stable release.

## Build targets

| Platform            | Desktop artifacts         | CLI               |
| ------------------- | ------------------------- | ----------------- |
| Linux x86-64        | Debian, RPM, and AppImage | ELF executable    |
| Windows x86-64      | NSIS setup and MSI        | `foldry.exe`      |
| macOS native runner | `.app` and DMG            | Mach-O executable |

The workflow currently uses the native architecture of `macos-latest`. Add explicit
Intel and Apple Silicon matrix targets before promising universal macOS support.

## Automated candidate

Run `.github/workflows/release.yml` manually to produce private workflow artifacts,
or push a `v*` tag to additionally create/update a draft GitHub release:

```bash
git tag v0.1.0
git push origin v0.1.0
```

The workflow:

1. repeats the full quality gate;
2. builds native desktop bundles and the CLI on each OS;
3. prefixes filenames with platform/architecture;
4. generates a source/dependency SPDX JSON SBOM;
5. generates and verifies `SHA256SUMS`;
6. uploads one assembled artifact;
7. leaves tag-triggered GitHub releases as drafts.

Verify downloaded assets locally:

```bash
sha256sum --check SHA256SUMS
```

Use `Get-FileHash -Algorithm SHA256` on Windows when GNU `sha256sum` is
unavailable.

## Signing and notarization

Credentials are intentionally not stored in the repository. Until they are
configured, workflow outputs are unsigned test candidates and must remain drafts.

For Windows, import the organization’s code-signing certificate into the CI user
certificate store and configure Tauri’s Windows signing identity or `signCommand`.
Verify the signature on both the executable and installer.

For macOS, import a Developer ID Application `.p12` into a temporary CI keychain,
set `APPLE_SIGNING_IDENTITY`, and provide Apple notarization credentials
(`APPLE_ID`, app-specific `APPLE_PASSWORD`, and `APPLE_TEAM_ID`, or an accepted API
key flow). Verify:

```bash
codesign --verify --deep --strict --verbose=2 Foldry.app
spctl --assess --type execute --verbose=4 Foldry.app
xcrun stapler validate Foldry.app
```

Linux package signing is optional for the first candidate. If enabled, keep the
private GPG key outside the repository, publish its fingerprint through an
authenticated channel, and verify the detached/package signature separately from
`SHA256SUMS`.

## Manual promotion

Install the assembled candidate on clean or disposable machines and complete
`notes/multiplatform-smoke.md`. Confirm upgrade/uninstall preserves config, data,
profiles, history, and archives. Record the tested artifact checksum and OS build.

Only then:

1. verify the release notes and known limitations;
2. confirm every release asset is represented in `SHA256SUMS`;
3. confirm the SBOM parses as SPDX JSON;
4. change the GitHub release from draft to published.

Deleting an application or uninstalling a package must not be presented as a way
to delete user archives or application data. Data removal is a separate,
explicitly documented user operation.
