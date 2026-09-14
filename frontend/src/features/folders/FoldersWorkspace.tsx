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
  Menu,
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
import { useDisclosure } from "@mantine/hooks";
import {
  ClockCounterClockwise,
  ArrowsClockwise,
  DotsThree,
  Eye,
  FolderOpen,
  FolderSimple,
  ListBullets,
  Pause,
  Play,
  Plus,
  MagnifyingGlass,
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
  FolderOperationalSummary,
  FolderSortMode,
  ProgressSnapshot,
  RunChangedResult,
  RunRecord,
} from "../../shared/contracts/generated";
import { useI18n } from "../../shared/i18n/I18nProvider";
import { useDesktopData } from "../../shared/ipc/DesktopDataProvider";
import { isTerminalRunState } from "../../shared/runs/runState";
import { RunStatus } from "../../shared/ui/RunStatus";
import { FolderBrowser } from "./FolderBrowser";
import { FolderInspector } from "./FolderInspector";
import { basename, filterFolders, sortFolders } from "./folderModel";
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
  const {
    command,
    preview,
    progressByRun,
    queuePositionByRun,
    query,
    reload,
    sessionStartedAt,
  } = useDesktopData();
  const allListedFolders = useMemo(
    () => snapshot.plan.folders.filter((folder) => folder.listed),
    [snapshot.plan.folders],
  );
  const [selectedFolderId, setSelectedFolderId] = useState<string | null>(
    allListedFolders[0]?.id ?? null,
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
  const [search, setSearch] = useState("");
  const [sortMode, setSortMode] = useState<FolderSortMode>(
    snapshot.settings.folder_sort_mode,
  );
  const [runChangedResult, setRunChangedResult] =
    useState<RunChangedResult | null>(null);
  const summaryByFolder = useMemo(
    () =>
      new Map(
        snapshot.folder_summaries.map((summary) => [
          summary.folder_id,
          summary,
        ]),
      ),
    [snapshot.folder_summaries],
  );
  const listedFolders = useMemo(
    () =>
      sortFolders(
        filterFolders(allListedFolders, search),
        sortMode,
        summaryByFolder,
      ),
    [allListedFolders, search, sortMode, summaryByFolder],
  );

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

  const updateSortMode = useCallback(
    (next: FolderSortMode) => {
      setSortMode(next);
      void command("save_settings", {
        settings: { ...snapshot.settings, folder_sort_mode: next },
      });
    },
    [command, snapshot.settings],
  );

  const selectedFolder =
    allListedFolders.find((folder) => folder.id === selectedFolderId) ?? null;
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
      !allListedFolders.some((folder) => folder.id === selectedFolderId)
    ) {
      queueMicrotask(() =>
        setSelectedFolderId(allListedFolders[0]?.id ?? null),
      );
    }
  }, [allListedFolders, selectedFolderId]);

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

          <Group gap="xs" grow wrap="nowrap">
            <TextInput
              aria-label={t("searchFolders")}
              leftSection={<MagnifyingGlass aria-hidden size={15} />}
              placeholder={t("searchFolders")}
              value={search}
              onChange={(event) => setSearch(event.currentTarget.value)}
            />
            <Select
              aria-label={t("sortFolders")}
              data={[
                { label: t("sortNameAscending"), value: "name_ascending" },
                { label: t("sortNameDescending"), value: "name_descending" },
                { label: t("sortRecentlyAdded"), value: "recently_added" },
                { label: t("sortOldestAdded"), value: "oldest_added" },
                { label: t("sortRecentlyRun"), value: "recently_run" },
                {
                  label: t("sortLeastRecentlyRun"),
                  value: "least_recently_run",
                },
              ]}
              value={sortMode}
              onChange={(value) =>
                value && updateSortMode(value as FolderSortMode)
              }
            />
          </Group>

          {allListedFolders.length > 0 && listedFolders.length > 0 ? (
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
                    summary={summaryByFolder.get(folder.id)}
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
                    onLocate={() =>
                      setFolderBrowser({
                        type: "single-directory",
                        initialPath: folder.source,
                        sourcePath: folder.source,
                        onConfirm: (newSource) => {
                          void command("locate_folder", {
                            folderId: folder.id,
                            newSource,
                          });
                        },
                      })
                    }
                    onOpen={() =>
                      void command("open_source_folder", {
                        folderId: folder.id,
                      })
                    }
                    onReveal={() =>
                      void command("reveal_last_folder_output", {
                        folderId: folder.id,
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
          ) : allListedFolders.length ? (
            <Paper className={classes.emptyState} withBorder>
              <MagnifyingGlass aria-hidden size={38} />
              <Text>{t("noFoldersMatch")}</Text>
            </Paper>
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
                          allListedFolders.map((folder) => folder.source),
                        ),
                        onToggle: async (path, added) => {
                          if (added) {
                            const folder = allListedFolders.find(
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
        progressByRun={progressByRun}
        queuePositionByRun={queuePositionByRun}
        runs={nonTerminalRuns}
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
        onRunChanged={() =>
          void command<RunChangedResult>("run_changed").then((result) => {
            if (result) setRunChangedResult(result);
          })
        }
        onStopAll={() =>
          void command<number>("stop_all").then((changed) => {
            if (changed !== undefined) {
              setGlobalPauseRequested(false);
            }
          })
        }
      />

      {runChangedResult ? (
        <Text aria-live="polite" className={classes.batchResult} size="xs">
          {t("runChangedResult", {
            queued: runChangedResult.queued.toString(),
            unchanged: runChangedResult.unchanged.toString(),
            skipped: (
              runChangedResult.missing +
              runChangedResult.invalid +
              runChangedResult.already_running +
              runChangedResult.output_conflict_skipped
            ).toString(),
            conflicts: runChangedResult.output_conflict_skipped.toString(),
          })}
        </Text>
      ) : null}

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
  summary,
  selected,
  sessionStartedAt,
  onSelect,
  onRun,
  onPreview,
  onHistory,
  onRemove,
  onLocate,
  onOpen,
  onReveal,
}: {
  folder: Folder;
  activeRuns: RunRecord[];
  profileName: string;
  latestRun?: RunRecord;
  summary?: FolderOperationalSummary;
  selected: boolean;
  sessionStartedAt: number;
  onSelect: () => void;
  onRun: () => void;
  onPreview: () => void;
  onHistory: () => void;
  onRemove: () => void;
  onLocate: () => void;
  onOpen: () => void;
  onReveal: () => void;
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
  const missing =
    summary?.availability !== undefined && summary.availability !== "available";
  return (
    <Paper
      aria-current={selected ? "true" : undefined}
      className={classes.folderCard}
      component="article"
      data-selected={selected || undefined}
      data-missing={missing || undefined}
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
        <Text c="dimmed" pl="calc(var(--folder-label-offset))" size="xs">
          {summary?.latest_run_at
            ? `${t("lastRun")} ${new Date(summary.latest_run_at).toLocaleString()}`
            : t("neverRun")}
        </Text>
        <Group gap={5} pl="calc(var(--folder-label-offset))">
          {summary?.latest_outcome ? (
            <Badge
              color={outcomeColor(summary.latest_outcome)}
              size="xs"
              variant="light"
            >
              {t(`outcome_${summary.latest_outcome}`)}
            </Badge>
          ) : null}
          {summary ? (
            <Badge
              color={changeColor(summary.change_state)}
              size="xs"
              variant="outline"
            >
              {t(`change_${summary.change_state}`)}
            </Badge>
          ) : null}
        </Group>
      </button>

      <Box className={classes.folderStatus}>
        <RunStatus state={aggregateState} />
      </Box>

      <Group className={classes.folderActions} gap={5} wrap="nowrap">
        <CardAction
          disabled={
            missing ||
            enabledActions === 0 ||
            activeRuns.some((run) => run.state === "stopping")
          }
          icon={<Play aria-hidden size={17} weight="fill" />}
          label={t("runFolder")}
          onClick={onRun}
        />
        <CardAction
          color="red"
          icon={<Trash aria-hidden size={17} />}
          label={t("removeFromFolders")}
          onClick={onRemove}
        />
        <Menu position="bottom-end" shadow="md" width={220}>
          <Menu.Target>
            <ActionIcon
              aria-label={t("moreActions")}
              size="lg"
              variant="default"
            >
              <DotsThree aria-hidden size={19} weight="bold" />
            </ActionIcon>
          </Menu.Target>
          <Menu.Dropdown>
            <Menu.Item
              disabled={missing}
              leftSection={<Eye aria-hidden size={16} />}
              onClick={onPreview}
            >
              {t("preview")}
            </Menu.Item>
            <Menu.Item
              leftSection={<ClockCounterClockwise aria-hidden size={16} />}
              onClick={onHistory}
            >
              {t("runHistory")}
            </Menu.Item>
            <Menu.Item
              disabled={missing}
              leftSection={<FolderOpen aria-hidden size={16} />}
              onClick={onOpen}
            >
              {t("openSource")}
            </Menu.Item>
            <Menu.Item
              disabled={!summary?.artifact_available}
              leftSection={<FolderOpen aria-hidden size={16} />}
              onClick={onReveal}
            >
              {t("revealLastArchive")}
            </Menu.Item>
            <Menu.Item
              leftSection={<FolderSimple aria-hidden size={16} />}
              onClick={onLocate}
            >
              {t("locateFolder")}
            </Menu.Item>
          </Menu.Dropdown>
        </Menu>
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
  runs,
  progressByRun,
  queuePositionByRun,
  onRunAll,
  onRunChanged,
  onPauseAll,
  onStopAll,
}: {
  counts: RunStateSummary;
  overall: number | null;
  hasPaused: boolean;
  hasStopping: boolean;
  hasStoppable: boolean;
  runs: RunRecord[];
  progressByRun: ReadonlyMap<string, ProgressSnapshot>;
  queuePositionByRun: ReadonlyMap<string, number>;
  onRunAll: () => void;
  onRunChanged: () => void;
  onPauseAll: () => void;
  onStopAll: () => void;
}) {
  const { t } = useI18n();
  const {
    cancelRecheckChanged,
    changeAssessmentProgress,
    command,
    recheckChanged,
  } = useDesktopData();
  const [activityOpened, activity] = useDisclosure(false);
  const rechecking =
    changeAssessmentProgress !== null && !changeAssessmentProgress.finished;
  const active = runs.filter((run) => run.state !== "queued");
  const waiting = runs.filter((run) => run.state === "queued");
  return (
    <>
      <footer className={classes.commandBar}>
        <Box className={classes.visuallyHidden}>
          <Text>{t("runningCount", { count: counts.running })}</Text>
          <Text>{t("queuedCount", { count: counts.queued })}</Text>
          <Text>{t("pausedCount", { count: counts.paused })}</Text>
          <Text>{t("overallProgress")}</Text>
        </Box>
        <Button
          leftSection={<ListBullets aria-hidden size={17} />}
          size="xs"
          variant="subtle"
          onClick={activity.open}
        >
          {t("activity")} · {active.length}/{waiting.length}
        </Button>
        <Box className={classes.overallProgress}>
          <Progress.Root radius="xl" size={6}>
            <Progress.Section
              aria-label={t("overallProgress")}
              animated={overall === null && counts.running > 0}
              value={overall ?? (counts.running > 0 ? 100 : 0)}
            />
          </Progress.Root>
        </Box>
        <Group gap="xs" wrap="nowrap">
          <Button
            disabled={hasStopping}
            leftSection={<Play aria-hidden size={17} weight="fill" />}
            onClick={onRunAll}
            size="xs"
          >
            {t("runAllEnabledActions")}
          </Button>
          <Button size="xs" variant="light" onClick={onRunChanged}>
            {t("runChanged")}
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
            size="xs"
          >
            {hasPaused ? t("resumeAll") : t("pauseAll")}
          </Button>
          <Button
            color="red"
            disabled={!hasStoppable}
            leftSection={<Stop aria-hidden size={17} weight="fill" />}
            variant="outline"
            onClick={onStopAll}
            size="xs"
          >
            {t("stopAll")}
          </Button>
        </Group>
      </footer>
      <Drawer
        closeButtonProps={{ "aria-label": t("close") }}
        opened={activityOpened}
        position="left"
        size="min(440px, 100vw)"
        title={t("activity")}
        onClose={activity.close}
      >
        <Group align="flex-end" justify="space-between" mb="sm">
          <Box style={{ flex: 1 }}>
            {rechecking ? (
              <>
                <Text size="xs">
                  {t("recheckProgress", {
                    completed: Number(changeAssessmentProgress.completed),
                    total: Number(changeAssessmentProgress.total),
                  })}
                </Text>
                {changeAssessmentProgress.current_folder ? (
                  <Text c="dimmed" lineClamp={1} size="xs">
                    {changeAssessmentProgress.current_folder}
                  </Text>
                ) : null}
                <Progress.Root mt={4} radius="xl" size="xs">
                  <Progress.Section
                    value={
                      changeAssessmentProgress.total === 0n
                        ? 100
                        : (Number(changeAssessmentProgress.completed) /
                            Number(changeAssessmentProgress.total)) *
                          100
                    }
                  />
                </Progress.Root>
              </>
            ) : null}
          </Box>
          {rechecking ? (
            <Button
              color="red"
              size="xs"
              variant="subtle"
              onClick={() => void cancelRecheckChanged()}
            >
              {t("cancel")}
            </Button>
          ) : null}
          <Button
            disabled={rechecking}
            leftSection={<ArrowsClockwise aria-hidden size={15} />}
            size="xs"
            variant="default"
            onClick={() => void recheckChanged()}
          >
            {t("recheckChanged")}
          </Button>
        </Group>
        <ActivitySection
          emptyLabel={t("noActiveRuns")}
          progressByRun={progressByRun}
          queuePositionByRun={queuePositionByRun}
          runs={active}
          title={t("activeRuns")}
          onCommand={(name, runId) => void command(name, { runId })}
        />
        <ActivitySection
          emptyLabel={t("noWaitingRuns")}
          progressByRun={progressByRun}
          queuePositionByRun={queuePositionByRun}
          runs={waiting}
          title={t("waitingRuns")}
          onCommand={(name, runId) => void command(name, { runId })}
        />
      </Drawer>
    </>
  );
}

function ActivitySection({
  title,
  emptyLabel,
  runs,
  progressByRun,
  queuePositionByRun,
  onCommand,
}: {
  title: string;
  emptyLabel: string;
  runs: RunRecord[];
  progressByRun: ReadonlyMap<string, ProgressSnapshot>;
  queuePositionByRun: ReadonlyMap<string, number>;
  onCommand: (
    name: "pause_run" | "resume_run" | "stop_run",
    runId: string,
  ) => void;
}) {
  const { t } = useI18n();
  return (
    <Stack gap="xs" mb="lg">
      <Title order={3}>{title}</Title>
      {runs.length === 0 ? (
        <Text c="dimmed" size="sm">
          {emptyLabel}
        </Text>
      ) : null}
      {runs.map((run) => {
        const progress = progressByRun.get(run.run_id);
        return (
          <Paper key={run.run_id} p="sm" withBorder>
            <Group justify="space-between" wrap="nowrap">
              <Box miw={0}>
                <Text fw={650} lineClamp={1}>
                  {basename(run.snapshot.folder.source)}
                </Text>
                <Text c="dimmed" size="xs">
                  {run.snapshot.action.spec.action_type} ·{" "}
                  {run.state === "queued"
                    ? t("queuePosition", {
                        position: queuePositionByRun.get(run.run_id) ?? "—",
                      })
                    : run.state}
                  {progress?.current_path ? ` · ${progress.current_path}` : ""}
                  {progress?.total_bytes && Number(progress.total_bytes) > 0
                    ? ` · ${Math.min(100, Math.round((Number(progress.completed_bytes) / Number(progress.total_bytes)) * 100))}%`
                    : ""}
                </Text>
              </Box>
              <Group gap={4} wrap="nowrap">
                {run.state === "paused" ? (
                  <CardAction
                    icon={<Play aria-hidden size={15} />}
                    label={t("resume")}
                    onClick={() => onCommand("resume_run", run.run_id)}
                  />
                ) : run.state === "running" || run.state === "planning" ? (
                  <CardAction
                    icon={<Pause aria-hidden size={15} />}
                    label={t("pause")}
                    onClick={() => onCommand("pause_run", run.run_id)}
                  />
                ) : null}
                <CardAction
                  color="red"
                  disabled={run.state === "stopping"}
                  icon={<Stop aria-hidden size={15} />}
                  label={t("stop")}
                  onClick={() => onCommand("stop_run", run.run_id)}
                />
              </Group>
            </Group>
          </Paper>
        );
      })}
    </Stack>
  );
}

function outcomeColor(
  outcome: FolderOperationalSummary["latest_outcome"],
): string {
  if (outcome === "succeeded") return "green";
  if (outcome === "succeeded_with_warnings") return "yellow";
  if (outcome === "failed") return "red";
  return "gray";
}

function changeColor(state: FolderOperationalSummary["change_state"]): string {
  if (state === "changed" || state === "no_checkpoint") return "orange";
  if (state === "unchanged") return "green";
  return "gray";
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
