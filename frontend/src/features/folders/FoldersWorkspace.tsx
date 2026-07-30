import {
  ActionIcon,
  Alert,
  Badge,
  Box,
  Button,
  Checkbox,
  Drawer,
  Group,
  Modal,
  Paper,
  Progress,
  ScrollArea,
  Stack,
  Text,
  Title,
  Tooltip,
} from "@mantine/core";
import { useDisclosure } from "@mantine/hooks";
import {
  ClockCounterClockwise,
  Eye,
  FolderOpen,
  FolderSimple,
  Pause,
  Play,
  Plus,
  Stop,
  Trash,
  WarningCircle,
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
  BrowserView,
  Folder,
  FolderAddResult,
  ProgressSnapshot,
  RunRecord,
} from "../../shared/contracts/generated";
import { useI18n } from "../../shared/i18n/I18nProvider";
import { useDesktopData } from "../../shared/ipc/DesktopDataProvider";
import { isTerminalRunState } from "../../shared/runs/runState";
import { RunStatus } from "../../shared/ui/RunStatus";
import { FolderBrowser } from "./FolderBrowser";
import { FolderInspector } from "./FolderInspector";
import { basename } from "./folderModel";
import { folderResultExpiresAt, resolveFolderStatus } from "./folderStatus";
import { runStateSummary, type RunStateSummary } from "./runQueue";
import classes from "./FoldersWorkspace.module.css";

type FoldersWorkspaceProps = {
  snapshot: BootstrapSnapshot;
};

const RunExplorer = lazy(() =>
  import("./RunExplorer").then((module) => ({ default: module.RunExplorer })),
);

export function FoldersWorkspace({ snapshot }: FoldersWorkspaceProps) {
  const { t } = useI18n();
  const { command, preview, progressByRun, query, reload, sessionStartedAt } =
    useDesktopData();
  const listedFolders = useMemo(
    () => snapshot.plan.folders.filter((folder) => folder.listed),
    [snapshot.plan.folders],
  );
  const [selectedFolderId, setSelectedFolderId] = useState<string | null>(
    listedFolders[0]?.id ?? null,
  );
  const [duplicateFolderId, setDuplicateFolderId] = useState<string | null>(
    null,
  );
  const [dragging, setDragging] = useState(false);
  const [removeFolder, setRemoveFolder] = useState<Folder | null>(null);
  const [cancelQueued, setCancelQueued] = useState(true);
  const [hiddenOpened, hiddenModal] = useDisclosure(false);
  const [explorer, setExplorer] = useState<{
    folderId: string;
    tab: "preview" | "history";
    actionId: string | null;
  } | null>(null);
  const [folderBrowser, setFolderBrowser] = useState<
    | { type: "multi-toggle-folders"; initialPath?: string }
    | {
        type: "single-directory";
        initialPath: string;
        sourcePath: string;
        onConfirm: (path: string) => void;
      }
    | null
  >(null);
  const [browserView, setBrowserView] = useState<BrowserView>(
    snapshot.settings.browser.view,
  );
  const [globalPauseRequested, setGlobalPauseRequested] = useState(false);

  const updateBrowserView = useCallback(
    (next: BrowserView) => {
      const previous = browserView;
      setBrowserView(next);
      void query<BrowserView>("set_browser_view", { view: next }).catch(() => {
        setBrowserView((current) => (current === next ? previous : current));
      });
    },
    [browserView, query],
  );

  const selectedFolder =
    listedFolders.find((folder) => folder.id === selectedFolderId) ?? null;
  const nonTerminalRuns = snapshot.active_runs.filter(
    (run) => !isTerminalRunState(run.state),
  );
  const latestByFolder = useMemo(
    () => latestFolderRuns([...snapshot.active_runs, ...snapshot.recent_runs]),
    [snapshot.active_runs, snapshot.recent_runs],
  );
  const defaultProfileId =
    snapshot.profiles.find(
      (profile) =>
        profile.id === snapshot.settings.default_profile_id && profile.valid,
    )?.id ??
    snapshot.profiles.find((profile) => profile.id && profile.valid)?.id ??
    null;

  useEffect(() => {
    if (
      selectedFolderId &&
      !listedFolders.some((folder) => folder.id === selectedFolderId)
    ) {
      queueMicrotask(() => setSelectedFolderId(listedFolders[0]?.id ?? null));
    }
  }, [listedFolders, selectedFolderId]);

  const addPaths = useCallback(
    async (paths: string[]) => {
      for (const source of paths) {
        const result = await command<FolderAddResult>("add_folder", {
          source,
          defaultProfileId,
        });
        if (result) {
          setSelectedFolderId(result.folder.id);
          setDuplicateFolderId(result.created ? null : result.folder.id);
        }
      }
      setDragging(false);
    },
    [command, defaultProfileId],
  );

  const openFolderBrowser = useCallback(() => {
    setFolderBrowser({
      type: "multi-toggle-folders",
      initialPath: selectedFolder?.source ?? snapshot.roots[0]?.path,
    });
  }, [selectedFolder?.source, snapshot.roots]);

  useEffect(() => {
    if (preview) return;
    let dispose: (() => void) | undefined;
    let active = true;
    void import("@tauri-apps/api/webview")
      .then(({ getCurrentWebview }) =>
        getCurrentWebview().onDragDropEvent((event) => {
          if (!active) return;
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
        if (active) dispose = unlisten;
        else unlisten();
      });
    return () => {
      active = false;
      dispose?.();
    };
  }, [addPaths, preview]);

  const queueCounts = runStateSummary(snapshot.active_runs);
  const overall = overallProgress(nonTerminalRuns, progressByRun);
  const hasPaused =
    globalPauseRequested ||
    nonTerminalRuns.some((run) => run.state === "paused");
  const hasStopping = nonTerminalRuns.some((run) => run.state === "stopping");
  const hasStoppable = nonTerminalRuns.some((run) => run.state !== "stopping");
  const queuePositionByRun = new Map(
    nonTerminalRuns
      .filter((run) => run.state === "queued")
      .map((run, index) => [run.run_id, index + 1]),
  );

  useEffect(() => {
    if (nonTerminalRuns.length === 0 && globalPauseRequested) {
      queueMicrotask(() => setGlobalPauseRequested(false));
    }
  }, [globalPauseRequested, nonTerminalRuns.length]);

  return (
    <Box className={classes.shell}>
      <Box className={classes.workspace} data-dragging={dragging || undefined}>
        <section className={classes.foldersPane}>
          <Group justify="space-between" wrap="nowrap">
            <Title order={1}>{t("folders")}</Title>
            <Group gap="xs" wrap="nowrap">
              <Button size="xs" variant="subtle" onClick={hiddenModal.open}>
                {t("hiddenFolders")}
              </Button>
              <Tooltip label={t("addFolders")}>
                <ActionIcon
                  aria-label={t("addFolders")}
                  size="lg"
                  variant="default"
                  onClick={openFolderBrowser}
                >
                  <Plus aria-hidden size={19} />
                </ActionIcon>
              </Tooltip>
            </Group>
          </Group>

          {duplicateFolderId ? (
            <Alert color="blue" onClose={() => setDuplicateFolderId(null)}>
              {t("duplicateFolderFocused")}
            </Alert>
          ) : null}

          {listedFolders.length ? (
            <ScrollArea className={classes.folderScroll} offsetScrollbars>
              <Stack gap="md" pr="sm">
                {listedFolders.map((folder) => (
                  <FolderCard
                    key={folder.id}
                    folder={folder}
                    activeRuns={nonTerminalRuns.filter(
                      (run) => run.folder_id === folder.id,
                    )}
                    latestRun={latestByFolder.get(folder.id)}
                    profileName={
                      snapshot.profiles.find(
                        (profile) => profile.id === folder.default_profile_id,
                      )?.name ?? "—"
                    }
                    selected={folder.id === selectedFolderId}
                    sessionStartedAt={sessionStartedAt}
                    onHistory={() =>
                      setExplorer({
                        folderId: folder.id,
                        tab: "history",
                        actionId: null,
                      })
                    }
                    onPreview={() =>
                      setExplorer({
                        folderId: folder.id,
                        tab: "preview",
                        actionId: null,
                      })
                    }
                    onRemove={() => setRemoveFolder(folder)}
                    onRun={() =>
                      void command<RunRecord[]>("run_folder", {
                        folderId: folder.id,
                      })
                    }
                    onSelect={() => {
                      setSelectedFolderId(folder.id);
                      setDuplicateFolderId(null);
                    }}
                  />
                ))}
              </Stack>
            </ScrollArea>
          ) : (
            <Paper className={classes.emptyState} withBorder>
              <FolderOpen aria-hidden size={38} />
              <Title order={2}>{t("noFolders")}</Title>
              <Text c="dimmed" maw={420} ta="center">
                {dragging ? t("dropFolders") : t("noFoldersHint")}
              </Text>
              <Button
                leftSection={<Plus aria-hidden size={18} />}
                onClick={openFolderBrowser}
              >
                {t("addFolders")}
              </Button>
            </Paper>
          )}
        </section>

        <FolderInspector
          key={selectedFolder?.id ?? "no-folder"}
          activeRuns={
            selectedFolder
              ? nonTerminalRuns.filter(
                  (run) => run.folder_id === selectedFolder.id,
                )
              : []
          }
          folder={selectedFolder}
          profiles={snapshot.profiles}
          progressByRun={progressByRun}
          queuePositionByRun={queuePositionByRun}
          onOpenActivity={(actionId, tab) =>
            selectedFolder &&
            setExplorer({
              folderId: selectedFolder.id,
              actionId,
              tab,
            })
          }
          onBrowseOutput={(sourcePath, initialPath, onConfirm) =>
            setFolderBrowser({
              type: "single-directory",
              sourcePath,
              initialPath,
              onConfirm,
            })
          }
        />

        {dragging ? (
          <Box aria-live="polite" className={classes.dropOverlay}>
            <FolderOpen aria-hidden size={40} />
            <Text fw={650}>{t("dropFolders")}</Text>
          </Box>
        ) : null}
      </Box>

      <Drawer.Root
        closeOnClickOutside
        closeOnEscape={false}
        lockScroll
        opened={folderBrowser !== null}
        position="right"
        size="min(100vw, max(800px, 60vw))"
        trapFocus
        onClose={() => setFolderBrowser(null)}
      >
        <Drawer.Overlay className={classes.browserBackdrop} />
        <Drawer.Content
          aria-label={t("folderBrowser")}
          classNames={{ content: classes.browserDrawerContent }}
        >
          <Drawer.Body className={classes.browserDrawerBody}>
            {folderBrowser ? (
              <FolderBrowser
                initialPath={folderBrowser.initialPath}
                mode={
                  folderBrowser.type === "multi-toggle-folders"
                    ? {
                        type: "multi-toggle-folders",
                        addedPaths: new Set(
                          listedFolders.map((folder) => folder.source),
                        ),
                        onToggle: async (path, added) => {
                          if (added) {
                            const folder = listedFolders.find(
                              (candidate) => candidate.source === path,
                            );
                            if (folder) {
                              await command("unlist_folder", {
                                folderId: folder.id,
                                cancelQueued: true,
                              });
                            }
                          } else {
                            await addPaths([path]);
                          }
                        },
                      }
                    : {
                        type: "single-directory",
                        sourcePath: folderBrowser.sourcePath,
                        onConfirm: (path) => {
                          folderBrowser.onConfirm(path);
                          setFolderBrowser(null);
                        },
                      }
                }
                roots={snapshot.roots}
                view={browserView}
                onClose={() => setFolderBrowser(null)}
                onViewChange={updateBrowserView}
              />
            ) : null}
          </Drawer.Body>
        </Drawer.Content>
      </Drawer.Root>

      <GlobalQueueBar
        counts={queueCounts}
        hasPaused={hasPaused}
        hasStopping={hasStopping}
        hasStoppable={hasStoppable}
        overall={overall}
        onPauseAll={() => {
          const resume = hasPaused;
          void command<number>(resume ? "resume_all" : "pause_all").then(
            (changed) => {
              if (changed !== undefined) {
                setGlobalPauseRequested(!resume);
              }
            },
          );
        }}
        onRunAll={() => void command("run_all_enabled")}
        onStopAll={() =>
          void command<number>("stop_all").then((changed) => {
            if (changed !== undefined) {
              setGlobalPauseRequested(false);
            }
          })
        }
      />

      <Modal
        centered
        opened={removeFolder !== null}
        title={t("removeFromFolders")}
        onClose={() => setRemoveFolder(null)}
      >
        <Alert
          color="yellow"
          icon={<WarningCircle aria-hidden size={19} />}
          title={removeFolder ? basename(removeFolder.source) : undefined}
        >
          {t("removeFolderHint")}
        </Alert>
        <Checkbox
          checked={cancelQueued}
          label={t("cancelQueuedRuns")}
          mt="md"
          onChange={(event) => setCancelQueued(event.currentTarget.checked)}
        />
        <Group justify="flex-end" mt="lg">
          <Button variant="default" onClick={() => setRemoveFolder(null)}>
            {t("cancel")}
          </Button>
          <Button
            color="red"
            onClick={async () => {
              if (!removeFolder) return;
              try {
                await query<boolean>("unlist_folder", {
                  folderId: removeFolder.id,
                  cancelQueued,
                });
                setRemoveFolder(null);
                await reload();
              } catch {
                // The shared error surface explains active running/paused Runs.
              }
            }}
          >
            {t("removeFromFolders")}
          </Button>
        </Group>
      </Modal>

      <HiddenFoldersModal opened={hiddenOpened} onClose={hiddenModal.close} />

      {explorer ? (
        <Suspense fallback={null}>
          <RunExplorer
            key={`${explorer.folderId}-${explorer.tab}`}
            initialTab={explorer.tab}
            initialActionId={explorer.actionId}
            opened
            folder={
              snapshot.plan.folders.find(
                (folder) => folder.id === explorer.folderId,
              ) ?? null
            }
            onClose={() => setExplorer(null)}
          />
        </Suspense>
      ) : null}
    </Box>
  );
}

function FolderCard({
  folder,
  activeRuns,
  profileName,
  latestRun,
  selected,
  sessionStartedAt,
  onSelect,
  onRun,
  onPreview,
  onHistory,
  onRemove,
}: {
  folder: Folder;
  activeRuns: RunRecord[];
  profileName: string;
  latestRun?: RunRecord;
  selected: boolean;
  sessionStartedAt: number;
  onSelect: () => void;
  onRun: () => void;
  onPreview: () => void;
  onHistory: () => void;
  onRemove: () => void;
}) {
  const { t } = useI18n();
  const enabledActions = folder.actions.filter(
    (action) => action.enabled,
  ).length;
  const [statusNow, setStatusNow] = useState(Date.now);
  const resultExpiresAt = folderResultExpiresAt(latestRun, sessionStartedAt);
  useEffect(() => {
    if (
      activeRuns.length > 0 ||
      resultExpiresAt === null ||
      resultExpiresAt <= statusNow
    ) {
      return;
    }
    const timeout = window.setTimeout(
      () => setStatusNow(Date.now()),
      Math.max(0, resultExpiresAt - Date.now()) + 25,
    );
    return () => window.clearTimeout(timeout);
  }, [activeRuns.length, resultExpiresAt, statusNow]);
  const aggregateState = resolveFolderStatus(
    activeRuns,
    latestRun,
    sessionStartedAt,
    statusNow,
  );
  return (
    <Paper
      aria-current={selected ? "true" : undefined}
      className={classes.folderCard}
      component="article"
      data-selected={selected || undefined}
      withBorder
    >
      <button className={classes.folderSelect} type="button" onClick={onSelect}>
        <Group gap="md" wrap="nowrap">
          <Box
            aria-hidden
            className={classes.folderIcon}
            data-enabled={folder.enabled}
          >
            <FolderSimple size={25} weight="duotone" />
          </Box>
          <Box miw={0}>
            <Text fw={700}>{basename(folder.source)}</Text>
            <Text className={classes.path} c="dimmed" mt={3} size="xs">
              {folder.source}
            </Text>
          </Box>
        </Group>
        <Group
          className={classes.folderMeta}
          gap="xl"
          justify="space-between"
          wrap="nowrap"
        >
          <Meta label={t("defaultIgnoreProfile")} value={profileName} />
          <Meta
            align="right"
            label={t("enabledActions")}
            value={`${enabledActions}/${folder.actions.length}`}
          />
        </Group>
      </button>

      <Box className={classes.folderStatus}>
        <RunStatus state={aggregateState} />
      </Box>

      <Group className={classes.folderActions} gap={5} wrap="nowrap">
        <CardAction
          icon={<Eye aria-hidden size={17} />}
          label={t("preview")}
          onClick={onPreview}
        />
        <CardAction
          disabled={
            enabledActions === 0 ||
            activeRuns.some((run) => run.state === "stopping")
          }
          icon={<Play aria-hidden size={17} weight="fill" />}
          label={t("runFolder")}
          onClick={onRun}
        />
        <CardAction
          icon={<ClockCounterClockwise aria-hidden size={17} />}
          label={t("runHistory")}
          onClick={onHistory}
        />
        <CardAction
          color="red"
          icon={<Trash aria-hidden size={17} />}
          label={t("removeFromFolders")}
          onClick={onRemove}
        />
      </Group>
    </Paper>
  );
}

function CardAction({
  label,
  icon,
  color,
  disabled,
  onClick,
}: {
  label: string;
  icon: React.ReactNode;
  color?: string;
  disabled?: boolean;
  onClick: () => void;
}) {
  return (
    <Tooltip label={label}>
      <ActionIcon
        aria-label={label}
        color={color}
        disabled={disabled}
        size="lg"
        variant={color ? "light" : "default"}
        onClick={onClick}
      >
        {icon}
      </ActionIcon>
    </Tooltip>
  );
}

function HiddenFoldersModal({
  opened,
  onClose,
}: {
  opened: boolean;
  onClose: () => void;
}) {
  const { t } = useI18n();
  const { command, query } = useDesktopData();
  const [folders, setFolders] = useState<Folder[]>([]);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [confirmAll, setConfirmAll] = useState(false);

  useEffect(() => {
    if (!opened) return;
    void query<Folder[]>("unlisted_folders").then(setFolders);
  }, [opened, query]);

  const remove = async (folderIds: string[]) => {
    const removed = await command<number>("forget_folders", {
      folderIds,
      cancelQueued: true,
    });
    if (removed !== undefined) {
      setFolders((current) =>
        current.filter((folder) => !folderIds.includes(folder.id)),
      );
      setSelected(new Set());
    }
  };

  return (
    <Modal
      centered
      opened={opened}
      size="lg"
      title={t("hiddenFolders")}
      onClose={onClose}
    >
      {folders.length ? (
        <Stack>
          <ScrollArea.Autosize mah={420} offsetScrollbars>
            <Stack gap="xs" pr="sm">
              {folders.map((folder) => (
                <Paper key={folder.id} p="sm" withBorder>
                  <Group wrap="nowrap">
                    <Checkbox
                      aria-label={t("selectFolderNamed", {
                        name: basename(folder.source),
                      })}
                      checked={selected.has(folder.id)}
                      onChange={(event) =>
                        setSelected((current) => {
                          const next = new Set(current);
                          if (event.currentTarget.checked) next.add(folder.id);
                          else next.delete(folder.id);
                          return next;
                        })
                      }
                    />
                    <Box miw={0}>
                      <Text fw={600}>{basename(folder.source)}</Text>
                      <Text className={classes.path} c="dimmed" size="xs">
                        {folder.source}
                      </Text>
                      <Text c="dimmed" size="xs">
                        {t("actionCount", { count: folder.actions.length })}
                      </Text>
                    </Box>
                  </Group>
                </Paper>
              ))}
            </Stack>
          </ScrollArea.Autosize>
          <Group justify="space-between">
            <Button
              color="red"
              disabled={selected.size === 0}
              variant="outline"
              onClick={() => void remove([...selected])}
            >
              {t("deleteSelected")}
            </Button>
            <Button
              color="red"
              variant="subtle"
              onClick={() => setConfirmAll(true)}
            >
              {t("deleteAll")}
            </Button>
          </Group>
        </Stack>
      ) : (
        <Text c="dimmed">{t("noHiddenFolders")}</Text>
      )}

      <Modal
        centered
        opened={confirmAll}
        title={t("deleteAllHidden")}
        onClose={() => setConfirmAll(false)}
      >
        <Alert color="red" icon={<WarningCircle aria-hidden size={19} />}>
          {t("deleteAllHiddenHint")}
        </Alert>
        <Group justify="flex-end" mt="lg">
          <Button variant="default" onClick={() => setConfirmAll(false)}>
            {t("cancel")}
          </Button>
          <Button
            color="red"
            onClick={async () => {
              const removed = await command<number>(
                "forget_all_unlisted_folders",
                { cancelQueued: true },
              );
              if (removed !== undefined) {
                setFolders([]);
                setConfirmAll(false);
              }
            }}
          >
            {t("deleteAll")}
          </Button>
        </Group>
      </Modal>
    </Modal>
  );
}

function GlobalQueueBar({
  counts,
  overall,
  hasPaused,
  hasStopping,
  hasStoppable,
  onRunAll,
  onPauseAll,
  onStopAll,
}: {
  counts: RunStateSummary;
  overall: number | null;
  hasPaused: boolean;
  hasStopping: boolean;
  hasStoppable: boolean;
  onRunAll: () => void;
  onPauseAll: () => void;
  onStopAll: () => void;
}) {
  const { t } = useI18n();
  return (
    <footer className={classes.commandBar}>
      <Group gap="xs" wrap="wrap">
        <Badge color="blue" variant="light">
          {t("runningCount", { count: counts.running })}
        </Badge>
        <Badge color="gray" variant="light">
          {t("queuedCount", { count: counts.queued })}
        </Badge>
        <Badge color="gray" variant="light">
          {t("pausedCount", { count: counts.paused })}
        </Badge>
      </Group>
      <Box className={classes.overallProgress}>
        <Group justify="space-between" mb={5}>
          <Text c="dimmed" size="xs">
            {t("overallProgress")}
          </Text>
          {overall === null ? null : (
            <Text fw={650} size="xs">
              {overall}%
            </Text>
          )}
        </Group>
        <Progress.Root radius="xl" size={7}>
          <Progress.Section
            aria-label={t("overallProgress")}
            animated={overall === null && counts.running > 0}
            value={overall ?? (counts.running > 0 ? 100 : 0)}
          />
        </Progress.Root>
      </Box>
      <Group gap="sm" wrap="nowrap">
        <Button
          disabled={hasStopping}
          leftSection={<Play aria-hidden size={17} weight="fill" />}
          onClick={onRunAll}
        >
          {t("runAllEnabledActions")}
        </Button>
        <Button
          disabled={!hasStoppable || hasStopping}
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
          disabled={!hasStoppable}
          leftSection={<Stop aria-hidden size={17} weight="fill" />}
          variant="outline"
          onClick={onStopAll}
        >
          {t("stopAll")}
        </Button>
      </Group>
    </footer>
  );
}

function Meta({
  label,
  value,
  align = "left",
}: {
  label: string;
  value: string;
  align?: "left" | "right";
}) {
  return (
    <Box ta={align}>
      <Text c="dimmed" size="xs">
        {label}
      </Text>
      <Text
        className={classes.folderMetaValue}
        component="span"
        fw={550}
        mt={4}
        size="sm"
      >
        {value}
      </Text>
    </Box>
  );
}

function latestFolderRuns(runs: RunRecord[]): Map<string, RunRecord> {
  const result = new Map<string, RunRecord>();
  for (const run of runs) {
    const current = result.get(run.folder_id);
    if (!current || runTimestamp(current) < runTimestamp(run)) {
      result.set(run.folder_id, run);
    }
  }
  return result;
}

function runTimestamp(run: RunRecord): number {
  const timestamp = Date.parse(run.finished_at ?? run.started_at);
  return Number.isFinite(timestamp) ? timestamp : 0;
}

function overallProgress(
  runs: RunRecord[],
  progressByRun: ReadonlyMap<string, ProgressSnapshot>,
): number | null {
  const values = runs
    .filter((run) => run.state !== "queued")
    .map((run) => {
      const progress = progressByRun.get(run.run_id);
      if (!progress?.total_bytes || Number(progress.total_bytes) === 0) {
        return null;
      }
      return Math.min(
        100,
        Math.round(
          (Number(progress.completed_bytes) / Number(progress.total_bytes)) *
            100,
        ),
      );
    });
  return values.length === 0 || values.some((value) => value === null)
    ? null
    : Math.round(
        values.reduce<number>((total, value) => total + (value ?? 0), 0) /
          values.length,
      );
}
