import type { StoredPreset } from "../../shared/contracts/generated";

export type PresetInstallationState =
  "absent" | "installed" | "modified" | "outdated";

export type PresetDefinition = {
  id: string;
  version: number;
  name: string;
  description: string;
  sensitive: boolean;
  content: string;
};

type PresetBlock = {
  version: number;
  start: number;
  end: number;
  content: string;
};

export function parsePresetDefinition(preset: StoredPreset): PresetDefinition {
  const lines = preset.text.replaceAll("\r\n", "\n").split("\n");
  const blank = lines.findIndex((line) => line.length === 0);
  const header = blank < 0 ? lines : lines.slice(0, blank);
  const metadata = new Map<string, string>();
  for (const line of header) {
    const match = /^# @preset-([a-z-]+) (.+)$/.exec(line);
    if (match?.[1] && match[2]) {
      metadata.set(match[1], match[2].trim());
    }
  }
  return {
    id: metadata.get("id") ?? preset.id,
    version:
      Number(metadata.get("version") ?? preset.resource_version ?? 1) || 1,
    name: metadata.get("name") ?? preset.id,
    description: metadata.get("description") ?? preset.filename,
    sensitive: metadata.get("safety") === "sensitive",
    content: normalizeContent(
      blank < 0 ? "" : lines.slice(blank + 1).join("\n"),
    ),
  };
}

export function presetState(
  profileText: string,
  preset: PresetDefinition,
): PresetInstallationState {
  const block = findBlock(profileText, preset.id);
  if (!block) {
    return "absent";
  }
  if (
    block.version === preset.version &&
    normalizeContent(block.content) === normalizeContent(preset.content)
  ) {
    return "installed";
  }
  return block.version < preset.version ? "outdated" : "modified";
}

export function insertPreset(
  profileText: string,
  preset: PresetDefinition,
): string {
  if (findBlock(profileText, preset.id)) {
    return profileText;
  }
  let prefix = profileText.replaceAll("\r\n", "\n").replace(/\s*$/, "\n\n");
  if (!prefix.endsWith("\n\n")) {
    prefix += "\n";
  }
  return `${prefix}${renderBlock(preset)}`;
}

export function removePreset(
  profileText: string,
  preset: PresetDefinition,
): string {
  const block = findBlock(profileText, preset.id);
  if (!block) {
    return profileText;
  }
  const edited =
    `${profileText.slice(0, block.start)}${profileText.slice(block.end)}`.replace(
      /\n{3,}/g,
      "\n\n",
    );
  return edited.endsWith("\n") ? edited : `${edited}\n`;
}

export function updatePreset(
  profileText: string,
  preset: PresetDefinition,
): string {
  const block = findBlock(profileText, preset.id);
  if (!block) {
    return insertPreset(profileText, preset);
  }
  return `${profileText.slice(0, block.start)}${renderBlock(preset)}${profileText.slice(block.end)}`;
}

export function changedLines(before: string, after: string) {
  const beforeLines = before.split("\n");
  const afterLines = after.split("\n");
  const prefix = beforeLines.findIndex(
    (line, index) => line !== afterLines[index],
  );
  if (prefix < 0 && beforeLines.length === afterLines.length) {
    return { removed: [], added: [] };
  }
  let beforeEnd = beforeLines.length - 1;
  let afterEnd = afterLines.length - 1;
  while (
    beforeEnd >= Math.max(prefix, 0) &&
    afterEnd >= Math.max(prefix, 0) &&
    beforeLines[beforeEnd] === afterLines[afterEnd]
  ) {
    beforeEnd -= 1;
    afterEnd -= 1;
  }
  return {
    removed: beforeLines.slice(Math.max(prefix, 0), beforeEnd + 1),
    added: afterLines.slice(Math.max(prefix, 0), afterEnd + 1),
  };
}

function findBlock(profileText: string, id: string): PresetBlock | null {
  const escaped = id.replaceAll(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const expression = new RegExp(
    `^# @preset-begin id=${escaped} version=(\\d+)\\r?\\n([\\s\\S]*?)^# @preset-end id=${escaped}(?:\\r?\\n|$)`,
    "m",
  );
  const match = expression.exec(profileText);
  if (!match || match.index === undefined) {
    return null;
  }
  return {
    version: Number(match[1]),
    start: match.index,
    end: match.index + match[0].length,
    content: match[2] ?? "",
  };
}

function renderBlock(preset: PresetDefinition): string {
  return `# @preset-begin id=${preset.id} version=${preset.version}\n${normalizeContent(preset.content)}# @preset-end id=${preset.id}\n`;
}

function normalizeContent(content: string): string {
  return `${content.replaceAll("\r\n", "\n").replace(/\n+$/, "")}\n`;
}
