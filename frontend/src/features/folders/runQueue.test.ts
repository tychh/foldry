import { describe, expect, it } from "vitest";

import type { RunRecord, RunState } from "../../shared/contracts/generated";
import { runStateSummary } from "./runQueue";

describe("run queue summary", () => {
  it("does not report actions that are stopping as running", () => {
    const runs = [
      "planning",
      "running",
      "stopping",
      "queued",
      "paused",
      "stopped",
    ].map((state, index) => run(`run-${index}`, state as RunState));

    expect(runStateSummary(runs)).toEqual({
      running: 2,
      queued: 1,
      paused: 1,
    });
  });
});

function run(runId: string, state: RunState): RunRecord {
  return {
    run_id: runId,
    state,
  } as RunRecord;
}
