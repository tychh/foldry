import type { RunRecord, RunState } from "../../shared/contracts/generated";

import { isTerminalRunState } from "../../shared/runs/runState";

export const FOLDER_RESULT_VISIBLE_MS = 30_000;

type FolderDisplayState = RunState | "ready";

const ACTIVE_STATE_PRIORITY: RunState[] = [
  "running",
  "planning",
  "stopping",
  "paused",
  "queued",
];

export function resolveFolderStatus(
  activeRuns: RunRecord[],
  latestRun: RunRecord | undefined,
  sessionStartedAt: number,
  now: number,
): FolderDisplayState {
  for (const state of ACTIVE_STATE_PRIORITY) {
    if (activeRuns.some((run) => run.state === state)) {
      return state;
    }
  }

  const expiresAt = folderResultExpiresAt(latestRun, sessionStartedAt);
  return latestRun && expiresAt !== null && now < expiresAt
    ? latestRun.state
    : "ready";
}

export function folderResultExpiresAt(
  latestRun: RunRecord | undefined,
  sessionStartedAt: number,
): number | null {
  if (
    !latestRun ||
    !isTerminalRunState(latestRun.state) ||
    !latestRun.finished_at
  ) {
    return null;
  }
  const finishedAt = Date.parse(latestRun.finished_at);
  if (!Number.isFinite(finishedAt) || finishedAt < sessionStartedAt) {
    return null;
  }
  return finishedAt + FOLDER_RESULT_VISIBLE_MS;
}
