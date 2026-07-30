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

export function DesktopDataProvider({ children }: PropsWithChildren) {
  const [client] = useState<DesktopClient>(createDesktopClient);
  const [sessionStartedAt] = useState(Date.now);
  const [snapshot, setSnapshot] = useState<BootstrapSnapshot | null>(null);
  const [connection, setConnection] = useState<ConnectionState>("loading");
  const [error, setError] = useState<IpcError | null>(null);
  const [progressByRun, setProgressByRun] = useState<
    ReadonlyMap<string, ProgressSnapshot>
  >(() => new Map());
  const runEventRevisionRef = useRef(new Map<string, number>());

  const applyEvent = useCallback((event: RunEvent) => {
    if (
      event.event.type === "state_changed" ||
      event.event.type === "completed"
    ) {
      const revisions = runEventRevisionRef.current;
      revisions.set(event.run_id, (revisions.get(event.run_id) ?? 0) + 1);
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
    const revisionsAtStart = new Map(runEventRevisionRef.current);
    setConnection((current) =>
      current === "loading" ? "loading" : "reconnecting",
    );
    try {
      const next = await client.bootstrap();
      setSnapshot((current) =>
        current
          ? reconcileBootstrapAfterRunEvents(
              next,
              current,
              changedRunIds(revisionsAtStart, runEventRevisionRef.current),
            )
          : next,
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

    void client.bootstrap().then(
      (next) => {
        if (active) {
          setSnapshot(next);
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
  current: BootstrapSnapshot,
  changedRunIds: ReadonlySet<string>,
): BootstrapSnapshot {
  if (changedRunIds.size === 0) {
    return next;
  }
  return {
    ...next,
    active_runs: preserveChangedRuns(
      next.active_runs,
      current.active_runs,
      changedRunIds,
    ),
    recent_runs: preserveChangedRuns(
      next.recent_runs,
      current.recent_runs,
      changedRunIds,
    ),
  };
}

function preserveChangedRuns(
  next: RunRecord[],
  current: RunRecord[],
  changedRunIds: ReadonlySet<string>,
): RunRecord[] {
  const currentById = new Map(current.map((run) => [run.run_id, run]));
  return next.map((run) =>
    changedRunIds.has(run.run_id) ? (currentById.get(run.run_id) ?? run) : run,
  );
}

function changedRunIds(
  before: ReadonlyMap<string, number>,
  after: ReadonlyMap<string, number>,
): Set<string> {
  return new Set(
    [...after].flatMap(([runId, revision]) =>
      revision === before.get(runId) ? [] : [runId],
    ),
  );
}

function outcomeState(outcome: RunOutcome): RunState {
  return outcome;
}
