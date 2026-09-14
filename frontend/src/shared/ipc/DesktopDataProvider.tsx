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
  ChangeAssessmentProgress,
  ChangeAssessmentResult,
  FolderOperationalSummary,
  IpcError,
  ProgressSnapshot,
  RunEvent,
  RunOutcome,
  RunRecord,
  SchedulerSnapshot,
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
  queuePositionByRun: ReadonlyMap<string, number>;
  changeAssessmentProgress: ChangeAssessmentProgress | null;
  sessionStartedAt: number;
  reload: () => Promise<void>;
  recheckChanged: () => Promise<ChangeAssessmentResult | undefined>;
  cancelRecheckChanged: () => Promise<boolean | undefined>;
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
const CHANGE_RECHECK_COMMANDS = new Set<DesktopCommand>([
  "save_plan",
  "add_folder",
  "update_folder",
  "locate_folder",
  "unlist_folder",
  "forget_folders",
  "forget_all_unlisted_folders",
  "add_action",
  "update_action",
  "remove_action",
  "reorder_actions",
  "save_profile",
  "delete_profile",
  "restore_default_profile",
]);

function shouldRecheckAfterCommand(name: DesktopCommand): boolean {
  return CHANGE_RECHECK_COMMANDS.has(name);
}

export function isCurrentAssessmentGeneration(
  current: string | null,
  candidate: string,
): boolean {
  return current === null || BigInt(candidate) >= BigInt(current);
}

export function DesktopDataProvider({ children }: PropsWithChildren) {
  const [client] = useState<DesktopClient>(createDesktopClient);
  const [sessionStartedAt] = useState(Date.now);
  const [snapshot, setSnapshot] = useState<BootstrapSnapshot | null>(null);
  const [connection, setConnection] = useState<ConnectionState>("loading");
  const [error, setError] = useState<IpcError | null>(null);
  const [progressByRun, setProgressByRun] = useState<
    ReadonlyMap<string, ProgressSnapshot>
  >(() => new Map());
  const [queuePositionByRun, setQueuePositionByRun] = useState<
    ReadonlyMap<string, number>
  >(() => new Map());
  const [changeAssessmentProgress, setChangeAssessmentProgress] =
    useState<ChangeAssessmentProgress | null>(null);
  const runEventRevisionRef = useRef(new Map<string, VersionedRunEvent>());
  const changeAssessmentGenerationRef = useRef(0n);

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
    if (
      event.event.type === "completed" ||
      (event.event.type === "state_changed" &&
        isTerminalRunState(event.event.state))
    ) {
      setProgressByRun((current) => {
        if (!current.has(event.run_id)) return current;
        const next = new Map(current);
        next.delete(event.run_id);
        return next;
      });
    }
    setSnapshot((current) =>
      current ? applyRunEvent(current, event) : current,
    );
  }, []);

  const replaceFolderSummaries = useCallback(
    (summaries: FolderOperationalSummary[]) => {
      setSnapshot((current) =>
        current ? { ...current, folder_summaries: summaries } : current,
      );
    },
    [],
  );

  const recheckChanged = useCallback(async () => {
    try {
      const result =
        await client.command<ChangeAssessmentResult>("recheck_changed");
      if (
        isCurrentAssessmentGeneration(
          changeAssessmentGenerationRef.current.toString(),
          result.generation,
        )
      ) {
        changeAssessmentGenerationRef.current = BigInt(result.generation);
        replaceFolderSummaries(result.summaries);
      }
      setError(null);
      return result;
    } catch (caught) {
      setError(normalizeIpcError(caught));
      return undefined;
    }
  }, [client, replaceFolderSummaries]);

  const cancelRecheckChanged = useCallback(async () => {
    try {
      const cancelled = await client.command<boolean>("cancel_recheck_changed");
      setError(null);
      return cancelled;
    } catch (caught) {
      setError(normalizeIpcError(caught));
      return undefined;
    }
  }, [client]);

  const reload = useCallback(async () => {
    const revisionsAtStart = runEventRevisions(runEventRevisionRef.current);
    setConnection((current) =>
      current === "loading" ? "loading" : "reconnecting",
    );
    try {
      const next = await client.bootstrap();
      setQueuePositionByRun(inferQueuePositions(next.active_runs));
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
          setQueuePositionByRun(inferQueuePositions(next.active_runs));
          setSnapshot(() =>
            reconcileBootstrapAfterRunEvents(
              next,
              changedRunEvents(revisionsAtStart, runEventRevisionRef.current),
            ),
          );
          setError(null);
          setConnection("connected");
          void recheckChanged();
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
          if (event.event.type === "completed") {
            void reload().then(recheckChanged);
          }
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
        void reload().then(recheckChanged);
      }
    };
    document.addEventListener("visibilitychange", handleVisibility);
    return () => {
      active = false;
      dispose?.();
      document.removeEventListener("visibilitychange", handleVisibility);
    };
  }, [applyEvent, client, recheckChanged, reload]);

  useEffect(() => {
    let active = true;
    let dispose: (() => void) | undefined;
    void client
      .listenChangeAssessmentEvents((event) => {
        if (active) {
          if (
            !isCurrentAssessmentGeneration(
              changeAssessmentGenerationRef.current.toString(),
              event.generation,
            )
          ) {
            return;
          }
          changeAssessmentGenerationRef.current = BigInt(event.generation);
          setChangeAssessmentProgress((current) =>
            isCurrentAssessmentGeneration(
              current?.generation ?? null,
              event.generation,
            )
              ? event
              : current,
          );
        }
      })
      .then((unlisten) => {
        if (active) dispose = unlisten;
        else unlisten();
      });
    return () => {
      active = false;
      dispose?.();
    };
  }, [client]);

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
        const scheduler =
          await client.command<SchedulerSnapshot>("scheduler_snapshot");
        const runs = scheduler.runs.map((scheduled) => scheduled.record);
        if (!active) return;
        setQueuePositionByRun(
          new Map(
            scheduler.runs.flatMap((scheduled) =>
              scheduled.queue_position === null
                ? []
                : [[scheduled.record.run_id, Number(scheduled.queue_position)]],
            ),
          ),
        );
        setError(null);
        const activeRunIds = new Set(
          runs
            .filter((run) => !isTerminalRunState(run.state))
            .map((run) => run.run_id),
        );
        setProgressByRun((current) => {
          const next = new Map(
            [...current].filter(([runId]) => activeRunIds.has(runId)),
          );
          return next.size === current.size ? current : next;
        });
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
        if (shouldRecheckAfterCommand(name)) void recheckChanged();
        return result;
      } catch (caught) {
        setError(normalizeIpcError(caught));
        return undefined;
      }
    },
    [client, recheckChanged, reload],
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
      queuePositionByRun,
      changeAssessmentProgress,
      sessionStartedAt,
      reload,
      recheckChanged,
      cancelRecheckChanged,
      query,
      command,
    }),
    [
      client.preview,
      cancelRecheckChanged,
      changeAssessmentProgress,
      command,
      connection,
      error,
      progressByRun,
      queuePositionByRun,
      query,
      reload,
      recheckChanged,
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

function inferQueuePositions(runs: RunRecord[]): ReadonlyMap<string, number> {
  return new Map(
    runs
      .filter((run) => run.state === "queued")
      .map((run, index) => [run.run_id, index + 1]),
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
