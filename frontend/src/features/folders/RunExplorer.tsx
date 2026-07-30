import {
  Alert,
  Badge,
  Box,
  Button,
  Code,
  Divider,
  Drawer,
  Group,
  Loader,
  Paper,
  SegmentedControl,
  Select,
  Stack,
  Tabs,
  Text,
  TextInput,
  Title,
} from "@mantine/core";
import {
  ArrowClockwise,
  Export,
  FolderOpen,
  ListMagnifyingGlass,
  Play,
} from "@phosphor-icons/react";
import {
  type UIEvent,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";

import type {
  LogRecord,
  PreviewEntry,
  PreviewFilter,
  PreviewPage,
  PreviewStarted,
  RunRecord,
  Folder,
  FolderAction,
} from "../../shared/contracts/generated";
import { useI18n } from "../../shared/i18n/I18nProvider";
import { useDesktopData } from "../../shared/ipc/DesktopDataProvider";
import { RunStatus } from "../../shared/ui/RunStatus";
import classes from "./RunExplorer.module.css";
import { basename } from "./folderModel";
import { formatBytes, formatDuration } from "./runFormatting";

const PREVIEW_PAGE_SIZE = 200;
const HISTORY_PAGE_SIZE = 50;
const LOG_PAGE_SIZE = 200;
const PREVIEW_ROW_HEIGHT = 58;
const LOG_ROW_HEIGHT = 58;

type ExplorerTab = "preview" | "history";

export function RunExplorer({
  opened,
  folder,
  initialTab,
  initialActionId,
  onClose,
}: {
  opened: boolean;
  folder: Folder | null;
  initialTab: ExplorerTab;
  initialActionId?: string | null;
  onClose: () => void;
}) {
  const { t } = useI18n();
  const [tab, setTab] = useState<ExplorerTab>(initialTab);

  return (
    <Drawer
      opened={opened}
      onClose={onClose}
      position="right"
      size="min(96vw, 760px)"
      closeButtonProps={{ "aria-label": t("close") }}
      title={
        folder
          ? t("folderActivityTitle", { name: basename(folder.source) })
          : undefined
      }
    >
      {folder ? (
        <Tabs
          keepMounted={false}
          value={tab}
          onChange={(value) => setTab((value as ExplorerTab) ?? "preview")}
        >
          <Tabs.List grow>
            <Tabs.Tab value="preview">{t("preview")}</Tabs.Tab>
            <Tabs.Tab value="history">{t("runHistory")}</Tabs.Tab>
          </Tabs.List>
          <Tabs.Panel pt="md" value="preview">
            <PreviewPanel folder={folder} initialActionId={initialActionId} />
          </Tabs.Panel>
          <Tabs.Panel pt="md" value="history">
            <HistoryPanel folder={folder} initialActionId={initialActionId} />
          </Tabs.Panel>
        </Tabs>
      ) : null}
    </Drawer>
  );
}

function PreviewPanel({
  folder,
  initialActionId,
}: {
  folder: Folder;
  initialActionId?: string | null;
}) {
  const { locale, t } = useI18n();
  const { query } = useDesktopData();
  const actions = useMemo(
    () =>
      folder.actions.filter(
        (action) =>
          action.spec.action_type === "archive" && action.spec.archive !== null,
      ),
    [folder.actions],
  );
  const [actionId, setActionId] = useState<string | null>(() =>
    actions.some((action) => action.id === initialActionId)
      ? (initialActionId ?? null)
      : (actions[0]?.id ?? null),
  );
  const [started, setStarted] = useState<PreviewStarted | null>(null);
  const [entries, setEntries] = useState<PreviewEntry[]>([]);
  const [cursor, setCursor] = useState<string | null>(null);
  const [filter, setFilter] = useState<PreviewFilter>("all");
  const filterRef = useRef<PreviewFilter>("all");
  const pageRequestRef = useRef(0);
  const [search, setSearch] = useState("");
  const [scrollTop, setScrollTop] = useState(0);
  const [loading, setLoading] = useState(true);
  const [loadingPage, setLoadingPage] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const actionOptions = useMemo(
    () =>
      actions.map((action) => ({
        value: action.id,
        label: actionLabel(folder, action.id, t),
      })),
    [actions, folder, t],
  );
  const selectedAction = actions.find((action) => action.id === actionId);
  const previewKey = JSON.stringify({
    folderId: folder.id,
    source: folder.source,
    defaultProfileId: folder.default_profile_id,
    action: selectedAction,
  });

  useEffect(() => {
    if (
      initialActionId &&
      actions.some((action) => action.id === initialActionId)
    ) {
      queueMicrotask(() => setActionId(initialActionId));
    } else if (!actions.some((action) => action.id === actionId)) {
      queueMicrotask(() => setActionId(actions[0]?.id ?? null));
    }
  }, [actionId, actions, initialActionId]);

  const loadPage = useCallback(
    async (
      nextCursor: string | null,
      nextFilter: PreviewFilter,
      append: boolean,
    ) => {
      const request = ++pageRequestRef.current;
      setLoadingPage(true);
      try {
        const page = await query<PreviewPage>("preview_page", {
          folderId: folder.id,
          actionId,
          cursor: nextCursor,
          limit: PREVIEW_PAGE_SIZE,
          filter: nextFilter,
        });
        if (request !== pageRequestRef.current) return;
        setEntries((current) =>
          append ? [...current, ...page.entries] : page.entries,
        );
        setCursor(page.next_cursor);
        setError(null);
      } catch (caught) {
        if (request !== pageRequestRef.current) return;
        setError(errorMessage(caught));
      } finally {
        if (request === pageRequestRef.current) {
          setLoadingPage(false);
        }
      }
    },
    [actionId, query, folder.id],
  );

  useEffect(() => {
    if (!actionId) {
      queueMicrotask(() => {
        setError("This folder has no preview-capable action.");
        setLoading(false);
      });
      return;
    }
    let active = true;
    pageRequestRef.current += 1;
    queueMicrotask(() => {
      if (!active) return;
      setStarted(null);
      setEntries([]);
      setCursor(null);
      setScrollTop(0);
      setError(null);
      setLoading(true);
    });
    void query<PreviewStarted>("start_preview", {
      folderId: folder.id,
      actionId,
    }).then(
      async (result) => {
        if (!active) return;
        setStarted(result);
        await loadPage(null, filterRef.current, false);
        if (active) setLoading(false);
      },
      (caught: unknown) => {
        if (active) {
          setError(errorMessage(caught));
          setLoading(false);
        }
      },
    );
    return () => {
      active = false;
      void query<boolean>("cancel_preview", {
        folderId: folder.id,
        actionId,
      });
    };
    // previewKey intentionally captures every setting that invalidates a preview.
  }, [actionId, loadPage, previewKey, query, folder.id]);

  const visibleEntries = useMemo(() => {
    const needle = search.trim().toLocaleLowerCase(locale);
    return needle
      ? entries.filter(
          (entry) =>
            entry.relative_path.toLocaleLowerCase(locale).includes(needle) ||
            entry.reason?.original_rule
              .toLocaleLowerCase(locale)
              .includes(needle),
        )
      : entries;
  }, [entries, locale, search]);
  const first = Math.max(0, Math.floor(scrollTop / PREVIEW_ROW_HEIGHT) - 4);
  const last = Math.min(visibleEntries.length, first + 16);
  const windowed = visibleEntries.slice(first, last);

  return (
    <Stack gap="md">
      {actionOptions.length ? (
        <Select
          data={actionOptions}
          label={t("previewAction")}
          value={actionId}
          onChange={setActionId}
        />
      ) : null}
      {loading ? <LoadingBlock label={t("buildingPreview")} /> : null}
      {error && !started ? (
        <Alert color="red" title={t("previewFailed")}>
          {error}
        </Alert>
      ) : null}
      {started ? (
        <Paper className={classes.previewSummary} p="md" withBorder>
          <Group justify="space-between" wrap="wrap">
            <Box>
              <Text fw={650} mb={4}>
                {t("previewReady")}
              </Text>
              <Text c="dimmed" size="xs">
                {formatDate(started.snapshot.created_at, locale)}
              </Text>
              <Text c="dimmed" mt={4} size="xs">
                {actionLabel(folder, started.action.id, t)} ·{" "}
                {t("effectiveProfile")}: {started.effective_profile_name}
              </Text>
            </Box>
            <Group gap="lg">
              <Metric
                label={t("included")}
                value={started.snapshot.summary.included_entries}
              />
              <Metric
                label={t("excluded")}
                value={started.snapshot.summary.excluded_entries}
              />
              <Metric
                label={t("skipped")}
                value={started.snapshot.summary.skipped_entries}
              />
            </Group>
          </Group>
          <div className={classes.previewSizeGrid}>
            <Metric
              label={t("rawSize")}
              value={formatBytes(started.raw_bytes)}
            />
            <Metric
              label={t("includedSize")}
              value={formatBytes(started.snapshot.summary.included_bytes)}
            />
            <Metric
              label={t("excludedSize")}
              value={
                started.raw_bytes_partial
                  ? "—"
                  : formatBytes(
                      subtractBytes(
                        started.raw_bytes,
                        started.snapshot.summary.included_bytes,
                      ),
                    )
              }
            />
            <Metric
              label={t("includedShare")}
              value={
                started.raw_bytes_partial
                  ? t("partialCalculation")
                  : bytePercentage(
                      started.snapshot.summary.included_bytes,
                      started.raw_bytes,
                      locale,
                    )
              }
            />
          </div>
          {started.raw_bytes_partial ? (
            <Alert color="yellow" mt="sm">
              {t("rawSizePartial", {
                count: Number(started.raw_bytes_warnings),
              })}
            </Alert>
          ) : null}
          <Text c="dimmed" mt="sm" size="xs">
            {t("profileHash")}:{" "}
            <Code>{shortHash(started.snapshot.profile_hash)}</Code>
          </Text>
        </Paper>
      ) : null}
      <Group align="end" wrap="wrap">
        <SegmentedControl
          data={[
            { label: t("all"), value: "all" },
            { label: t("included"), value: "included" },
            { label: t("excluded"), value: "excluded" },
            { label: t("skipped"), value: "skipped" },
          ]}
          value={filter}
          onChange={(value) => {
            const nextFilter = value as PreviewFilter;
            filterRef.current = nextFilter;
            setFilter(nextFilter);
            setScrollTop(0);
            if (started) {
              void loadPage(null, nextFilter, false);
            }
          }}
        />
        <TextInput
          className={classes.search}
          leftSection={<ListMagnifyingGlass aria-hidden size={17} />}
          placeholder={t("searchLoadedEntries")}
          value={search}
          onChange={(event) => {
            setSearch(event.currentTarget.value);
            setScrollTop(0);
          }}
        />
      </Group>
      {error ? <Alert color="red">{error}</Alert> : null}
      <div
        aria-label={t("previewEntries")}
        className={classes.virtualViewport}
        role="list"
        tabIndex={0}
        onScroll={(event: UIEvent<HTMLDivElement>) =>
          setScrollTop(event.currentTarget.scrollTop)
        }
      >
        <div
          className={classes.virtualCanvas}
          style={{ height: visibleEntries.length * PREVIEW_ROW_HEIGHT }}
        >
          {windowed.map((entry, index) => {
            const absoluteIndex = first + index;
            return (
              <PreviewRow
                key={`${entry.relative_path}-${absoluteIndex}`}
                entry={entry}
                top={absoluteIndex * PREVIEW_ROW_HEIGHT}
              />
            );
          })}
        </div>
      </div>
      <Group justify="space-between">
        <Text c="dimmed" size="xs">
          {t("loadedEntries", {
            count: entries.length,
            visible: visibleEntries.length,
          })}
        </Text>
        {cursor ? (
          <Button
            loading={loadingPage}
            variant="default"
            onClick={() => void loadPage(cursor, filter, true)}
          >
            {t("loadMore")}
          </Button>
        ) : null}
      </Group>
    </Stack>
  );
}

function PreviewRow({ entry, top }: { entry: PreviewEntry; top: number }) {
  const { t } = useI18n();
  const reason = entry.reason
    ? t("matchedRule", {
        line: entry.reason.line,
        rule: entry.reason.original_rule,
      })
    : entry.disposition === "included"
      ? t("includedByDefault")
      : t("filesystemDecision");
  return (
    <div
      className={classes.previewRow}
      data-disposition={entry.disposition}
      role="listitem"
      style={{ transform: `translateY(${top}px)` }}
    >
      <Badge
        color={dispositionColor(entry.disposition)}
        size="sm"
        variant="light"
      >
        {t(entry.disposition)}
      </Badge>
      <Box miw={0}>
        <Text className={classes.entryPath} fw={550} size="sm">
          {entry.relative_path}
        </Text>
        <Text c="dimmed" size="xs">
          {reason}
          {entry.reason?.preset_id
            ? ` · ${t("preset")}: ${entry.reason.preset_id}`
            : ""}
        </Text>
      </Box>
      <Text c="dimmed" size="xs">
        {formatBytes(entry.size)}
      </Text>
    </div>
  );
}

function HistoryPanel({
  folder,
  initialActionId,
}: {
  folder: Folder;
  initialActionId?: string | null;
}) {
  const { t } = useI18n();
  const { query } = useDesktopData();
  const [actionFilter, setActionFilter] = useState(
    folder.actions.some((action) => action.id === initialActionId)
      ? (initialActionId ?? "all")
      : "all",
  );
  const [runs, setRuns] = useState<RunRecord[]>([]);
  const [offset, setOffset] = useState(0);
  const [hasMore, setHasMore] = useState(true);
  const [loading, setLoading] = useState(true);
  const [selected, setSelected] = useState<RunRecord | null>(null);
  const [error, setError] = useState<string | null>(null);
  const historyRequestRef = useRef(0);

  const loadHistory = useCallback(
    async (nextOffset: number, append: boolean) => {
      const request = ++historyRequestRef.current;
      setLoading(true);
      try {
        const page = await query<RunRecord[]>("history_page", {
          offset: nextOffset,
          limit: HISTORY_PAGE_SIZE,
          folderId: folder.id,
          actionId: actionFilter === "all" ? null : actionFilter,
        });
        if (request !== historyRequestRef.current) return;
        setRuns((current) => (append ? [...current, ...page] : page));
        setOffset(nextOffset + page.length);
        setHasMore(page.length === HISTORY_PAGE_SIZE);
        setError(null);
      } catch (caught) {
        if (request !== historyRequestRef.current) return;
        setError(errorMessage(caught));
      } finally {
        if (request === historyRequestRef.current) {
          setLoading(false);
        }
      }
    },
    [actionFilter, query, folder.id],
  );

  useEffect(() => {
    let active = true;
    historyRequestRef.current += 1;
    queueMicrotask(() => {
      if (!active) return;
      setRuns([]);
      setSelected(null);
      setLoading(true);
    });
    void query<RunRecord[]>("history_page", {
      offset: 0,
      limit: HISTORY_PAGE_SIZE,
      folderId: folder.id,
      actionId: actionFilter === "all" ? null : actionFilter,
    }).then(
      (page) => {
        if (!active) return;
        setRuns(page);
        setOffset(page.length);
        setHasMore(page.length === HISTORY_PAGE_SIZE);
        setError(null);
        setLoading(false);
      },
      (caught: unknown) => {
        if (!active) return;
        setError(errorMessage(caught));
        setLoading(false);
      },
    );
    return () => {
      active = false;
    };
  }, [actionFilter, query, folder.id]);

  if (loading && runs.length === 0) {
    return <LoadingBlock label={t("loadingHistory")} />;
  }

  return (
    <Stack gap="md">
      <Select
        data={[
          { value: "all", label: t("allActions") },
          ...folder.actions.map((action) => ({
            value: action.id,
            label: actionLabel(folder, action.id, t),
          })),
        ]}
        label={t("historyAction")}
        value={actionFilter}
        onChange={(value) => setActionFilter(value ?? "all")}
      />
      {error ? <Alert color="red">{error}</Alert> : null}
      {runs.length === 0 ? (
        <Paper p="xl" ta="center" withBorder>
          <Text fw={650}>{t("noRunHistory")}</Text>
          <Text c="dimmed" mt="xs" size="sm">
            {t("noRunHistoryHint")}
          </Text>
        </Paper>
      ) : (
        <Stack gap="xs">
          {runs.map((run) => (
            <button
              key={run.run_id}
              aria-pressed={selected?.run_id === run.run_id}
              className={classes.historyRow}
              type="button"
              onClick={() => void selectRun(run, query, setSelected, setError)}
            >
              <RunStatus state={run.state} />
              <Box miw={0}>
                <Text fw={600} size="sm">
                  {historyActionLabel(folder, run, t)}
                </Text>
                <Text c="dimmed" size="xs">
                  {new Date(run.started_at).toLocaleString()}
                </Text>
              </Box>
              <Text c="dimmed" size="xs">
                {run.summary
                  ? formatDuration(run.summary.duration_ms)
                  : t("runInProgress")}
              </Text>
            </button>
          ))}
        </Stack>
      )}
      {hasMore ? (
        <Button
          loading={loading}
          variant="default"
          onClick={() => void loadHistory(offset, true)}
        >
          {t("loadOlderRuns")}
        </Button>
      ) : null}
      {selected ? (
        <>
          <Divider />
          <RunDetails key={selected.run_id} folder={folder} run={selected} />
        </>
      ) : null}
    </Stack>
  );
}

async function selectRun(
  run: RunRecord,
  query: ReturnType<typeof useDesktopData>["query"],
  setSelected: (run: RunRecord | null) => void,
  setError: (message: string | null) => void,
) {
  try {
    const details = await query<RunRecord | null>("run_details", {
      runId: run.run_id,
    });
    setSelected(details);
    setError(null);
  } catch (caught) {
    setError(errorMessage(caught));
  }
}

function RunDetails({ run, folder }: { run: RunRecord; folder: Folder }) {
  const { locale, t } = useI18n();
  const { command, query } = useDesktopData();
  const [logsOpened, setLogsOpened] = useState(false);
  const [logs, setLogs] = useState<LogRecord[]>([]);
  const [hasMoreLogs, setHasMoreLogs] = useState(true);
  const [loadingLogs, setLoadingLogs] = useState(false);
  const [logScrollTop, setLogScrollTop] = useState(0);
  const [exportedPath, setExportedPath] = useState<string | null>(null);
  const summary = run.summary;
  const currentAction =
    folder.listed &&
    folder.actions.find((action) => action.id === run.action_id);
  const firstLog = Math.max(0, Math.floor(logScrollTop / LOG_ROW_HEIGHT) - 3);
  const visibleLogs = logs.slice(firstLog, firstLog + 10);

  const loadLogs = async () => {
    setLoadingLogs(true);
    try {
      const page = await query<LogRecord[]>("logs_page", {
        runId: run.run_id,
        offset: logs.length,
        limit: LOG_PAGE_SIZE,
      });
      setLogs((current) => [...current, ...page]);
      setHasMoreLogs(page.length === LOG_PAGE_SIZE);
      setLogsOpened(true);
    } finally {
      setLoadingLogs(false);
    }
  };

  return (
    <Stack gap="md">
      <Group justify="space-between">
        <Box>
          <Title order={3}>{t("runResult")}</Title>
          <Text c="dimmed" size="xs">
            {formatDate(run.started_at, locale)}
          </Text>
        </Box>
        <RunStatus state={run.state} />
      </Group>
      <Paper p="sm" withBorder>
        <div className={classes.snapshotGrid}>
          <Metric
            label={t("snapshotAction")}
            value={actionTypeLabel(run.snapshot.action, t)}
          />
          <Metric
            label={t("snapshotProfile")}
            value={run.snapshot.effective_profile_id}
          />
          <Metric
            label={t("snapshotSource")}
            value={run.snapshot.folder.source}
          />
        </div>
      </Paper>
      {summary ? (
        <>
          <div className={classes.resultGrid}>
            <Metric
              label={t("duration")}
              value={formatDuration(summary.duration_ms)}
            />
            <Metric
              label={t("filesProcessed")}
              value={summary.included_entries}
            />
            <Metric
              label={t("sourceSize")}
              value={formatBytes(summary.source_bytes)}
            />
            <Metric
              label={t("archiveSize")}
              value={
                summary.artifact
                  ? formatBytes(summary.artifact.size_bytes)
                  : "—"
              }
            />
          </div>
          {summary.artifact ? (
            <Paper p="sm" withBorder>
              <Text c="dimmed" size="xs">
                {t("outputFile")}
              </Text>
              <Text className={classes.outputPath} mt={4} size="sm">
                {summary.artifact.path}
              </Text>
            </Paper>
          ) : null}
          {summary.warnings.map((warning, index) => (
            <Alert key={`${warning.code}-${index}`} color="yellow">
              {warning.message}
              {warning.path ? ` · ${warning.path}` : ""}
            </Alert>
          ))}
          {summary.error ? (
            <Alert color="red" title={t("runError")}>
              {summary.error.message}
              {summary.error.path ? ` · ${summary.error.path}` : ""}
            </Alert>
          ) : null}
        </>
      ) : (
        <Text c="dimmed">{t("runInProgress")}</Text>
      )}
      <Group wrap="wrap">
        {summary?.artifact ? (
          <Button
            leftSection={<FolderOpen aria-hidden size={17} />}
            variant="default"
            onClick={() =>
              void command("reveal_run_output", { runId: run.run_id })
            }
          >
            {t("revealOutput")}
          </Button>
        ) : null}
        <Button
          leftSection={<ArrowClockwise aria-hidden size={17} />}
          variant="default"
          onClick={() => void command("repeat_run", { runId: run.run_id })}
        >
          {t("repeatSnapshot")}
        </Button>
        <Button
          disabled={!currentAction}
          leftSection={<Play aria-hidden size={17} weight="fill" />}
          title={!currentAction ? t("currentActionUnavailable") : undefined}
          onClick={() =>
            void command("run_action", {
              folderId: run.folder_id,
              actionId: run.action_id,
            })
          }
        >
          {t("runCurrent")}
        </Button>
      </Group>
      {!currentAction ? (
        <Text c="dimmed" size="xs">
          {t("currentActionUnavailable")}
        </Text>
      ) : null}
      <Divider />
      <Group justify="space-between">
        <Title order={4}>{t("logs")}</Title>
        <Group gap="xs">
          <Button
            leftSection={<Export aria-hidden size={16} />}
            size="xs"
            variant="default"
            onClick={() =>
              void query<string | null>("export_run_logs", {
                runId: run.run_id,
              }).then((path) => setExportedPath(path))
            }
          >
            {t("exportLogs")}
          </Button>
          {!logsOpened ? (
            <Button
              loading={loadingLogs}
              size="xs"
              onClick={() => void loadLogs()}
            >
              {t("showLogs")}
            </Button>
          ) : null}
        </Group>
      </Group>
      {exportedPath ? (
        <Text c="dimmed" size="xs">
          {t("logsExported", { path: exportedPath })}
        </Text>
      ) : null}
      {logsOpened ? (
        <>
          <div
            aria-label={t("logs")}
            className={classes.logs}
            role="log"
            tabIndex={0}
            onScroll={(event: UIEvent<HTMLDivElement>) =>
              setLogScrollTop(event.currentTarget.scrollTop)
            }
          >
            <div
              className={classes.logCanvas}
              style={{ height: logs.length * LOG_ROW_HEIGHT }}
            >
              {visibleLogs.map((log, index) => (
                <div
                  className={classes.logRow}
                  key={log.sequence}
                  style={{
                    transform: `translateY(${(firstLog + index) * LOG_ROW_HEIGHT}px)`,
                  }}
                >
                  <Text c="dimmed" ff="monospace" size="xs">
                    {log.sequence}
                  </Text>
                  <Badge
                    color={
                      log.level === "error"
                        ? "red"
                        : log.level === "warn"
                          ? "yellow"
                          : "gray"
                    }
                    size="xs"
                    variant="light"
                  >
                    {log.level}
                  </Badge>
                  <Box miw={0}>
                    <Text size="xs">{log.message}</Text>
                    {log.path ? (
                      <Text c="dimmed" ff="monospace" size="xs">
                        {log.path}
                      </Text>
                    ) : null}
                  </Box>
                </div>
              ))}
            </div>
          </div>
          {hasMoreLogs ? (
            <Button
              loading={loadingLogs}
              size="xs"
              variant="default"
              onClick={() => void loadLogs()}
            >
              {t("loadMoreLogs")}
            </Button>
          ) : null}
        </>
      ) : (
        <Text c="dimmed" size="sm">
          {t("logsLazyHint")}
        </Text>
      )}
    </Stack>
  );
}

function LoadingBlock({ label }: { label: string }) {
  return (
    <Group justify="center" p="xl">
      <Loader size="sm" />
      <Text c="dimmed">{label}</Text>
    </Group>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <Box>
      <Text c="dimmed" size="xs">
        {label}
      </Text>
      <Text fw={650} mt={2} size="sm">
        {value}
      </Text>
    </Box>
  );
}

function formatDate(value: string, locale: string): string {
  return new Intl.DateTimeFormat(locale, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(new Date(value));
}

function shortHash(hash: string): string {
  return `${hash.slice(0, 12)}…${hash.slice(-8)}`;
}

type Translator = ReturnType<typeof useI18n>["t"];

function actionLabel(folder: Folder, actionId: string, t: Translator): string {
  const index = folder.actions.findIndex((action) => action.id === actionId);
  const action = folder.actions[index];
  return action
    ? `${actionTypeLabel(action, t)} · ${t("actionNumber", { number: index + 1 })}`
    : t("historicalAction");
}

function historyActionLabel(
  folder: Folder,
  run: RunRecord,
  t: Translator,
): string {
  const index = folder.actions.findIndex(
    (action) => action.id === run.action_id,
  );
  return index >= 0
    ? `${actionTypeLabel(run.snapshot.action, t)} · ${t("actionNumber", { number: index + 1 })}`
    : `${actionTypeLabel(run.snapshot.action, t)} · ${t("historicalAction")}`;
}

function actionTypeLabel(action: FolderAction, t: Translator): string {
  return action.spec.action_type === "archive"
    ? t("actionArchive")
    : action.spec.action_type;
}

function subtractBytes(raw: string, included: string): string {
  try {
    const difference = BigInt(raw) - BigInt(included);
    return (difference > 0n ? difference : 0n).toString();
  } catch {
    return "0";
  }
}

function bytePercentage(included: string, raw: string, locale: string): string {
  try {
    const total = BigInt(raw);
    if (total === 0n) return "—";
    const tenths = (BigInt(included) * 1000n) / total;
    return `${new Intl.NumberFormat(locale, {
      minimumFractionDigits: 1,
      maximumFractionDigits: 1,
    }).format(Number(tenths) / 10)}%`;
  } catch {
    return "—";
  }
}

function dispositionColor(disposition: PreviewEntry["disposition"]): string {
  return disposition === "included"
    ? "green"
    : disposition === "excluded"
      ? "gray"
      : "yellow";
}

function errorMessage(error: unknown): string {
  if (typeof error === "object" && error && "message" in error) {
    return String(error.message);
  }
  return String(error);
}
