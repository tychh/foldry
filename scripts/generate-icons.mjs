import { execFileSync } from "node:child_process";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const workspace = dirname(dirname(fileURLToPath(import.meta.url)));
const darkSource = join(workspace, "extra", "foldry-icon-source-dark.png");
const lightSource = join(workspace, "extra", "foldry-icon-source-light.png");
const tauriIcons = join(workspace, "crates", "foldry-tauri", "icons");
const publicAssets = join(workspace, "frontend", "public");
const temporary = mkdtempSync(join(tmpdir(), "foldry-icons-"));
const pnpm = process.platform === "win32" ? "pnpm.cmd" : "pnpm";

for (const source of [darkSource, lightSource]) {
  if (!existsSync(source)) {
    throw new Error(`Missing icon source: ${source}`);
  }
}

mkdirSync(tauriIcons, { recursive: true });
mkdirSync(publicAssets, { recursive: true });

function iconSvg(source, frame) {
  const png = readFileSync(source).toString("base64");
  const image = `<image href="data:image/png;base64,${png}" x="${frame.x}" y="${frame.y}" width="${frame.size}" height="${frame.size}" preserveAspectRatio="xMidYMid slice"/>`;

  if (frame.radius === 0) {
    return `<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 1024 1024">${image}</svg>`;
  }

  return `<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 1024 1024"><defs><clipPath id="icon-frame"><rect x="${frame.x}" y="${frame.y}" width="${frame.size}" height="${frame.size}" rx="${frame.radius}" ry="${frame.radius}"/></clipPath></defs><g clip-path="url(#icon-frame)">${image}</g></svg>`;
}

function generate(name, source, frame) {
  const input = join(temporary, `${name}.svg`);
  const output = join(temporary, name);
  writeFileSync(input, iconSvg(source, frame));
  execFileSync(pnpm, ["exec", "tauri", "icon", input, "--output", output], {
    cwd: workspace,
    stdio: "inherit",
  });
  return output;
}

function copy(source, destination) {
  mkdirSync(dirname(destination), { recursive: true });
  copyFileSync(source, destination);
}

try {
  // Tauri's legacy ICNS path can expose the supplied pixels directly in the
  // Dock, so bake the macOS-style rounded silhouette into the alpha channel.
  const macos = generate("macos", darkSource, {
    x: 64,
    y: 64,
    size: 896,
    radius: 196,
  });

  // A small transparent gutter and moderate radius match the Windows icon grid
  // while retaining enough contrast on both light and dark system surfaces.
  const windows = generate("windows", darkSource, {
    x: 32,
    y: 32,
    size: 960,
    radius: 128,
  });

  // Linux desktops do not share one mandatory mask, so ship a neutral,
  // self-contained silhouette that works across common docks and launchers.
  const linux = generate("linux", darkSource, {
    x: 24,
    y: 24,
    size: 976,
    radius: 144,
  });

  const guiDark = generate("gui-dark", darkSource, {
    x: 0,
    y: 0,
    size: 1024,
    radius: 208,
  });
  const guiLight = generate("gui-light", lightSource, {
    x: 0,
    y: 0,
    size: 1024,
    radius: 208,
  });

  copy(join(macos, "icon.icns"), join(tauriIcons, "icon.icns"));
  copy(join(windows, "icon.ico"), join(tauriIcons, "icon.ico"));

  for (const filename of readdirSync(windows)) {
    if (filename === "StoreLogo.png" || filename.startsWith("Square")) {
      copy(join(windows, filename), join(tauriIcons, filename));
    }
  }

  for (const filename of [
    "32x32.png",
    "64x64.png",
    "128x128.png",
    "128x128@2x.png",
    "icon.png",
  ]) {
    const source = join(linux, filename);
    if (existsSync(source)) {
      copy(source, join(tauriIcons, filename));
    }
  }

  copy(join(guiDark, "128x128.png"), join(publicAssets, "app-icon-dark.png"));
  copy(join(guiLight, "128x128.png"), join(publicAssets, "app-icon-light.png"));
  copy(join(guiDark, "32x32.png"), join(publicAssets, "favicon.png"));

  // Keep the dark artwork as the canonical source for tooling and packaging.
  copy(darkSource, join(workspace, "resources", "app-icon.png"));
} finally {
  rmSync(temporary, { force: true, recursive: true });
}
