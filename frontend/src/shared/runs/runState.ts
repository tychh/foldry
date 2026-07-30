import type { RunState } from "../contracts/generated";

const TERMINAL_RUN_STATES = new Set<RunState>([
  "succeeded",
  "succeeded_with_warnings",
  "failed",
  "stopped",
  "interrupted",
]);

export function isTerminalRunState(state: RunState): boolean {
  return TERMINAL_RUN_STATES.has(state);
}
