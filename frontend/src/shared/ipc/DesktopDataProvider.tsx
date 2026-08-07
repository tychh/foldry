/* eslint-disable react-refresh/only-export-components */

import {
  createContext,
  type PropsWithChildren,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";

import type {
  BootstrapSnapshot,
  IpcError,
  ProgressSnapshot,
  RunEvent,
  RunOutcome,
  RunRecord,
  RunState,
} from "../contracts/generated";
import {
  createDesktopClient,
  type DesktopClient,
  type DesktopCommand,
  type DesktopCommandArgs,
  normalizeIpcError,
} from "./client";
import { isTerminalRunState } from "../runs/runState";

export type ConnectionState =
  "loading" | "connected" | "reconnecting" | "error";

type DesktopDataContextValue = {
  snapshot: BootstrapSnapshot | null;
  connection: ConnectionState;
  error: IpcError | null;
  preview: boolean;
  progressByRun: ReadonlyMap<string, ProgressSnapshot>;
  sessionStartedAt: number;
  reload: () => Promise<void>;
  query: <T>(name: DesktopCommand, args?: DesktopCommandArgs) => Promise<T>;
  command: <T>(
    name: DesktopCommand,
    args?: DesktopCommandArgs,
  ) => Promise<T | undefined>;
};

const DesktopDataContext = createContext<DesktopDataContextValue | null>(null);

type VersionedRunEvent = {
  revision: number;
  event: RunEvent;
};

const RUN_RECONCILIATION_INTERVAL_MS = 1_000;

export function DesktopDataProvider({ children }: PropsWithChildren) {
  const [client] = useState<DesktopClient>(createDesktopClient);
  const [sessionStartedAt] = useState(Date.now);
  const [snapshot, setSnapshot] = useState<BootstrapSnapshot | null>(null);
  const [connection, setConnection] = useState<ConnectionState>("loading");
  const [error, setError] = useState<IpcError | null>(null);
  const [progressByRun, setProgressByRun] = useState<
    ReadonlyMap<string, ProgressSnapshot>
  >(() => new Map());
  const runEventRevisionRef = useRef(new Map<string, VersionedRunEvent>());

  const applyEvent = useCallback((event: RunEvent) => {
    if (
      event.event.type === "state_changed" ||
      event.event.type === "completed"
    ) {
      const revisions = runEventRevisionRef.current;
      const previous = revisions.get(event.run_id);
      if (previous && previous.event.sequence >= event.sequence) {
        return;
      }
      revisions.set(event.run_id, {
        revision: (previous?.revision ?? 0) + 1,
        event,
      });
    }
    if (event.event.type === "progress") {
      const progress = event.event.progress;
      setProgressByRun((current) => {
        const next = new Map(current);
        next.set(event.run_id, progress);
        return next;
      });
    }
    setSnapshot((current) =>
      current ? applyRunEvent(current, event) : current,
    );
  }, []);

  const reload = useCallback(async () => {
    const revisionsAtStart = runEventRevisions(runEventRevisionRef.current);
    setConnection((current) =>
      current === "loading" ? "loading" : "reconnecting",
    );
    try {
      const next = await client.bootstrap();
      setSnapshot(() =>
        reconcileBootstrapAfterRunEvents(
          next,
          changedRunEvents(revisionsAtStart, runEventRevisionRef.current),
        ),
      );
      setError(null);
      setConnection("connected");
    } catch (caught) {
      setError(normalizeIpcError(caught));
      setConnection("error");
    }
  }, [client]);

  useEffect(() => {
    let active = true;
    let dispose: (() => void) | undefined;
    const revisionsAtStart = runEventRevisions(runEventRevisionRef.current);

    void client.bootstrap().then(
      (next) => {
        if (active) {
          setSnapshot(() =>
            reconcileBootstrapAfterRunEvents(
              next,
              changedRunEvents(revisionsAtStart, runEventRevisionRef.current),
            ),
          );
          setError(null);
          setConnection("connected");
        }
      },
      (caught: unknown) => {
        if (active) {
          setError(normalizeIpcError(caught));
          setConnection("error");
        }
      },
    );
    void client
      .listenRunEvents((event) => {
        if (active) {
          applyEvent(event);
        }
      })
      .then((unlisten) => {
        if (active) {
          dispose = unlisten;
        } else {
          unlisten();
        }
      });

    const handleVisibility = () => {
      if (document.visibilityState === "visible" && client.preview === false) {
        void reload();
      }
    };
    document.addEventListener("visibilitychange", handleVisibility);
    return () => {
      active = false;
      dispose?.();
      document.removeEventListener("visibilitychange", handleVisibility);
    };
  }, [applyEvent, client, reload]);

  const hasNonTerminalRuns =
    snapshot?.active_runs.some((run) => !isTerminalRunState(run.state)) ??
    false;

  useEffect(() => {
    if (!hasNonTerminalRuns || client.preview) return;
    let active = true;
    let refreshing = false;

    const refreshRuns = async () => {
      if (refreshing) return;
      refreshing = true;
      const revisionsAtStart = runEventRevisions(runEventRevisionRef.current);
      try {
        const runs = await client.command<RunRecord[]>("scheduler_snapshot");
        if (!active) return;
        setError(null);
        setSnapshot((current) =>
          current
            ? reconcileSchedulerSnapshot(
                current,
                runs,
                changedRunEvents(revisionsAtStart, runEventRevisionRef.current),
              )
            : current,
        );
      } catch (caught) {
        if (active) {
          setError(normalizeIpcError(caught));
        }
      } finally {
        refreshing = false;
      }
    };

    const interval = window.setInterval(
      () => void refreshRuns(),
      RUN_RECONCILIATION_INTERVAL_MS,
    );
    return () => {
      active = false;
      window.clearInterval(interval);
    };
  }, [client, hasNonTerminalRuns]);

  const command = useCallback(
    async <T,>(
      name: DesktopCommand,
      args?: DesktopCommandArgs,
    ): Promise<T | undefined> => {
      try {
        const result = await client.command<T>(name, args);
        setError(null);
        await reload();
        return result;
      } catch (caught) {
        setError(normalizeIpcError(caught));
        return undefined;
      }
    },
    [client, reload],
  );

  const query = useCallback(
    async <T,>(name: DesktopCommand, args?: DesktopCommandArgs): Promise<T> => {
      try {
        const result = await client.command<T>(name, args);
        setError(null);
        return result;
      } catch (caught) {
        const normalized = normalizeIpcError(caught);
        setError(normalized);
        throw normalized;
      }
    },
    [client],
  );

  const value = useMemo<DesktopDataContextValue>(
    () => ({
      snapshot,
      connection,
      error,
      preview: client.preview,
      progressByRun,
      sessionStartedAt,
      reload,
      query,
      command,
    }),
    [
      client.preview,
      command,
      connection,
      error,
      progressByRun,
      query,
      reload,
      sessionStartedAt,
      snapshot,
    ],
  );

  return (
    <DesktopDataContext.Provider value={value}>
      {children}
    </DesktopDataContext.Provider>
  );
}

export function useDesktopData(): DesktopDataContextValue {
  const context = useContext(DesktopDataContext);
  if (!context) {
    throw new Error("useDesktopData must be used inside DesktopDataProvider");
  }
  return context;
}

function updateRunRecords(records: RunRecord[], event: RunEvent): RunRecord[] {
  const index = records.findIndex((run) => run.run_id === event.run_id);
  if (index < 0) {
    return records;
  }
  const current = records[index];
  if (!current) {
    return records;
  }
  let next = current;
  if (event.event.type === "state_changed") {
    if (
      isTerminalRunState(current.state) &&
      !isTerminalRunState(event.event.state)
    ) {
      return records;
    }
    next = {
      ...current,
      state: event.event.state,
      finished_at: isTerminalRunState(event.event.state)
        ? event.occurred_at
        : current.finished_at,
    };
  } else if (event.event.type === "completed") {
    next = {
      ...current,
      state: outcomeState(event.event.summary.outcome),
      summary: event.event.summary,
      finished_at: event.occurred_at,
    };
  }
  const updated = [...records];
  updated[index] = next;
  return updated;
}

export function applyRunEvent(
  snapshot: BootstrapSnapshot,
  event: RunEvent,
): BootstrapSnapshot {
  return {
    ...snapshot,
    active_runs: updateRunRecords(snapshot.active_runs, event),
    recent_runs: updateRunRecords(snapshot.recent_runs, event),
  };
}

export function reconcileBootstrapAfterRunEvents(
  next: BootstrapSnapshot,
  changedEvents: ReadonlyMap<string, RunEvent>,
): BootstrapSnapshot {
  let reconciled = next;
  for (const event of changedEvents.values()) {
    reconciled = applyRunEvent(reconciled, event);
  }
  return reconciled;
}

export function reconcileSchedulerSnapshot(
  current: BootstrapSnapshot,
  runs: RunRecord[],
  changedEvents: ReadonlyMap<string, RunEvent>,
): BootstrapSnapshot {
  const scheduledById = new Map(runs.map((run) => [run.run_id, run]));
  return reconcileBootstrapAfterRunEvents(
    {
      ...current,
      active_runs: runs,
      recent_runs: current.recent_runs.map(
        (run) => scheduledById.get(run.run_id) ?? run,
      ),
    },
    changedEvents,
  );
}

function runEventRevisions(
  events: ReadonlyMap<string, VersionedRunEvent>,
): Map<string, number> {
  return new Map([...events].map(([runId, event]) => [runId, event.revision]));
}

function changedRunEvents(
  before: ReadonlyMap<string, number>,
  after: ReadonlyMap<string, VersionedRunEvent>,
): Map<string, RunEvent> {
  return new Map(
    [...after].flatMap(([runId, versioned]) =>
      versioned.revision === before.get(runId)
        ? []
        : [[runId, versioned.event] as const],
    ),
  );
}

function outcomeState(outcome: RunOutcome): RunState {
  return outcome;
}
