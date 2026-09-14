import type {
  ActionSpec,
  ArchiveActionSpec,
  ArchiveOutputDirectory,
  FolderAction,
  Settings,
  Folder,
  FolderOperationalSummary,
  FolderSortMode,
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

export function filterFolders(folders: Folder[], search: string): Folder[] {
  const needle = search.trim().toLocaleLowerCase();
  if (!needle) return folders;
  return folders.filter(
    (folder) =>
      basename(folder.source).toLocaleLowerCase().includes(needle) ||
      folder.source.toLocaleLowerCase().includes(needle),
  );
}

export function sortFolders(
  folders: Folder[],
  mode: FolderSortMode,
  summaries: ReadonlyMap<string, FolderOperationalSummary>,
): Folder[] {
  const result = [...folders];
  const stableName = (left: Folder, right: Folder) =>
    basename(left.source).localeCompare(basename(right.source), undefined, {
      sensitivity: "base",
    }) ||
    left.source.localeCompare(right.source) ||
    left.id.localeCompare(right.id);
  result.sort((left, right) => {
    if (mode === "name_ascending") return stableName(left, right);
    if (mode === "name_descending") return -stableName(left, right);
    if (mode === "recently_added")
      return (
        right.created_at.localeCompare(left.created_at) ||
        stableName(left, right)
      );
    if (mode === "oldest_added")
      return (
        left.created_at.localeCompare(right.created_at) ||
        stableName(left, right)
      );
    const leftRun = summaries.get(left.id)?.latest_run_at ?? null;
    const rightRun = summaries.get(right.id)?.latest_run_at ?? null;
    if (leftRun === null && rightRun === null) return stableName(left, right);
    if (mode === "recently_run") {
      if (leftRun === null) return 1;
      if (rightRun === null) return -1;
      return rightRun.localeCompare(leftRun) || stableName(left, right);
    }
    if (leftRun === null) return -1;
    if (rightRun === null) return 1;
    return leftRun.localeCompare(rightRun) || stableName(left, right);
  });
  return result;
}
