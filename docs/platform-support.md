# Platform support

Foldry is designed for Windows, macOS, and Linux. Automated CI builds and tests
all three platforms, but filesystem and installer behavior still requires manual
release validation.

## Current release targets

| Platform | Architecture        | Desktop artifacts           |
| -------- | ------------------- | --------------------------- |
| Windows  | x86-64              | MSI and NSIS                |
| macOS    | Intel x86-64        | `.app` and DMG              |
| macOS    | Apple Silicon ARM64 | `.app` and DMG              |
| Linux    | x86-64              | Debian package and AppImage |

The minimum native window is 1024×700. The configured Intel macOS minimum is
10.15; Apple Silicon requires macOS 11 or newer.

ARM Linux is not a release target in 0.1.2.

## Automated checks

GitHub Actions runs the complete quality gate and native package build on:

- Ubuntu 24.04 x64;
- Windows Server 2025 x64;
- macOS 15 Intel;
- macOS 15 Apple Silicon.

The gate covers formatting, lint, generated contracts, TypeScript, frontend tests,
Rust tests, workspace builds, and Tauri packaging.

## Manual release checks

Before publishing a platform artifact, verify on a disposable source/output tree:

- launch, folder picker, drag and drop, Favorites, Recent, Tree, and List;
- ZIP, TAR.GZ, and TAR.ZST creation and independent extraction;
- overwrite, skip, increment, full verification, and checksum;
- pause, resume, stop, restart recovery, history, and logs;
- permission-denied and read-only destinations;
- changing/disappearing source files;
- symlinks on Unix and junction/reparse points on Windows;
- Unicode, long paths, removable media, and a representative network mount;
- light/dark themes, keyboard navigation, HiDPI/scaling, and native menus;
- installer signature, uninstall, and preservation of user data.

Unsigned builds are development or release-candidate artifacts. Windows
Authenticode signing and macOS Developer ID signing/notarization are required
before presenting packages as trusted public installers.
