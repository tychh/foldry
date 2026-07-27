import {
  ActionIcon,
  Alert,
  Box,
  Button,
  Drawer,
  Group,
  Modal,
  NumberInput,
  Paper,
  Progress,
  ScrollArea,
  Select,
  Stack,
  Text,
  TextInput,
  Title,
  Tooltip,
} from "@mantine/core";
import { useDisclosure, useMediaQuery } from "@mantine/hooks";
import {
  CaretRight,
  Folder,
  FolderOpen,
  ListMagnifyingGlass,
  Pause,
  Play,
  Plus,
  SelectionSlash,
  SlidersHorizontal,
  Stop,
} from "@phosphor-icons/react";
import {
  lazy,
  Suspense,
  useCallback,
  useEffect,
  useMemo,
  useState,
} from "react";

import type {
  BootstrapSnapshot,
  ProgressSnapshot,
  RunRecord,
  Settings,
  Task,
  TaskAddResult,
} from "../../shared/contracts/generated";
import { type MessageKey, useI18n } from "../../shared/i18n/I18nProvider";
import { useDesktopData } from "../../shared/ipc/DesktopDataProvider";
import { RunStatus } from "../../shared/ui/RunStatus";
import { FolderTree } from "./FolderTree";
import { TaskInspector } from "./TaskInspector";
import { basename, defaultArchiveStep } from "./taskModel";
import classes from "./TasksWorkspace.module.css";

type TasksWorkspaceProps = {
  snapshot: BootstrapSnapshot;
};

const RunExplorer = lazy(() =>
  import("./RunExplorer").then((module) => ({ default: module.RunExplorer })),
);

export function TasksWorkspace({ snapshot }: TasksWorkspaceProps) {
  const { t } = useI18n();
  const { command, preview, progressByRun, query } = useDesktopData();
  const compact = useMediaQuery("(max-width: 74.99em)");
  const [selectedTaskId, setSelectedTaskId] = useState<string | null>(
    snapshot.plan.tasks[0]?.id ?? null,
  );
  const [duplicateTaskId, setDuplicateTaskId] = useState<string | null>(null);
  const [dragging, setDragging] = useState(false);
  const [foldersOpened, folders] = useDisclosure(false);
  const [settingsOpened, settingsDrawer] = useDisclosure(false);
  const [defaultsOpened, defaultsModal] = useDisclosure(false);
  const [explorer, setExplorer] = useState<{
    taskId: string;
    tab: "preview" | "history";
  } | null>(null);
  const selectedTask =
    snapshot.plan.tasks.find((task) => task.id === selectedTaskId) ?? null;
  const runsByTask = useMemo(
    () => latestRuns([...snapshot.active_runs, ...snapshot.recent_runs]),
    [snapshot.active_runs, snapshot.recent_runs],
  );
  const queuePositions = useMemo(
    () =>
      new Map(
        snapshot.active_runs
          .filter((run) => run.state === "queued")
          .map((run, index) => [run.run_id, index + 1] as const),
      ),
    [snapshot.active_runs],
  );
  const queueCounts = runStateSummary(snapshot.active_runs);
  const percentages = snapshot.active_runs
    .filter((run) => !isTerminal(run.state) && run.state !== "queued")
    .map((run) => runProgress(run, progressByRun.get(run.run_id)));
  const overall =
    percentages.length === 0 || percentages.some((value) => value === null)
      ? null
      : Math.round(
          percentages.reduce<number>(
            (total, value) => total + (value ?? 0),
            0,
          ) / percentages.length,
        );
  const defaultProfileId =
    snapshot.profiles.find(
      (profile) =>
        profile.id === snapshot.settings.default_profile_id && profile.valid,
    )?.id ??
    snapshot.profiles.find((profile) => profile.id && profile.valid)?.id;

  const addPaths = useCallback(
    async (paths: string[]) => {
      if (!defaultProfileId) {
        return;
      }
      for (const path of paths) {
        const result = await command<TaskAddResult[]>("add_dropped_sources", {
          paths: [path],
          profileId: defaultProfileId,
          steps: [defaultArchiveStep(path, snapshot.settings)],
        });
        const added = result?.[0];
        if (added) {
          setSelectedTaskId(added.task.id);
          setDuplicateTaskId(added.created ? null : added.task.id);
        }
      }
      setDragging(false);
    },
    [command, defaultProfileId, snapshot.settings],
  );

  const chooseFolders = useCallback(async () => {
    const paths = await query<string[]>("pick_folders");
    await addPaths(paths);
  }, [addPaths, query]);

  useEffect(() => {
    if (preview) {
      return;
    }
    let dispose: (() => void) | undefined;
    let active = true;
    void import("@tauri-apps/api/webview")
      .then(({ getCurrentWebview }) =>
        getCurrentWebview().onDragDropEvent((event) => {
          if (!active) {
            return;
          }
          if (event.payload.type === "enter" || event.payload.type === "over") {
            setDragging(true);
          } else if (event.payload.type === "leave") {
            setDragging(false);
          } else if (event.payload.type === "drop") {
            void addPaths(event.payload.paths);
          }
        }),
      )
      .then((unlisten) => {
        if (active) {
          dispose = unlisten;
        } else {
          unlisten();
        }
      });
    return () => {
      active = false;
      dispose?.();
    };
  }, [addPaths, preview]);

  const runTask = (task: Task) => {
    void command<RunRecord>("run_task", { taskId: task.id });
  };

  const updateTask = async (task: Task) =>
    Boolean(await command<Task>("update_task", { task }));

  const removeTask = async (task: Task) => {
    const removed = Boolean(
      await command<boolean>("remove_task", { taskId: task.id }),
    );
    if (removed) {
      setSelectedTaskId(null);
    }
    return removed;
  };

  return (
    <Box className={classes.shell}>
      <Box className={classes.workspace} data-dragging={dragging || undefined}>
        {compact ? (
          <button
            aria-label={t("openFolders")}
            className={classes.collapsedRail}
            onClick={folders.open}
            type="button"
          >
            <Folder aria-hidden size={23} />
            <span>{t("folders")}</span>
            <CaretRight aria-hidden size={16} />
          </button>
        ) : (
          <FoldersPanel
            roots={snapshot.roots}
            onAdd={(path) => void addPaths([path])}
            onChoose={() => void chooseFolders()}
          />
        )}

        <TaskList
          dragging={dragging}
          duplicateTaskId={duplicateTaskId}
          profiles={snapshot.profiles}
          progressByRun={progressByRun}
          queuePositions={queuePositions}
          runsByTask={runsByTask}
          selectedTaskId={selectedTaskId}
          snapshot={snapshot}
          onChoose={() => void chooseFolders()}
          onPause={(run) => void command("pause_run", { runId: run.run_id })}
          onPreview={(taskId) => setExplorer({ taskId, tab: "preview" })}
          onResume={(run) => void command("resume_run", { runId: run.run_id })}
          onRun={runTask}
          onSelect={(taskId) => {
            setSelectedTaskId(taskId);
            setDuplicateTaskId(null);
            if (compact) {
              settingsDrawer.open();
            }
          }}
          onHistory={(taskId) => setExplorer({ taskId, tab: "history" })}
          onStop={(run) => void command("stop_run", { runId: run.run_id })}
        />

        {compact ? (
          <button
            aria-label={t("openSettings")}
            className={classes.collapsedRail}
            onClick={settingsDrawer.open}
            type="button"
          >
            <SlidersHorizontal aria-hidden size={23} />
            <span>{t("taskSettings")}</span>
            <CaretRight aria-hidden size={16} />
          </button>
        ) : (
          <TaskInspector
            key={selectedTask?.id ?? "no-task"}
            profiles={snapshot.profiles}
            task={selectedTask}
            onRemove={removeTask}
            onRun={runTask}
            onUpdate={updateTask}
          />
        )}
        {dragging ? (
          <Box aria-live="polite" className={classes.dropOverlay}>
            <FolderOpen aria-hidden size={38} />
            <Text fw={650}>{t("dropFolders")}</Text>
          </Box>
        ) : null}
      </Box>

      <BottomCommandBar
        hasPaused={snapshot.active_runs.some((run) => run.state === "paused")}
        overall={overall}
        queueCounts={queueCounts}
        settings={snapshot.settings}
        onClear={() => setSelectedTaskId(null)}
        onEditDefaults={defaultsModal.open}
        onPauseAll={() =>
          void command(
            snapshot.active_runs.some((run) => run.state === "paused")
              ? "resume_all"
              : "pause_all",
          )
        }
        onRunAll={() => void command("run_all_enabled")}
        onStopAll={() => void command("stop_all")}
      />

      <Drawer
        opened={foldersOpened}
        onClose={folders.close}
        position="left"
        size="min(88vw, 340px)"
        title={t("folders")}
      >
        <FoldersPanel
          roots={snapshot.roots}
          unframed
          onAdd={(path) => void addPaths([path])}
          onChoose={() => void chooseFolders()}
        />
      </Drawer>
      <Drawer
        opened={settingsOpened}
        onClose={settingsDrawer.close}
        position="right"
        size="min(94vw, 390px)"
        title={t("taskSettings")}
      >
        <TaskInspector
          key={selectedTask?.id ?? "no-task"}
          profiles={snapshot.profiles}
          task={selectedTask}
          unframed
          onRemove={removeTask}
          onRun={runTask}
          onUpdate={updateTask}
        />
      </Drawer>
      <DefaultsModal
        opened={defaultsOpened}
        settings={snapshot.settings}
        onClose={defaultsModal.close}
        onSave={async (next) => {
          const saved = await command<Settings>("save_settings", {
            settings: next,
          });
          if (saved) {
            defaultsModal.close();
          }
        }}
      />
      {explorer ? (
        <Suspense fallback={null}>
          <RunExplorer
            key={`${explorer.taskId}-${explorer.tab}`}
            initialTab={explorer.tab}
            opened
            task={
              snapshot.plan.tasks.find((task) => task.id === explorer.taskId) ??
              null
            }
            onClose={() => setExplorer(null)}
          />
        </Suspense>
      ) : null}
    </Box>
  );
}

function FoldersPanel({
  roots,
  onAdd,
  onChoose,
  unframed = false,
}: {
  roots: BootstrapSnapshot["roots"];
  onAdd: (path: string) => void;
  onChoose: () => void;
  unframed?: boolean;
}) {
  const { t } = useI18n();
  return (
    <aside className={unframed ? classes.unframedPanel : classes.foldersPanel}>
      <Group justify="space-between" wrap="nowrap">
        <Title order={2}>{t("folders")}</Title>
        <Tooltip label={t("chooseFolders")}>
          <ActionIcon
            aria-label={t("chooseFolders")}
            size="lg"
            variant="default"
            onClick={onChoose}
          >
            <Plus aria-hidden size={18} />
          </ActionIcon>
        </Tooltip>
      </Group>
      <Button
        fullWidth
        justify="flex-start"
        leftSection={<FolderOpen aria-hidden size={18} />}
        variant="default"
        onClick={onChoose}
      >
        {t("chooseFolders")}
      </Button>
      <ScrollArea className={classes.tree}>
        {roots.length ? (
          <FolderTree roots={roots} onAdd={onAdd} />
        ) : (
          <Text c="dimmed" p="sm" size="sm">
            {t("browserUnavailable")}
          </Text>
        )}
      </ScrollArea>
    </aside>
  );
}

function TaskList({
  snapshot,
  profiles,
  selectedTaskId,
  runsByTask,
  queuePositions,
  progressByRun,
  dragging,
  duplicateTaskId,
  onChoose,
  onSelect,
  onRun,
  onPause,
  onPreview,
  onResume,
  onStop,
  onHistory,
}: {
  snapshot: BootstrapSnapshot;
  profiles: BootstrapSnapshot["profiles"];
  selectedTaskId: string | null;
  runsByTask: ReadonlyMap<string, RunRecord>;
  queuePositions: ReadonlyMap<string, number>;
  progressByRun: ReadonlyMap<string, ProgressSnapshot>;
  dragging: boolean;
  duplicateTaskId: string | null;
  onChoose: () => void;
  onSelect: (taskId: string) => void;
  onRun: (task: Task) => void;
  onPause: (run: RunRecord) => void;
  onPreview: (taskId: string) => void;
  onResume: (run: RunRecord) => void;
  onStop: (run: RunRecord) => void;
  onHistory: (taskId: string) => void;
}) {
  const { t } = useI18n();
  const profileNames = new Map(
    profiles
      .filter((profile) => profile.id)
      .map((profile) => [profile.id!, profile.name]),
  );
  return (
    <main className={classes.taskPanel}>
      <Group justify="space-between">
        <Title order={1}>{t("configuredTasks")}</Title>
        <ActionIcon
          aria-label={t("chooseFolders")}
          size="lg"
          variant="subtle"
          onClick={onChoose}
        >
          <Plus aria-hidden size={20} />
        </ActionIcon>
      </Group>
      {duplicateTaskId ? (
        <Alert color="blue">{t("duplicateTaskFocused")}</Alert>
      ) : null}
      {snapshot.plan.tasks.length === 0 ? (
        <Paper className={classes.emptyState} withBorder>
          <FolderOpen aria-hidden size={34} />
          <Title order={2}>{t("noTasks")}</Title>
          <Text c="dimmed" maw={380} ta="center">
            {dragging ? t("dropFolders") : t("noTasksHint")}
          </Text>
          <Button
            leftSection={<Plus aria-hidden size={18} />}
            onClick={onChoose}
          >
            {t("chooseFolders")}
          </Button>
        </Paper>
      ) : (
        <ScrollArea className={classes.taskScroll}>
          <Stack gap="md" pr="xs">
            {snapshot.plan.tasks.map((task) => {
              const run = runsByTask.get(task.id);
              return (
                <TaskCard
                  key={task.id}
                  profileName={profileNames.get(task.profile_id) ?? "—"}
                  progress={run ? progressByRun.get(run.run_id) : undefined}
                  queuePosition={
                    run ? queuePositions.get(run.run_id) : undefined
                  }
                  run={run}
                  selected={selectedTaskId === task.id}
                  task={task}
                  onPause={() => run && onPause(run)}
                  onPreview={() => onPreview(task.id)}
                  onResume={() => run && onResume(run)}
                  onRun={() => onRun(task)}
                  onSelect={() => onSelect(task.id)}
                  onStop={() => run && onStop(run)}
                  onHistory={() => onHistory(task.id)}
                />
              );
            })}
          </Stack>
        </ScrollArea>
      )}
    </main>
  );
}

function TaskCard({
  task,
  run,
  progress,
  profileName,
  queuePosition,
  selected,
  onSelect,
  onRun,
  onPause,
  onPreview,
  onResume,
  onStop,
  onHistory,
}: {
  task: Task;
  run?: RunRecord;
  progress?: ProgressSnapshot;
  profileName: string;
  queuePosition?: number;
  selected: boolean;
  onSelect: () => void;
  onRun: () => void;
  onPause: () => void;
  onPreview: () => void;
  onResume: () => void;
  onStop: () => void;
  onHistory: () => void;
}) {
  const { t } = useI18n();
  const archive = task.steps[0]?.archive;
  const state = run?.state ?? "ready";
  const percentage = run ? runProgress(run, progress) : null;
  const active = run && !isTerminal(run.state);
  return (
    <Paper
      aria-current={selected ? "true" : undefined}
      className={classes.taskCard}
      component="article"
      data-selected={selected || undefined}
      withBorder
    >
      <button
        aria-label={`${basename(task.source)} · ${task.source}`}
        className={classes.taskSelect}
        type="button"
        onClick={onSelect}
      >
        <Box className={classes.taskIdentity}>
          <Box aria-hidden className={classes.archiveIcon}>
            <FolderOpen size={27} weight="duotone" />
          </Box>
          <Box miw={0}>
            <Text fw={650} size="md">
              {basename(task.source)}
            </Text>
            <Text className={classes.path} c="dimmed" mt={3} size="xs">
              {task.source}
            </Text>
          </Box>
        </Box>
        <Box className={classes.taskMeta}>
          <Meta label={t("profile")} value={profileName} />
          <Meta
            label={t("format")}
            value={archive ? t(formatMessage(archive.output.format)) : "—"}
          />
          <Meta
            label={t("compression")}
            value={
              archive ? t(compressionMessage(archive.output.compression)) : "—"
            }
          />
        </Box>
      </button>
      <button
        aria-label={t("openRunHistory")}
        className={classes.taskProgress}
        type="button"
        onClick={onHistory}
      >
        <Group justify="space-between" wrap="nowrap">
          <RunStatus state={state} />
          <Text c="dimmed" fw={600} size="xs">
            {queuePosition
              ? t("queuePosition", { position: queuePosition })
              : percentage !== null && percentage > 0
                ? `${percentage}%`
                : "—"}
          </Text>
        </Group>
        <Progress
          aria-label={t("overallProgress")}
          mt="xs"
          radius="xl"
          size={6}
          value={percentage ?? 0}
        />
      </button>
      <Group className={classes.taskActions} gap="xs" wrap="nowrap">
        <TaskAction
          icon={<ListMagnifyingGlass aria-hidden size={17} />}
          label={t("preview")}
          onClick={onPreview}
        />
        {!active || isTerminal(run.state) ? (
          <TaskAction
            icon={<Play aria-hidden size={17} weight="fill" />}
            label={t("runTask")}
            onClick={onRun}
          />
        ) : null}
        {run?.state === "paused" ? (
          <TaskAction
            icon={<Play aria-hidden size={17} weight="fill" />}
            label={t("resumeTask")}
            onClick={onResume}
          />
        ) : run && ["planning", "running"].includes(run.state) ? (
          <TaskAction
            icon={<Pause aria-hidden size={17} weight="fill" />}
            label={t("pauseTask")}
            onClick={onPause}
          />
        ) : null}
        {active ? (
          <TaskAction
            color="red"
            icon={<Stop aria-hidden size={17} weight="fill" />}
            label={t("stopTask")}
            onClick={onStop}
          />
        ) : null}
      </Group>
    </Paper>
  );
}

function TaskAction({
  label,
  icon,
  color,
  onClick,
}: {
  label: string;
  icon: React.ReactNode;
  color?: string;
  onClick: () => void;
}) {
  return (
    <Tooltip label={label}>
      <ActionIcon
        aria-label={label}
        color={color}
        size="lg"
        variant={color ? "light" : "default"}
        onClick={onClick}
      >
        {icon}
      </ActionIcon>
    </Tooltip>
  );
}

function BottomCommandBar({
  settings,
  queueCounts,
  overall,
  hasPaused,
  onRunAll,
  onPauseAll,
  onStopAll,
  onClear,
  onEditDefaults,
}: {
  settings: BootstrapSnapshot["settings"];
  queueCounts: {
    running: number;
    queued: number;
    paused: number;
    succeeded: number;
    failed: number;
  };
  overall: number | null;
  hasPaused: boolean;
  onRunAll: () => void;
  onPauseAll: () => void;
  onStopAll: () => void;
  onClear: () => void;
  onEditDefaults: () => void;
}) {
  const { t } = useI18n();
  return (
    <footer className={classes.commandBar}>
      <button
        aria-label={`${t("editDefaults")}: ${t("defaultOutput")}`}
        className={classes.summaryButton}
        type="button"
        onClick={onEditDefaults}
      >
        <Meta
          label={t("defaultOutput")}
          value={settings.archive_defaults.output_directory}
        />
      </button>
      <button
        aria-label={`${t("editDefaults")}: ${t("archiveDefaults")}`}
        className={`${classes.summaryButton} ${classes.archiveSummary}`}
        type="button"
        onClick={onEditDefaults}
      >
        <Meta
          label={t("archiveDefaults")}
          value={`${t(formatMessage(settings.archive_defaults.format))} · ${t(
            compressionMessage(settings.archive_defaults.compression),
          )}`}
        />
      </button>
      <Text className={classes.queueSummary} c="dimmed" size="sm">
        {Object.values(queueCounts).every((count) => count === 0)
          ? t("queueEmpty")
          : t("aggregateSummary", queueCounts)}
      </Text>
      <Group className={classes.globalActions} gap="sm" wrap="nowrap">
        <Tooltip label={t("clearSelection")}>
          <ActionIcon
            aria-label={t("clearSelection")}
            size="lg"
            variant="subtle"
            onClick={onClear}
          >
            <SelectionSlash aria-hidden size={17} />
          </ActionIcon>
        </Tooltip>
        <Button
          leftSection={<Play aria-hidden size={17} weight="fill" />}
          onClick={onRunAll}
        >
          {t("runAll")}
        </Button>
        <Button
          leftSection={
            hasPaused ? (
              <Play aria-hidden size={17} weight="fill" />
            ) : (
              <Pause aria-hidden size={17} weight="fill" />
            )
          }
          variant="default"
          onClick={onPauseAll}
        >
          {hasPaused ? t("resumeAll") : t("pauseAll")}
        </Button>
        <Button
          color="red"
          leftSection={<Stop aria-hidden size={17} weight="fill" />}
          variant="outline"
          onClick={onStopAll}
        >
          {t("stopAll")}
        </Button>
      </Group>
      <Box className={classes.overallProgress}>
        <Group justify="space-between" mb={5}>
          <Text c="dimmed" size="xs">
            {t("overallProgress")}
          </Text>
          <Text fw={650} size="xs">
            {overall === null ? "—" : `${overall}%`}
          </Text>
        </Group>
        <Progress
          aria-label={t("overallProgress")}
          animated={overall === null && queueCounts.running > 0}
          radius="xl"
          size={7}
          value={overall ?? (queueCounts.running > 0 ? 100 : 0)}
        />
      </Box>
    </footer>
  );
}

function DefaultsModal({
  opened,
  settings,
  onClose,
  onSave,
}: {
  opened: boolean;
  settings: Settings;
  onClose: () => void;
  onSave: (settings: Settings) => Promise<void>;
}) {
  const { t } = useI18n();
  const [draft, setDraft] = useState(settings);
  const close = () => {
    setDraft(settings);
    onClose();
  };
  return (
    <Modal
      centered
      opened={opened}
      size="lg"
      title={t("defaultArchiveSettings")}
      onClose={close}
    >
      <Stack>
        <TextInput
          label={t("outputDirectory")}
          value={draft.archive_defaults.output_directory}
          onChange={(event) =>
            setDraft((current) => ({
              ...current,
              archive_defaults: {
                ...current.archive_defaults,
                output_directory: event.currentTarget.value,
              },
            }))
          }
        />
        <Group grow>
          <Select
            data={[
              { label: t("zip"), value: "zip" },
              { label: t("tarGz"), value: "tar_gz" },
              { label: t("tarZst"), value: "tar_zst" },
            ]}
            label={t("format")}
            value={draft.archive_defaults.format}
            onChange={(value) =>
              value &&
              setDraft((current) => ({
                ...current,
                archive_defaults: {
                  ...current.archive_defaults,
                  format: value as typeof current.archive_defaults.format,
                },
              }))
            }
          />
          <Select
            data={[
              { label: t("fast"), value: "fast" },
              { label: t("balanced"), value: "balanced" },
              { label: t("maximum"), value: "maximum" },
            ]}
            label={t("compression")}
            value={draft.archive_defaults.compression}
            onChange={(value) =>
              value &&
              setDraft((current) => ({
                ...current,
                archive_defaults: {
                  ...current.archive_defaults,
                  compression:
                    value as typeof current.archive_defaults.compression,
                },
              }))
            }
          />
        </Group>
        <NumberInput
          clampBehavior="strict"
          label={t("maxParallelRuns")}
          max={64}
          min={1}
          value={draft.execution.max_parallel_runs}
          onChange={(value) =>
            typeof value === "number" &&
            setDraft((current) => ({
              ...current,
              execution: { ...current.execution, max_parallel_runs: value },
            }))
          }
        />
      </Stack>
      <Group justify="flex-end" mt="lg">
        <Button variant="default" onClick={close}>
          {t("cancel")}
        </Button>
        <Button onClick={() => void onSave(draft)}>{t("saveDefaults")}</Button>
      </Group>
    </Modal>
  );
}

function Meta({ label, value }: { label: string; value: string }) {
  return (
    <Box>
      <Text c="dimmed" size="xs">
        {label}
      </Text>
      <Text fw={550} mt={2} size="sm">
        {value}
      </Text>
    </Box>
  );
}

function formatMessage(
  format: BootstrapSnapshot["settings"]["archive_defaults"]["format"],
): MessageKey {
  return format === "zip" ? "zip" : format === "tar_gz" ? "tarGz" : "tarZst";
}

function compressionMessage(
  compression: BootstrapSnapshot["settings"]["archive_defaults"]["compression"],
): MessageKey {
  return compression;
}

function runProgress(
  run: RunRecord,
  progress: ProgressSnapshot | undefined,
): number | null {
  if (progress?.total_bytes && Number(progress.total_bytes) > 0) {
    return Math.min(
      100,
      Math.round(
        (Number(progress.completed_bytes) / Number(progress.total_bytes)) * 100,
      ),
    );
  }
  if (run.summary) {
    return 100;
  }
  return null;
}

function latestRuns(runs: RunRecord[]): Map<string, RunRecord> {
  const result = new Map<string, RunRecord>();
  for (const run of runs) {
    const current = result.get(run.task_id);
    if (!current || current.started_at <= run.started_at) {
      result.set(run.task_id, run);
    }
  }
  return result;
}

function runStateSummary(runs: RunRecord[]): {
  running: number;
  queued: number;
  paused: number;
  succeeded: number;
  failed: number;
} {
  return {
    running: runs.filter((run) =>
      ["planning", "running", "stopping"].includes(run.state),
    ).length,
    queued: runs.filter((run) => run.state === "queued").length,
    paused: runs.filter((run) => run.state === "paused").length,
    succeeded: runs.filter((run) =>
      ["succeeded", "succeeded_with_warnings"].includes(run.state),
    ).length,
    failed: runs.filter((run) =>
      ["failed", "stopped", "interrupted"].includes(run.state),
    ).length,
  };
}

function isTerminal(state: RunRecord["state"]): boolean {
  return [
    "succeeded",
    "succeeded_with_warnings",
    "failed",
    "stopped",
    "interrupted",
  ].includes(state);
}
