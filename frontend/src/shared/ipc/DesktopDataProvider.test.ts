import { describe, expect, it } from "vitest";

import type {
  BootstrapSnapshot,
  RunEvent,
  RunRecord,
} from "../contracts/generated";
import {
  applyRunEvent,
  reconcileBootstrapAfterRunEvents,
  reconcileSchedulerSnapshot,
} from "./DesktopDataProvider";

describe("DesktopDataProvider run events", () => {
  it("updates duplicate active and recent records when a run finishes", () => {
    const running = {
      run_id: "run-1",
      folder_id: "folder-1",
      action_id: "action-1",
      state: "running",
      started_at: "2026-07-30T10:00:00Z",
      finished_at: null,
      summary: null,
    } as RunRecord;
    const snapshot = {
      active_runs: [running],
      recent_runs: [{ ...running }],
    } as BootstrapSnapshot;
    const event = {
      run_id: running.run_id,
      occurred_at: "2026-07-30T10:01:00Z",
      event: {
        type: "state_changed",
        state: "succeeded",
      },
    } as RunEvent;

    const updated = applyRunEvent(snapshot, event);

    expect(updated.active_runs[0]?.state).toBe("succeeded");
    expect(updated.recent_runs[0]?.state).toBe("succeeded");
    expect(updated.active_runs[0]?.finished_at).toBe(event.occurred_at);
    expect(updated.recent_runs[0]?.finished_at).toBe(event.occurred_at);
  });

  it("does not regress a terminal run when a delayed event arrives", () => {
    const stopped = run("stopped", "2026-07-30T10:01:00Z");
    const delayed = {
      run_id: stopped.run_id,
      occurred_at: "2026-07-30T10:00:30Z",
      event: {
        type: "state_changed",
        state: "stopping",
      },
    } as RunEvent;
    const snapshot = {
      active_runs: [stopped],
      recent_runs: [],
    } as unknown as BootstrapSnapshot;

    expect(applyRunEvent(snapshot, delayed).active_runs[0]?.state).toBe(
      "stopped",
    );
  });

  it("preserves a completion event received during a stale bootstrap reload", () => {
    const stopping = run("stopping", null);
    const stopped = run("stopped", "2026-07-30T10:01:00Z");
    const stale = {
      active_runs: [stopping],
      recent_runs: [{ ...stopping }],
    } as unknown as BootstrapSnapshot;
    const reconciled = reconcileBootstrapAfterRunEvents(
      stale,
      new Map([
        [
          stopped.run_id,
          {
            run_id: stopped.run_id,
            occurred_at: stopped.finished_at,
            sequence: 4,
            event: { type: "state_changed", state: "stopped" },
          } as unknown as RunEvent,
        ],
      ]),
    );

    expect(reconciled.active_runs[0]?.state).toBe("stopped");
    expect(reconciled.recent_runs[0]?.state).toBe("stopped");
    expect(reconciled.active_runs[0]?.finished_at).toBe(stopped.finished_at);
  });

  it("replays a completion received before a new run enters the snapshot", () => {
    const running = run("running", null);
    const completed = {
      run_id: running.run_id,
      occurred_at: "2026-07-30T10:01:00Z",
      sequence: 4,
      event: {
        type: "completed",
        summary: {
          outcome: "succeeded",
        },
      },
    } as unknown as RunEvent;
    const stale = {
      active_runs: [running],
      recent_runs: [{ ...running }],
    } as unknown as BootstrapSnapshot;

    const reconciled = reconcileBootstrapAfterRunEvents(
      stale,
      new Map([[running.run_id, completed]]),
    );

    expect(reconciled.active_runs[0]?.state).toBe("succeeded");
    expect(reconciled.recent_runs[0]?.state).toBe("succeeded");
    expect(reconciled.active_runs[0]?.summary?.outcome).toBe("succeeded");
  });

  it("repairs a missed terminal event from the scheduler snapshot", () => {
    const stale = run("running", null);
    const succeeded = run("succeeded", "2026-07-30T10:01:00Z");
    const current = {
      active_runs: [stale],
      recent_runs: [{ ...stale }],
    } as unknown as BootstrapSnapshot;

    const reconciled = reconcileSchedulerSnapshot(
      current,
      [succeeded],
      new Map(),
    );

    expect(reconciled.active_runs[0]?.state).toBe("succeeded");
    expect(reconciled.recent_runs[0]?.state).toBe("succeeded");
  });
});

function run(state: RunRecord["state"], finishedAt: string | null): RunRecord {
  return {
    run_id: "run-1",
    folder_id: "folder-1",
    action_id: "action-1",
    state,
    started_at: "2026-07-30T10:00:00Z",
    finished_at: finishedAt,
    summary: null,
  } as RunRecord;
}
