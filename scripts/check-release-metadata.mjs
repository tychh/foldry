import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repository = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const tauriConfigPath = resolve(
  repository,
  "crates/foldry-tauri/tauri.conf.json",
);
const frontendPackagePath = resolve(repository, "frontend/package.json");

const tauriConfig = JSON.parse(readFileSync(tauriConfigPath, "utf8"));
const frontendPackage = JSON.parse(readFileSync(frontendPackagePath, "utf8"));
const cargoMetadata = JSON.parse(
  execFileSync("cargo", ["metadata", "--format-version", "1", "--no-deps"], {
    cwd: repository,
    encoding: "utf8",
  }),
);

const errors = [];
const workspacePackageIds = new Set(cargoMetadata.workspace_members);
const workspaceVersions = new Set(
  cargoMetadata.packages
    .filter((entry) => workspacePackageIds.has(entry.id))
    .map((entry) => entry.version),
);

if (workspaceVersions.size !== 1) {
  errors.push(
    `workspace package versions differ: ${[...workspaceVersions].join(", ")}`,
  );
}

const [cargoVersion] = workspaceVersions;
for (const [source, version] of [
  ["Tauri", tauriConfig.version],
  ["frontend", frontendPackage.version],
]) {
  if (version !== cargoVersion) {
    errors.push(
      `${source} version ${String(version)} differs from Cargo ${String(cargoVersion)}`,
    );
  }
}

if (tauriConfig.productName !== "Foldry") {
  errors.push("Tauri productName must be Foldry");
}
if (tauriConfig.identifier !== "app.foldry.desktop") {
  errors.push("Tauri identifier must be app.foldry.desktop");
}
if (tauriConfig.bundle?.active !== true) {
  errors.push("Tauri bundle.active must be true");
}

const configDirectory = dirname(tauriConfigPath);
for (const icon of tauriConfig.bundle?.icon ?? []) {
  if (!existsSync(resolve(configDirectory, icon))) {
    errors.push(`bundle icon does not exist: ${icon}`);
  }
}
for (const resource of Object.keys(tauriConfig.bundle?.resources ?? {})) {
  if (!existsSync(resolve(configDirectory, resource))) {
    errors.push(`bundle resource does not exist: ${resource}`);
  }
}

const gitRef = process.env.GITHUB_REF_NAME;
if (gitRef?.startsWith("v") && gitRef.slice(1) !== cargoVersion) {
  errors.push(`tag ${gitRef} does not match package version ${cargoVersion}`);
}

if (errors.length > 0) {
  for (const error of errors) {
    process.stderr.write(`release metadata error: ${error}\n`);
  }
  process.exitCode = 1;
} else {
  process.stdout.write(`release metadata ${cargoVersion}: ok\n`);
}
