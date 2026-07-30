import type {
  ActionSpec,
  ArchiveActionSpec,
  ArchiveOutputDirectory,
  FolderAction,
  Settings,
  Folder,
} from "../../shared/contracts/generated";

export function defaultArchiveActionSpec(settings: Settings): ActionSpec {
  const defaults = settings.archive_defaults;
  const directory: ArchiveOutputDirectory = defaults.output_directory
    ? { mode: "custom", path: defaults.output_directory }
    : { mode: "parent" };
  return {
    action_type: "archive",
    version: 1,
    archive: {
      version: 1,
      output: {
        directory,
        filename: "{folder}.{date}",
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
  folder: Folder,
  actionId: string,
  update: (archive: ArchiveActionSpec) => ArchiveActionSpec,
): Folder {
  const index = folder.actions.findIndex((action) => action.id === actionId);
  const action = folder.actions[index];
  if (!action?.spec.archive) {
    return folder;
  }
  const actions = [...folder.actions];
  actions[index] = {
    ...action,
    spec: {
      ...action.spec,
      archive: update(structuredClone(action.spec.archive)),
    },
  };
  return { ...folder, actions };
}

export function archiveActions(folder: Folder): FolderAction[] {
  return folder.actions.filter(
    (action) => action.spec.action_type === "archive" && action.spec.archive,
  );
}

export function basename(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts.at(-1) ?? path;
}
