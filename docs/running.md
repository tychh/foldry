# Building and running Foldry

## Pinned toolchain

- Rust 1.97.1 through `rustup`, with `rustfmt` and `clippy`;
- Node.js 24.18.0;
- pnpm 11.17.0 through Corepack;
- platform tools required by Tauri 2.

Clone the repository, then install JavaScript dependencies:

```bash
corepack enable
pnpm install --frozen-lockfile
```

The lockfiles and `rust-toolchain.toml` are authoritative. Do not substitute
unrelated global Tauri, pnpm, or Rust versions when reproducing CI.

## Linux prerequisites

Ubuntu 22.04 and Debian 12 are the release build baseline. On Debian-family
systems:

```bash
sudo apt update
sudo apt install --yes \
  build-essential \
  curl \
  file \
  libayatana-appindicator3-dev \
  librsvg2-dev \
  libssl-dev \
  libwebkit2gtk-4.1-dev \
  libxdo-dev \
  patchelf \
  wget
```

For another distribution, install its WebKitGTK 4.1, GTK, OpenSSL, AppIndicator,
RSVG, compiler, and linker equivalents. Build Linux release artifacts on the
oldest supported base system; a newer glibc can make the binary unusable on older
distributions.

## Windows prerequisites

Install:

1. Microsoft C++ Build Tools with **Desktop development with C++**;
2. Microsoft Edge WebView2 Evergreen Runtime;
3. `rustup` and the pinned Rust toolchain;
4. Node.js and Corepack.

MSI packaging also needs the Windows optional VBSCRIPT feature. If WiX reports
`failed to run light.exe`, enable VBSCRIPT under Windows optional features and
retry. Run commands from PowerShell:

```powershell
corepack enable
pnpm install --frozen-lockfile
pnpm check
pnpm desktop:build
```

## macOS prerequisites

Install Xcode Command Line Tools for desktop-only development:

```bash
xcode-select --install
```

Then install `rustup`, Node.js, and enable Corepack. A distributable macOS build
requires a Developer ID Application identity and Apple notarization credentials.
Unsigned/ad-hoc bundles are suitable only for development and the documented test
matrix.

## Development commands

Run the browser frontend with hot reload:

```bash
pnpm dev
```

Run the native desktop shell:

```bash
pnpm desktop:dev
```

Run the CLI:

```bash
cargo run -p foldry-cli --bin foldry -- --help
```

## Verification and builds

The complete local quality gate is:

```bash
pnpm check
```

It runs formatting, ESLint/Clippy, generated-contract drift, TypeScript, frontend
and Rust tests, and production builds. For changes to Tauri configuration,
capabilities, icons, resources, or desktop integration, also build native bundles:

```bash
pnpm desktop:build
```

Useful narrower commands:

```bash
pnpm format:check
pnpm lint
pnpm typecheck
pnpm contracts:check
pnpm test
pnpm build
cargo run --release -p foldry-core --example performance_smoke
```

Tauri writes installers under `target/release/bundle/`. Cargo writes the standalone
CLI as `target/release/foldry` on Unix and `target/release/foldry.exe` on Windows.

## Isolated development and recovery

The hidden CLI flags below redirect all mutable application state:

```bash
foldry \
  --config-dir /tmp/foldry/config \
  --data-dir /tmp/foldry/data \
  --cache-dir /tmp/foldry/cache \
  config path
```

They are intended for tests, development, and recovery, not as a supported portable
installation mode. `FOLDRY_RESOURCE_DIR` may point an unpackaged development build
at the repository `resources/` directory.

## Release builds

Pushing a `v*` tag runs `.github/workflows/release.yml`. It builds native bundles
and the standalone CLI on Linux, Windows, and macOS, collects them as workflow
artifacts, generates an SPDX JSON SBOM and `SHA256SUMS`, and creates a draft GitHub
release. A manual workflow dispatch performs the same build without publishing a
release.

The workflow currently produces unsigned candidates because the repository has no
signing credentials. The credential/import integration described in
[`releasing.md`](releasing.md) must be configured before public promotion. An
unsigned artifact must remain a draft/test artifact and cannot pass the manual
release checklist in `notes/multiplatform-smoke.md`.
