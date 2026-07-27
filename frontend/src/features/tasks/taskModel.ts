import type {
  ActionSpec,
  ArchiveActionSpec,
  Settings,
  Task,
} from "../../shared/contracts/generated";

export function defaultArchiveStep(
  source: string,
  settings: Settings,
): ActionSpec {
  const defaults = settings.archive_defaults;
  return {
    action_type: "archive",
    version: 1,
    archive: {
      version: 1,
      output: {
        directory: defaults.output_directory,
        filename: `${filenameStem(source)}-{date}`,
        format: defaults.format,
        compression: defaults.compression,
        conflict_policy: defaults.conflict_policy,
        extensions: {},
      },
      include_root: defaults.include_root,
      unreadable_policy: defaults.unreadable_policy,
      verification: {
        mode: defaults.verification_mode,
        checksum: defaults.checksum,
        extensions: {},
      },
      extensions: {},
    },
    fields: {},
  };
}

export function updateArchive(
  task: Task,
  update: (archive: ArchiveActionSpec) => ArchiveActionSpec,
): Task {
  const step = task.steps[0];
  if (!step?.archive) {
    return task;
  }
  const steps = [...task.steps];
  steps[0] = { ...step, archive: update(structuredClone(step.archive)) };
  return { ...task, steps };
}

export function basename(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts.at(-1) ?? path;
}

function filenameStem(path: string): string {
  const stem = basename(path)
    .toLowerCase()
    .replaceAll(/[^a-z0-9_-]+/g, "-")
    .replaceAll(/^-+|-+$/g, "");
  return stem || "archive";
}
