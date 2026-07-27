import {
  Alert,
  Box,
  Button,
  Checkbox,
  Divider,
  Group,
  Paper,
  Select,
  Stack,
  Switch,
  Text,
  TextInput,
  Title,
} from "@mantine/core";
import { Play, Trash, WarningCircle } from "@phosphor-icons/react";
import { useState } from "react";

import type { StoredProfile, Task } from "../../shared/contracts/generated";
import { useI18n } from "../../shared/i18n/I18nProvider";
import { basename, updateArchive } from "./taskModel";
import classes from "./TasksWorkspace.module.css";

type TaskInspectorProps = {
  profiles: StoredProfile[];
  task: Task | null;
  unframed?: boolean;
  onRemove: (task: Task) => Promise<boolean>;
  onRun: (task: Task) => void;
  onUpdate: (task: Task) => Promise<boolean>;
};

export function TaskInspector({
  task,
  profiles,
  unframed = false,
  onRemove,
  onRun,
  onUpdate,
}: TaskInspectorProps) {
  const { t } = useI18n();
  const archive = task?.steps[0]?.archive;
  const [outputDirectory, setOutputDirectory] = useState(
    archive?.output.directory ?? "",
  );
  const [outputFilename, setOutputFilename] = useState(
    archive?.output.filename ?? "",
  );
  const [status, setStatus] = useState<"idle" | "saving" | "error">("idle");
  const [confirmRemove, setConfirmRemove] = useState(false);

  const save = async (next: Task) => {
    setStatus("saving");
    const saved = await onUpdate(next);
    setStatus(saved ? "idle" : "error");
  };

  const saveOutputText = () => {
    if (!task || !archive) {
      return;
    }
    if (
      outputDirectory === archive.output.directory &&
      outputFilename === archive.output.filename
    ) {
      return;
    }
    void save(
      updateArchive(task, (value) => ({
        ...value,
        output: {
          ...value.output,
          directory: outputDirectory,
          filename: outputFilename,
        },
      })),
    );
  };

  return (
    <aside
      className={unframed ? classes.unframedPanel : classes.inspectorPanel}
    >
      {!unframed ? <Title order={2}>{t("taskSettings")}</Title> : null}
      {task && archive ? (
        <Stack gap="md">
          <Box>
            <Text c="dimmed" size="xs">
              {t("actionArchive")}
            </Text>
            <Text fw={650} mt={3}>
              {basename(task.source)}
            </Text>
            <Text className={classes.path} c="dimmed" mt={3} size="xs">
              {task.source}
            </Text>
          </Box>
          <Switch
            checked={task.enabled}
            label={t("enabled")}
            onChange={(event) =>
              void save({ ...task, enabled: event.currentTarget.checked })
            }
          />
          <Select
            data={profiles
              .filter((profile) => profile.id)
              .map((profile) => ({
                label: `${profile.name}${profile.valid ? "" : ` · ${t("invalid")}`}`,
                value: profile.id!,
                disabled: !profile.valid,
              }))}
            label={t("profile")}
            value={task.profile_id}
            onChange={(profileId) => {
              if (profileId) {
                void save({ ...task, profile_id: profileId });
              }
            }}
          />
          <Paper className={classes.actionStep} p="sm" withBorder>
            <Group justify="space-between">
              <Text fw={650} size="sm">
                1. {t("actionArchive")}
              </Text>
              <Text c="dimmed" size="xs">
                v{archive.version}
              </Text>
            </Group>
          </Paper>
          <Select
            data={[
              { label: t("zip"), value: "zip" },
              { label: t("tarGz"), value: "tar_gz" },
              { label: t("tarZst"), value: "tar_zst" },
            ]}
            label={t("format")}
            value={archive.output.format}
            onChange={(format) => {
              if (format) {
                void save(
                  updateArchive(task, (value) => ({
                    ...value,
                    output: {
                      ...value.output,
                      format: format as typeof value.output.format,
                    },
                  })),
                );
              }
            }}
          />
          <Select
            data={[
              { label: t("fast"), value: "fast" },
              { label: t("balanced"), value: "balanced" },
              { label: t("maximum"), value: "maximum" },
            ]}
            label={t("compression")}
            value={archive.output.compression}
            onChange={(compression) => {
              if (compression) {
                void save(
                  updateArchive(task, (value) => ({
                    ...value,
                    output: {
                      ...value.output,
                      compression:
                        compression as typeof value.output.compression,
                    },
                  })),
                );
              }
            }}
          />
          <TextInput
            label={t("outputDirectory")}
            value={outputDirectory}
            onBlur={saveOutputText}
            onChange={(event) => setOutputDirectory(event.currentTarget.value)}
          />
          <TextInput
            label={t("outputFilename")}
            value={outputFilename}
            onBlur={saveOutputText}
            onChange={(event) => setOutputFilename(event.currentTarget.value)}
          />
          <Select
            data={[
              { label: t("conflictSkip"), value: "skip" },
              { label: t("conflictOverwrite"), value: "overwrite" },
              { label: t("conflictIncrement"), value: "increment" },
            ]}
            label={t("conflictPolicy")}
            value={archive.output.conflict_policy}
            onChange={(policy) => {
              if (policy) {
                void save(
                  updateArchive(task, (value) => ({
                    ...value,
                    output: {
                      ...value.output,
                      conflict_policy:
                        policy as typeof value.output.conflict_policy,
                    },
                  })),
                );
              }
            }}
          />
          <Select
            data={[
              { label: t("unreadableFail"), value: "fail" },
              { label: t("unreadableSkip"), value: "warn_and_skip" },
            ]}
            label={t("unreadablePolicy")}
            value={archive.unreadable_policy}
            onChange={(policy) => {
              if (policy) {
                void save(
                  updateArchive(task, (value) => ({
                    ...value,
                    unreadable_policy: policy as typeof value.unreadable_policy,
                  })),
                );
              }
            }}
          />
          <Divider />
          <Checkbox
            checked={archive.include_root}
            label={t("includeRoot")}
            onChange={(event) =>
              void save(
                updateArchive(task, (value) => ({
                  ...value,
                  include_root: event.currentTarget.checked,
                })),
              )
            }
          />
          <Checkbox
            checked={archive.verification.mode === "full"}
            label={t("fullVerification")}
            onChange={(event) =>
              void save(
                updateArchive(task, (value) => ({
                  ...value,
                  verification: {
                    ...value.verification,
                    mode: event.currentTarget.checked ? "full" : "structural",
                  },
                })),
              )
            }
          />
          <Text aria-live="polite" c="dimmed" size="xs">
            {status === "saving"
              ? t("taskSaving")
              : status === "error"
                ? t("taskSaveFailed")
                : t("taskSaved")}
          </Text>
          <Button
            leftSection={<Play aria-hidden size={17} />}
            variant="default"
            onClick={() => onRun(task)}
          >
            {t("runTask")}
          </Button>
          {confirmRemove ? (
            <Alert
              color="red"
              icon={<WarningCircle aria-hidden size={18} />}
              title={t("removeTask")}
            >
              <Group gap="xs" mt="xs">
                <Button
                  size="xs"
                  variant="default"
                  onClick={() => setConfirmRemove(false)}
                >
                  {t("cancel")}
                </Button>
                <Button
                  color="red"
                  size="xs"
                  onClick={() => void onRemove(task)}
                >
                  {t("confirm")}
                </Button>
              </Group>
            </Alert>
          ) : (
            <Button
              color="red"
              leftSection={<Trash aria-hidden size={16} />}
              variant="subtle"
              onClick={() => setConfirmRemove(true)}
            >
              {t("removeTask")}
            </Button>
          )}
        </Stack>
      ) : (
        <Text c="dimmed" size="sm">
          {t("selectTask")}
        </Text>
      )}
    </aside>
  );
}
