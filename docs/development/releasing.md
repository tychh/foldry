# Release process

## Version invariant

The release version must match in:

- workspace Cargo package metadata;
- `frontend/package.json`;
- root `package.json`;
- `crates/foldry-tauri/tauri.conf.json`;
- `Cargo.lock`.

Check it with:

```bash
pnpm release:check
```

## Candidate builds

Run:

```bash
pnpm check
pnpm desktop:build
```

Pull requests run the quality gate without producing native packages. Every push
to `main` runs the quality gate and builds the complete native matrix:

| Runner                  | Desktop artifacts             |
| ----------------------- | ----------------------------- |
| Ubuntu 22.04 x64        | Debian package, RPM, AppImage |
| Windows Server 2025 x64 | MSI, NSIS                     |
| macOS 15 Intel          | `.app`, x64 DMG               |
| macOS 15 Apple Silicon  | `.app`, ARM64 DMG             |

The **Build artifacts** workflow uploads native candidates as workflow artifacts
for 14 days. For another branch, run it manually in GitHub Actions, select the
branch, and choose either the complete matrix or one platform. The equivalent
GitHub CLI request is:

```bash
gh workflow run artifacts.yml --ref <branch> -f platform=all
```

Version 0.1.2 distributes only the desktop packages listed above. The
`foldry-cli` crate remains an internal development and test adapter and must not
be uploaded as a release asset. Installer integration can be introduced in a
later version as one coordinated desktop-and-CLI package.

## Public release

Pushing a matching `v*` tag repeats the quality gate and complete native build,
creates `SHA256SUMS`, and immediately publishes a public GitHub Release with
generated release notes. For example:

```bash
git tag v0.1.2
git push origin v0.1.2
```

The tag version must exactly match the package metadata. The release workflow is
not manually dispatchable and branch builds cannot publish a release.

## Promotion checklist

1. Confirm `pnpm check` and all four native CI builds pass.
2. Download each artifact and record its checksum.
3. Complete the manual checks in [Platform support](../platform-support.md).
4. Confirm installers do not remove user configuration, history, or archives.
5. Prepare concise release notes with changes and known limitations.
6. Sign/notarize public installers or clearly document that they are unsigned.
7. Create and push tag `v<version>`; this immediately publishes the matching
   GitHub Release assets.

## Signing

Windows public packages require Authenticode signing. macOS public packages
require a Developer ID Application signature, Apple notarization, and stapling.
Credentials must remain outside the repository and CI logs.

Verify macOS artifacts with:

```bash
codesign --verify --deep --strict --verbose=2 Foldry.app
spctl --assess --type execute --verbose=4 Foldry.app
xcrun stapler validate Foldry.app
```

Linux package signing is optional for the first candidate. Checksums are still
required for every published asset.

## Data ownership

Uninstalling Foldry must not be presented as deleting user archives or application
state. Data removal is a separate explicit operation. Upgrade tests must confirm
that profiles, settings, folders, history, and existing archives survive.
