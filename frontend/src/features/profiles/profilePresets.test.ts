import { describe, expect, it } from "vitest";

import {
  changedLines,
  insertPreset,
  parsePresetDefinition,
  presetState,
  removePreset,
  resolvePresetDefinitions,
  updatePreset,
} from "./profilePresets";

const profile =
  "# @profile-id 01982ce0-7381-7d55-9a28-7c932a635d24\n" +
  "# @profile-version 1\n" +
  "# @profile-name Test\n\n";
const definition = parsePresetDefinition({
  id: "rust",
  filename: "rust.packignore",
  text:
    "# @preset-id rust\n" +
    "# @preset-version 2\n" +
    "# @preset-name Rust\n" +
    "# @preset-description Cargo output.\n" +
    "# @preset-safety safe\n\n" +
    "target/\n",
  resource_version: 2,
});

describe("profile preset edits", () => {
  it("inserts once and removes the exact installed block", () => {
    const installed = insertPreset(profile, definition);

    expect(presetState(installed, definition)).toBe("installed");
    expect(insertPreset(installed, definition)).toBe(installed);
    expect(removePreset(installed, definition)).toBe(profile);
  });

  it("distinguishes modified and outdated blocks", () => {
    const modified = insertPreset(profile, definition).replace(
      "target/",
      "target/debug/",
    );
    const outdated = modified.replace("version=2", "version=1");

    expect(presetState(modified, definition)).toBe("modified");
    expect(presetState(outdated, definition)).toBe("outdated");
    expect(presetState(updatePreset(outdated, definition), definition)).toBe(
      "installed",
    );
  });

  it("builds a bounded line diff for update confirmation", () => {
    const modified = insertPreset(profile, definition).replace(
      "target/",
      "target/debug/",
    );
    const diff = changedLines(modified, updatePreset(modified, definition));

    expect(diff.removed).toContain("target/debug/");
    expect(diff.added).toContain("target/");
  });

  it("flattens resource-only includes and upgrades dynamic profiles to v2", () => {
    const definitions = resolvePresetDefinitions([
      {
        id: "component",
        filename: "component.packignore",
        text: "# @preset-id component\n# @preset-version 1\n# @preset-name Component\n# @preset-description Component.\n# @preset-safety safe\n\ntarget/\n",
        resource_version: 1,
      },
      {
        id: "development",
        filename: "development.packignore",
        text: "# @preset-id development\n# @preset-version 1\n# @preset-name Development\n# @preset-description Dynamic.\n# @preset-safety safe\n# @preset-include id=component\n\n# @rule-source gitignore mode=exclude-unignored nested=true\n",
        resource_version: 1,
      },
    ]);
    const development = definitions.find(
      (preset) => preset.id === "development",
    )!;
    const installed = insertPreset(profile, development);

    expect(development.content).toContain("target/");
    expect(installed).toContain("# @profile-version 2");
    expect(installed).toContain("# @rule-source gitignore");
  });
});
