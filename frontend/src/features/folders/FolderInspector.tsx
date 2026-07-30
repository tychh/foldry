import {
  ActionIcon,
  Alert,
  Box,
  Button,
  Checkbox,
  Divider,
  Group,
  Modal,
  Paper,
  Progress,
  Select,
  Stack,
  Switch,
  Text,
  TextInput,
  Title,
  Tooltip,
} from "@mantine/core";
import { useDisclosure } from "@mantine/hooks";
import {
  ArrowDown,
  ArrowUp,
  DotsSixVertical,
  FolderSimple,
  FolderOpen,
  Play,
  Plus,
  Trash,
  WarningCircle,
} from "@phosphor-icons/react";
import { useState } from "react";

import type {
  Folder,
  FolderAction,
  ProgressSnapshot,
  RunRecord,
  StoredProfile,
} from "../../shared/contracts/generated";
import { useI18n } from "../../shared/i18n/I18nProvider";
import { useDesktopData } from "../../shared/ipc/DesktopDataProvider";
import { RunStatus } from "../../shared/ui/RunStatus";
import { basename } from "./folderModel";
import classes from "./FoldersWorkspace.module.css";

type FolderInspectorProps = {
  profiles: StoredProfile[];
  folder: Folder | null;
  activeRuns: RunRecord[];
  progressByRun: ReadonlyMap<string, ProgressSnapshot>;
  queuePositionByRun: ReadonlyMap<string, number>;
  onOpenActivity: (actionId: string, tab: "preview" | "history") => void;
  onBrowseOutput: (
    sourcePath: string,
    initialPath: string,
    onConfirm: (path: string) => void,
  ) => void;
};

export function FolderInspector({
  folder,
  profiles,
  activeRuns,
  progressByRun,
  queuePositionByRun,
  onOpenActivity,
  onBrowseOutput,
}: FolderInspectorProps) {
  const { t } = useI18n();
  const { command } = useDesktopData();
  const [addOpened, addModal] = useDisclosure(false);
  const [removeAction, setRemoveAction] = useState<FolderAction | null>(null);
  const [draggedActionId, setDraggedActionId] = useState<string | null>(null);

  if (!folder) {
    return (
      <aside className={classes.settingsPane}>
        <Text c="dimmed" size="sm">
          {t("selectFolder")}
        </Text>
      </aside>
    );
  }

  const archiveExists = folder.actions.some(
    (action) => action.spec.action_type === "archive",
  );
  const updateFolder = (next: Folder) =>
    void command<Folder>("update_folder", { folder: next });
  const updateAction = (action: FolderAction) =>
    void command<FolderAction>("update_action", {
      folderId: folder.id,
      action,
    });
  const reorder = (actionIds: string[]) =>
    void command("reorder_actions", { folderId: folder.id, actionIds });
  const move = (index: number, direction: -1 | 1) => {
    const target = index + direction;
    if (target < 0 || target >= folder.actions.length) return;
    const ids = folder.actions.map((action) => action.id);
    [ids[index], ids[target]] = [ids[target]!, ids[index]!];
    reorder(ids);
  };

  return (
    <aside className={classes.settingsPane}>
      <Stack gap="lg">
        <Box>
          <Group gap="xs" wrap="nowrap">
            <Box
              aria-hidden
              className={classes.folderHeadingIcon}
              data-enabled={folder.enabled}
            >
              <FolderSimple size={27} weight="duotone" />
            </Box>
            <Title order={2}>{basename(folder.source)}</Title>
            <Switch
              aria-label={t("includeInRunAll")}
              checked={folder.enabled}
              ml="xs"
              onChange={(event) =>
                updateFolder({
                  ...folder,
                  enabled: event.currentTarget.checked,
                })
              }
            />
          </Group>
          <Text c="dimmed" mt={3} title={folder.source} truncate>
            {folder.source}
          </Text>
        </Box>

        <Paper className={classes.folderSettingsCard} p="md" withBorder>
          <Select
            data={profileOptions(profiles, t("invalid"))}
            label={t("defaultIgnoreProfile")}
            value={folder.default_profile_id}
            onChange={(profileId) =>
              profileId &&
              updateFolder({ ...folder, default_profile_id: profileId })
            }
          />
        </Paper>

        <Group justify="space-between">
          <Title order={3}>{t("actions")}</Title>
          <Button
            leftSection={<Plus aria-hidden size={16} />}
            size="xs"
            variant="default"
            onClick={addModal.open}
          >
            {t("addAction")}
          </Button>
        </Group>

        {folder.actions.length ? (
          <Stack gap="md">
            {folder.actions.map((action, index) => (
              <ArchiveActionCard
                key={action.id}
                action={action}
                activeRun={activeRuns.find(
                  (run) => run.action_id === action.id,
                )}
                progress={progressByRun.get(
                  activeRuns.find((run) => run.action_id === action.id)
                    ?.run_id ?? "",
                )}
                queuePosition={queuePositionByRun.get(
                  activeRuns.find((run) => run.action_id === action.id)
                    ?.run_id ?? "",
                )}
                canReorder={folder.actions.length > 1}
                folder={folder}
                index={index}
                profiles={profiles}
                onDragStart={() => setDraggedActionId(action.id)}
                onDrop={() => {
                  if (!draggedActionId || draggedActionId === action.id) return;
                  const ids = folder.actions.map((candidate) => candidate.id);
                  const from = ids.indexOf(draggedActionId);
                  const to = ids.indexOf(action.id);
                  if (from < 0 || to < 0) return;
                  ids.splice(to, 0, ids.splice(from, 1)[0]!);
                  setDraggedActionId(null);
                  reorder(ids);
                }}
                onMoveDown={() => move(index, 1)}
                onMoveUp={() => move(index, -1)}
                onRemove={() => setRemoveAction(action)}
                onOpenActivity={onOpenActivity}
                onUpdate={updateAction}
                onBrowseOutput={onBrowseOutput}
              />
            ))}
          </Stack>
        ) : (
          <Paper className={classes.actionsEmpty} p="lg" withBorder>
            <Text c="dimmed" size="sm">
              {t("noActions")}
            </Text>
          </Paper>
        )}
      </Stack>

      <Modal
        centered
        opened={addOpened}
        title={t("addAction")}
        onClose={addModal.close}
      >
        <Paper className={classes.catalogItem} p="md" withBorder>
          <Group justify="space-between" wrap="nowrap">
            <Box>
              <Text fw={650}>{t("actionArchive")}</Text>
              <Text c="dimmed" mt={4} size="sm">
                {t("archiveActionDescription")}
              </Text>
            </Box>
            <Button
              disabled={archiveExists}
              onClick={async () => {
                const added = await command<FolderAction>("add_action", {
                  folderId: folder.id,
                  actionType: "archive",
                  enabled: false,
                  profileIdOverride: null,
                });
                if (added) addModal.close();
              }}
            >
              {archiveExists ? t("alreadyAdded") : t("add")}
            </Button>
          </Group>
        </Paper>
      </Modal>

      <Modal
        centered
        opened={removeAction !== null}
        title={t("removeAction")}
        onClose={() => setRemoveAction(null)}
      >
        <Alert
          color="red"
          icon={<WarningCircle aria-hidden size={19} />}
          title={t("removeAction")}
        >
          {t("removeActionHint")}
        </Alert>
        <Group justify="flex-end" mt="lg">
          <Button variant="default" onClick={() => setRemoveAction(null)}>
            {t("cancel")}
          </Button>
          <Button
            color="red"
            onClick={async () => {
              if (!removeAction) return;
              const removed = await command<boolean>("remove_action", {
                folderId: folder.id,
                actionId: removeAction.id,
              });
              if (removed) setRemoveAction(null);
            }}
          >
            {t("remove")}
          </Button>
        </Group>
      </Modal>
    </aside>
  );
}

function ArchiveActionCard({
  folder,
  action,
  profiles,
  activeRun,
  progress,
  queuePosition,
  index,
  canReorder,
  onUpdate,
  onRemove,
  onMoveUp,
  onMoveDown,
  onDragStart,
  onDrop,
  onOpenActivity,
  onBrowseOutput,
}: {
  folder: Folder;
  action: FolderAction;
  profiles: StoredProfile[];
  activeRun?: RunRecord;
  progress?: ProgressSnapshot;
  queuePosition?: number;
  index: number;
  canReorder: boolean;
  onUpdate: (action: FolderAction) => void;
  onRemove: () => void;
  onMoveUp: () => void;
  onMoveDown: () => void;
  onDragStart: () => void;
  onDrop: () => void;
  onOpenActivity: FolderInspectorProps["onOpenActivity"];
  onBrowseOutput: FolderInspectorProps["onBrowseOutput"];
}) {
  const { t } = useI18n();
  const { command } = useDesktopData();
  const archive = action.spec.archive;
  const [filename, setFilename] = useState(archive?.output.filename ?? "");
  const [customPath, setCustomPath] = useState(
    archive?.output.directory.mode === "custom"
      ? archive.output.directory.path
      : "",
  );
  const [helpOpened, helpModal] = useDisclosure(false);

  if (!archive) {
    return (
      <Paper className={classes.actionCard} p="md" withBorder>
        <Alert color="yellow">{t("unsupportedAction")}</Alert>
      </Paper>
    );
  }

  const outputMode = archive.output.directory.mode;
  const customError =
    outputMode === "custom" && pathInsideSource(customPath, folder.source)
      ? t("outputInsideSource")
      : null;
  const filenameError =
    filename.trim() === "" || unknownFilenameToken(filename)
      ? t("invalidFilenameTemplate")
      : null;
  const valid = !customError && !filenameError;
  const patchArchive = (
    patch: Partial<typeof archive>,
    outputPatch?: Partial<typeof archive.output>,
  ) =>
    onUpdate({
      ...action,
      spec: {
        ...action.spec,
        archive: {
          ...archive,
          ...patch,
          output: { ...archive.output, ...outputPatch },
        },
      },
    });
  const resolvedDirectory =
    outputMode === "parent" ? parentPath(folder.source) : customPath;
  const resolvedFilename = resolveFilenamePreview(
    filename,
    folder.source,
    archive.output.format,
  );

  return (
    <Paper
      className={classes.actionCard}
      draggable={canReorder}
      p="md"
      withBorder
      onDragOver={(event) => event.preventDefault()}
      onDragStart={onDragStart}
      onDrop={onDrop}
    >
      <Stack gap="md">
        <Group justify="space-between" wrap="nowrap">
          <Group gap="xs" wrap="nowrap">
            {canReorder ? (
              <Tooltip label={t("dragToReorder")}>
                <span className={classes.dragHandle}>
                  <DotsSixVertical aria-hidden size={20} />
                </span>
              </Tooltip>
            ) : null}
            <Group gap="xs" wrap="nowrap">
              <Box
                className={classes.actionNumber}
                data-enabled={action.enabled}
              >
                {String(index + 1).padStart(2, "0")}
              </Box>
              <Text fw={700}>{t("actionArchive")}</Text>
              <Switch
                aria-label={t("includeInRunAll")}
                checked={action.enabled}
                ml="xs"
                onChange={(event) =>
                  onUpdate({ ...action, enabled: event.currentTarget.checked })
                }
              />
              {activeRun ? (
                <Box ml="xs">
                  <RunStatus state={activeRun.state} />
                </Box>
              ) : null}
            </Group>
          </Group>
          <Group gap={4} wrap="nowrap">
            {canReorder ? (
              <>
                <Tooltip label={t("moveUp")}>
                  <Button
                    aria-label={t("moveUp")}
                    disabled={index === 0}
                    px={7}
                    size="compact-sm"
                    variant="subtle"
                    onClick={onMoveUp}
                  >
                    <ArrowUp aria-hidden size={15} />
                  </Button>
                </Tooltip>
                <Tooltip label={t("moveDown")}>
                  <Button
                    aria-label={t("moveDown")}
                    px={7}
                    size="compact-sm"
                    variant="subtle"
                    onClick={onMoveDown}
                  >
                    <ArrowDown aria-hidden size={15} />
                  </Button>
                </Tooltip>
              </>
            ) : null}
            <Tooltip label={t("archiveHelpOpen")}>
              <ActionIcon
                aria-label={t("archiveHelpOpen")}
                size="1.75rem"
                variant="default"
                onClick={helpModal.open}
              >
                ?
              </ActionIcon>
            </Tooltip>
            <Tooltip label={t("removeAction")}>
              <ActionIcon
                aria-label={t("removeAction")}
                color="red"
                size="1.75rem"
                variant="light"
                onClick={onRemove}
              >
                <Trash aria-hidden size={15} />
              </ActionIcon>
            </Tooltip>
          </Group>
        </Group>

        <Select
          clearable
          data={[
            { label: t("inheritProfile"), value: "__inherit__" },
            ...profileOptions(profiles, t("invalid")),
          ]}
          label={t("ignoreProfile")}
          value={action.profile_id_override ?? "__inherit__"}
          w="100%"
          onChange={(profileId) =>
            onUpdate({
              ...action,
              profile_id_override:
                !profileId || profileId === "__inherit__" ? null : profileId,
            })
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
            value={archive.output.format}
            onChange={(format) =>
              format &&
              patchArchive(
                {},
                { format: format as typeof archive.output.format },
              )
            }
          />
          <Select
            data={[
              { label: t("fast"), value: "fast" },
              { label: t("balanced"), value: "balanced" },
              { label: t("maximum"), value: "maximum" },
            ]}
            label={t("compression")}
            value={archive.output.compression}
            onChange={(compression) =>
              compression &&
              patchArchive(
                {},
                {
                  compression: compression as typeof archive.output.compression,
                },
              )
            }
          />
        </Group>

        <Select
          data={[
            { label: t("parentFolder"), value: "parent" },
            { label: t("customFolder"), value: "custom" },
          ]}
          label={t("outputDirectory")}
          value={outputMode}
          onChange={(mode) =>
            mode &&
            patchArchive(
              {},
              {
                directory:
                  mode === "parent"
                    ? { mode: "parent" }
                    : {
                        mode: "custom",
                        path: customPath || parentPath(folder.source),
                      },
              },
            )
          }
        />
        {outputMode === "custom" ? (
          <TextInput
            error={customError}
            label={t("customOutputPath")}
            rightSection={
              <Tooltip label={t("chooseFolder")}>
                <Button
                  aria-label={t("chooseFolder")}
                  px={7}
                  size="compact-sm"
                  variant="subtle"
                  onClick={() =>
                    onBrowseOutput(
                      folder.source,
                      customPath || parentPath(folder.source),
                      (selected) => {
                        setCustomPath(selected);
                        if (!pathInsideSource(selected, folder.source)) {
                          patchArchive(
                            {},
                            { directory: { mode: "custom", path: selected } },
                          );
                        }
                      },
                    )
                  }
                >
                  <FolderOpen aria-hidden size={16} />
                </Button>
              </Tooltip>
            }
            value={customPath}
            onBlur={() =>
              !customError &&
              customPath &&
              patchArchive(
                {},
                { directory: { mode: "custom", path: customPath } },
              )
            }
            onChange={(event) => setCustomPath(event.currentTarget.value)}
          />
        ) : (
          <TextInput
            readOnly
            label={t("resolvedOutputPath")}
            value={resolvedDirectory}
          />
        )}

        <TextInput
          error={filenameError}
          label={t("filenameTemplate")}
          value={filename}
          onBlur={() =>
            !filenameError && patchArchive({}, { filename: filename.trim() })
          }
          onChange={(event) => setFilename(event.currentTarget.value)}
        />
        <Text c="dimmed" size="xs">
          {t("filenamePreview")}: {resolvedFilename}
        </Text>

        <Group grow>
          <Select
            data={[
              { label: t("conflictSkip"), value: "skip" },
              { label: t("conflictOverwrite"), value: "overwrite" },
              { label: t("conflictIncrement"), value: "increment" },
            ]}
            label={t("conflictPolicy")}
            value={archive.output.conflict_policy}
            onChange={(policy) =>
              policy &&
              patchArchive(
                {},
                {
                  conflict_policy:
                    policy as typeof archive.output.conflict_policy,
                },
              )
            }
          />
          <Select
            data={[
              { label: t("unreadableFail"), value: "fail" },
              { label: t("unreadableSkip"), value: "warn_and_skip" },
            ]}
            label={t("unreadablePolicy")}
            value={archive.unreadable_policy}
            onChange={(policy) =>
              policy &&
              patchArchive({
                unreadable_policy: policy as typeof archive.unreadable_policy,
              })
            }
          />
        </Group>

        <Group>
          <Checkbox
            checked={archive.include_root}
            label={t("includeRoot")}
            onChange={(event) =>
              patchArchive({ include_root: event.currentTarget.checked })
            }
          />
          <Checkbox
            checked={archive.verification.mode === "full"}
            label={t("fullVerification")}
            onChange={(event) =>
              patchArchive({
                verification: {
                  ...archive.verification,
                  mode: event.currentTarget.checked ? "full" : "structural",
                },
              })
            }
          />
        </Group>
        <Divider />
        <Group grow>
          <Button
            variant="default"
            onClick={() => onOpenActivity(action.id, "preview")}
          >
            {t("preview")}
          </Button>
          <Button
            disabled={!valid || Boolean(activeRun)}
            leftSection={<Play aria-hidden size={17} weight="fill" />}
            onClick={() =>
              void command<RunRecord>("run_action", {
                folderId: folder.id,
                actionId: action.id,
              })
            }
          >
            {t("runAction")}
          </Button>
        </Group>

        {activeRun ? (
          <Paper p="sm" withBorder>
            <Group justify="space-between">
              <RunStatus state={activeRun.state} />
              <Text c="dimmed" size="xs">
                {queuePosition
                  ? t("queuePosition", { position: queuePosition })
                  : progress
                    ? t("runProgressPercent", {
                        percent: progressPercent(progress),
                      })
                    : t("waitingForProgress")}
              </Text>
            </Group>
            <Progress.Root mt="xs" size="sm">
              <Progress.Section
                aria-label={t("actionRunProgress")}
                animated={activeRun.state === "running"}
                value={progressPercent(progress)}
              />
            </Progress.Root>
            {progress?.current_path ? (
              <Text className={classes.path} c="dimmed" mt="xs" size="xs">
                {progress.current_path}
              </Text>
            ) : null}
            <Group grow mt="sm">
              <Button
                variant="default"
                onClick={() => onOpenActivity(action.id, "history")}
              >
                {t("openActiveRun")}
              </Button>
              {activeRun.state === "paused" ||
              activeRun.state === "planning" ||
              activeRun.state === "running" ? (
                <Button
                  variant="default"
                  onClick={() =>
                    void command(
                      activeRun.state === "paused" ? "resume_run" : "pause_run",
                      { runId: activeRun.run_id },
                    )
                  }
                >
                  {activeRun.state === "paused" ? t("resume") : t("pause")}
                </Button>
              ) : null}
              <Button
                color="red"
                disabled={activeRun.state === "stopping"}
                variant="light"
                onClick={() =>
                  void command("stop_run", { runId: activeRun.run_id })
                }
              >
                {t("stop")}
              </Button>
            </Group>
          </Paper>
        ) : null}
      </Stack>

      <Modal
        centered
        closeButtonProps={{ "aria-label": t("archiveHelpClose") }}
        opened={helpOpened}
        size="lg"
        title={t("archiveHelpTitle")}
        onClose={helpModal.close}
      >
        <Stack gap="md">
          <Text size="sm">{t("archiveHelpIntro")}</Text>

          <Box>
            <Text fw={650} mb={4} size="sm">
              {t("archiveHelpFlowTitle")}
            </Text>
            <Text c="dimmed" size="sm">
              {t("archiveHelpFlow")}
            </Text>
          </Box>

          <Divider />
          <Text fw={650} size="sm">
            {t("archiveHelpFormatsTitle")}
          </Text>
          <Stack gap="sm">
            <Box>
              <Text fw={650} size="sm">
                {t("zip")}
              </Text>
              <Text c="dimmed" size="sm">
                {t("archiveHelpZip")}
              </Text>
            </Box>
            <Box>
              <Text fw={650} size="sm">
                {t("tarGz")}
              </Text>
              <Text c="dimmed" size="sm">
                {t("archiveHelpTarGz")}
              </Text>
            </Box>
            <Box>
              <Text fw={650} size="sm">
                {t("tarZst")}
              </Text>
              <Text c="dimmed" size="sm">
                {t("archiveHelpTarZst")}
              </Text>
            </Box>
          </Stack>

          <Alert color="yellow" title={t("archiveHelpLimitsTitle")}>
            {t("archiveHelpLimits")}
          </Alert>

          <Group justify="flex-end">
            <Button variant="default" onClick={helpModal.close}>
              {t("close")}
            </Button>
          </Group>
        </Stack>
      </Modal>
    </Paper>
  );
}

function profileOptions(profiles: StoredProfile[], invalid: string) {
  return profiles.flatMap((profile) =>
    profile.id
      ? [
          {
            label: `${profile.name}${profile.valid ? "" : ` · ${invalid}`}`,
            value: profile.id,
            disabled: !profile.valid,
          },
        ]
      : [],
  );
}

function parentPath(path: string): string {
  const normalized = path.replace(/[\\/]+$/, "");
  const index = Math.max(
    normalized.lastIndexOf("/"),
    normalized.lastIndexOf("\\"),
  );
  return index <= 0 ? normalized.slice(0, 1) : normalized.slice(0, index);
}

function normalizedPath(path: string): string {
  return path.replaceAll("\\", "/").replace(/\/+$/, "").toLocaleLowerCase();
}

function pathInsideSource(path: string, source: string): boolean {
  const candidate = normalizedPath(path);
  const root = normalizedPath(source);
  return candidate === root || candidate.startsWith(`${root}/`);
}

function progressPercent(progress?: ProgressSnapshot): number {
  if (!progress) return 0;
  const completed = BigInt(progress.completed_bytes);
  const total = progress.total_bytes ? BigInt(progress.total_bytes) : 0n;
  if (total > 0n) {
    return Math.min(100, Number((completed * 100n) / total));
  }
  const completedEntries = BigInt(progress.completed_entries);
  const totalEntries = progress.total_entries
    ? BigInt(progress.total_entries)
    : 0n;
  return totalEntries > 0n
    ? Math.min(100, Number((completedEntries * 100n) / totalEntries))
    : 0;
}

function unknownFilenameToken(value: string): boolean {
  return [...value.matchAll(/\{([^{}]+)\}/g)].some(
    (match) => match[1] !== "folder" && match[1] !== "date",
  );
}

function resolveFilenamePreview(
  template: string,
  source: string,
  format: "zip" | "tar_gz" | "tar_zst",
): string {
  const date = new Intl.DateTimeFormat("en-CA", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  }).format(new Date());
  const name = template
    .replaceAll("{folder}", basename(source))
    .replaceAll("{date}", date);
  const extension =
    format === "zip" ? ".zip" : format === "tar_gz" ? ".tar.gz" : ".tar.zst";
  return name.toLocaleLowerCase().endsWith(extension)
    ? name
    : `${name}${extension}`;
}
