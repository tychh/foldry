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

## Candidate build

Run:

```bash
pnpm check
pnpm desktop:build
cargo build --release -p foldry-cli --bin foldry
```

GitHub Actions repeats the complete quality gate and builds:

| Runner                  | Desktop artifacts        |
| ----------------------- | ------------------------ |
| Ubuntu 24.04 x64        | Debian package, AppImage |
| Windows Server 2025 x64 | MSI, NSIS                |
| macOS 15 Intel          | `.app`, x64 DMG          |
| macOS 15 Apple Silicon  | `.app`, ARM64 DMG        |

The current CI uploads native candidates as workflow artifacts for 14 days. A
public GitHub Release is promoted manually until a dedicated signed release
workflow is added.

## Promotion checklist

1. Confirm `pnpm check` and all three native CI builds pass.
2. Download each artifact and record its checksum.
3. Complete the manual checks in [Platform support](../platform-support.md).
4. Confirm installers do not remove user configuration, history, or archives.
5. Prepare concise release notes with changes and known limitations.
6. Sign/notarize public installers or clearly mark them as unsigned candidates.
7. Create tag `v<version>` and publish the matching GitHub Release assets.

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
