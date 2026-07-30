import type { RunRecord } from "../../shared/contracts/generated";

export type RunStateSummary = {
  running: number;
  queued: number;
  paused: number;
};

export function runStateSummary(runs: RunRecord[]): RunStateSummary {
  return {
    running: runs.filter(
      (run) => run.state === "planning" || run.state === "running",
    ).length,
    queued: runs.filter((run) => run.state === "queued").length,
    paused: runs.filter((run) => run.state === "paused").length,
  };
}
