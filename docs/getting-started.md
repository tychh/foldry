# Getting started

Foldry is available as a desktop application and a standalone command-line tool.
Both use the same Ignore Profiles, archive engine, validation, and local history.

## Install a release

Open [GitHub Releases](https://github.com/tychh/foldry/releases), choose version
0.1.2 or newer, and download the package for your platform:

| Platform            | Desktop package             | CLI          |
| ------------------- | --------------------------- | ------------ |
| Windows x64         | NSIS installer or MSI       | `foldry.exe` |
| macOS Intel         | x64 DMG with `Foldry.app`   | `foldry`     |
| macOS Apple Silicon | ARM64 DMG with `Foldry.app` | `foldry`     |
| Linux x64           | AppImage or Debian package  | `foldry`     |

The initial release candidates may be unsigned. Verify the release checksum when
one is provided and read the release notes before bypassing an operating-system
warning.

## Build from source

The repository pins Rust 1.97.1, Node.js 24.18.0, and pnpm 11.17.0.

Install the common toolchain:

1. install [Rust through rustup](https://rustup.rs/);
2. install Node.js 24;
3. enable Corepack;
4. install the platform requirements below.

Then:

```bash
git clone https://github.com/tychh/foldry.git
cd foldry
corepack enable
pnpm install --frozen-lockfile
pnpm check
pnpm desktop:build
```

Native packages are written to `target/release/bundle/`. The standalone CLI is
written to `target/release/foldry` on Unix and
`target/release/foldry.exe` on Windows.

### Linux

On Ubuntu or Debian:

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

Other distributions need equivalent GTK, WebKitGTK 4.1, OpenSSL, AppIndicator,
RSVG, compiler, and linker packages.

### Windows

Install Microsoft C++ Build Tools with **Desktop development with C++** and the
Microsoft Edge WebView2 Evergreen Runtime. MSI packaging may require the Windows
VBSCRIPT optional feature.

Use PowerShell for the workspace commands.

### macOS

Install Xcode Command Line Tools:

```bash
xcode-select --install
```

Public distribution requires a Developer ID Application identity and Apple
notarization. An ad-hoc build is suitable for local development.

## Create your first archive

1. Start Foldry.
2. Select **Add folders**.
3. Pick a folder in Locations, Favorites, Recent, Tree, or List.
4. Keep **Default** as the Ignore Profile for a minimal first run.
5. Configure the Archive action and open **Preview**.
6. Review excluded entries and run the action.

The Default profile excludes only common operating-system metadata. Add presets or
edit the profile when you also want to ignore language, build, editor, or sensitive
files.

## Run the CLI

From a source checkout:

```bash
cargo run -p foldry-cli --bin foldry -- --help
```

After a release build:

```bash
target/release/foldry archive ./example --format zip
```

Read the [CLI guide](cli.md) for configured workflows and automation.

## Development mode

Start the frontend only:

```bash
pnpm dev
```

Start the native desktop shell with hot reload:

```bash
pnpm desktop:dev
```

The browser frontend uses demo data when it is not running inside Tauri. Filesystem
dialogs and real operations require the native desktop shell or CLI.
