import { describe, expect, it } from "vitest";

import { defaultArchiveStep, updateArchive } from "./taskModel";

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
  extensions: {},
};

describe("task model helpers", () => {
  it("creates an archive step from defaults with a source-derived filename", () => {
    const step = defaultArchiveStep("/home/alice/My Work", settings);

    expect(step.archive?.output).toMatchObject({
      directory: "/backups",
      filename: "my-work-{date}",
      format: "tar_zst",
      compression: "maximum",
    });
  });

  it("updates one archive without mutating the task snapshot", () => {
    const step = defaultArchiveStep("/source", settings);
    const task = {
      id: "task",
      source: "/source",
      enabled: true,
      profile_id: "profile",
      steps: [step],
      extensions: {},
    };
    const updated = updateArchive(task, (archive) => ({
      ...archive,
      include_root: false,
    }));

    expect(task.steps[0]?.archive?.include_root).toBe(true);
    expect(updated.steps[0]?.archive?.include_root).toBe(false);
  });
});
