import { describe, expect, it } from "vitest";

import type { RunRecord, RunState } from "../../shared/contracts/generated";
import { FOLDER_RESULT_VISIBLE_MS, resolveFolderStatus } from "./folderStatus";

function run(state: RunState, finishedAt: string | null = null): RunRecord {
  return {
    run_id: `${state}-run`,
    folder_id: "folder",
    action_id: "action",
    state,
    started_at: "2026-07-30T10:00:00.000Z",
    finished_at: finishedAt,
    snapshot: {},
    summary: null,
  } as RunRecord;
}

describe("folder status", () => {
  const finishedAt = Date.parse("2026-07-30T10:01:00.000Z");
  const completed = run("succeeded", new Date(finishedAt).toISOString());

  it("shows a fresh result for 30 seconds, then returns to ready", () => {
    expect(
      resolveFolderStatus([], completed, finishedAt - 1_000, finishedAt),
    ).toBe("succeeded");
    expect(
      resolveFolderStatus(
        [],
        completed,
        finishedAt - 1_000,
        finishedAt + FOLDER_RESULT_VISIBLE_MS,
      ),
    ).toBe("ready");
  });

  it("does not restore terminal history from before this app session", () => {
    expect(
      resolveFolderStatus(
        [],
        completed,
        finishedAt + 1_000,
        finishedAt + 1_000,
      ),
    ).toBe("ready");
  });

  it("keeps a current action state above the recent folder result", () => {
    expect(
      resolveFolderStatus(
        [run("running")],
        completed,
        finishedAt - 1_000,
        finishedAt,
      ),
    ).toBe("running");
  });
});
