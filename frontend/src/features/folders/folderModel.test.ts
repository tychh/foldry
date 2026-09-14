import { describe, expect, it } from "vitest";
import type {
  Folder,
  FolderOperationalSummary,
} from "../../shared/contracts/generated";

import {
  defaultArchiveActionSpec,
  filterFolders,
  sortFolders,
  updateArchive,
} from "./folderModel";

const settings = {
  version: 1,
  locale: "en" as const,
  appearance: "system" as const,
  default_profile_id: null,
  archive_defaults: {
    output_directory: "/backups",
    format: "tar_zst" as const,
    compression: "maximum" as const,
    conflict_policy: "increment" as const,
    include_root: true,
    unreadable_policy: "fail" as const,
    verification_mode: "structural" as const,
    checksum: "none" as const,
    extensions: {},
  },
  execution: { max_parallel_runs: 2, extensions: {} },
  history: {
    runs: {
      unlimited: false,
      max_age_days: 365,
      max_entries: 10_000,
      extensions: {},
    },
    logs: {
      unlimited: false,
      max_age_days: 90,
      max_entries: 1_000,
      extensions: {},
    },
    extensions: {},
  },
  browser: {
    favorites: [],
    recent: [],
    view: "tree" as const,
    extensions: {},
  },
  folder_sort_mode: "oldest_added" as const,
  extensions: {},
};

describe("folder model helpers", () => {
  it("creates an archive action from defaults", () => {
    const spec = defaultArchiveActionSpec(settings);

    expect(spec.archive?.output).toMatchObject({
      directory: { mode: "custom", path: "/backups" },
      filename: "{folder}.{date}",
      format: "tar_zst",
      compression: "maximum",
    });
  });

  it("updates one archive without mutating the folder snapshot", () => {
    const spec = defaultArchiveActionSpec(settings);
    const folder = {
      id: "folder",
      source: "/source",
      created_at: "2026-01-01T00:00:00Z",
      listed: true,
      enabled: true,
      default_profile_id: "profile",
      actions: [
        {
          id: "action",
          enabled: true,
          profile_id_override: null,
          spec,
          extensions: {},
        },
      ],
      extensions: {},
    };
    const updated = updateArchive(folder, "action", (archive) => ({
      ...archive,
      include_root: false,
    }));

    expect(folder.actions[0]?.spec.archive?.include_root).toBe(true);
    expect(updated.actions[0]?.spec.archive?.include_root).toBe(false);
  });

  it("filters basename and full path case-insensitively", () => {
    const folders = [folder("a", "/Users/ME/Проект", "2026-01-01T00:00:00Z")];

    expect(filterFolders(folders, "пРоЕкТ")).toHaveLength(1);
    expect(filterFolders(folders, "users/me")).toHaveLength(1);
    expect(filterFolders(folders, "missing")).toEqual([]);
  });

  it("sorts all modes with stable ties and consistent never-run placement", () => {
    const old = folder("b", "/zeta/Same", "2025-01-01T00:00:00Z");
    const recent = folder("a", "/alpha/Same", "2026-01-01T00:00:00Z");
    const never = folder("c", "/never", "2024-01-01T00:00:00Z");
    const summaries = new Map([
      [old.id, summary(old.id, "2026-01-01T00:00:00Z")],
      [recent.id, summary(recent.id, "2026-02-01T00:00:00Z")],
      [never.id, summary(never.id, null)],
    ]);

    expect(
      sortFolders([old, recent], "name_ascending", summaries).map(
        (item) => item.id,
      ),
    ).toEqual(["a", "b"]);
    expect(
      sortFolders([old, recent], "name_descending", summaries).map(
        (item) => item.id,
      ),
    ).toEqual(["b", "a"]);
    expect(sortFolders([old, recent], "recently_added", summaries)[0]?.id).toBe(
      "a",
    );
    expect(sortFolders([old, recent], "oldest_added", summaries)[0]?.id).toBe(
      "b",
    );
    expect(
      sortFolders([never, old, recent], "recently_run", summaries).map(
        (item) => item.id,
      ),
    ).toEqual(["a", "b", "c"]);
    expect(
      sortFolders([old, recent, never], "least_recently_run", summaries).map(
        (item) => item.id,
      ),
    ).toEqual(["c", "b", "a"]);
  });
});

function folder(id: string, source: string, createdAt: string): Folder {
  return {
    id,
    source,
    created_at: createdAt,
    listed: true,
    enabled: true,
    default_profile_id: "profile",
    actions: [],
    extensions: {},
  };
}

function summary(
  folderId: string,
  latestRunAt: string | null,
): FolderOperationalSummary {
  return {
    folder_id: folderId,
    availability: "available",
    availability_diagnostic: null,
    latest_outcome: latestRunAt ? "succeeded" : null,
    latest_run_at: latestRunAt,
    last_successful_artifact: null,
    artifact_available: false,
    change_state: "unchanged",
  };
}
