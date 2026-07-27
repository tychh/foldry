/* eslint-disable react-hooks/set-state-in-effect */

import {
  ActionIcon,
  Alert,
  Badge,
  Box,
  Button,
  Code,
  Divider,
  Group,
  Modal,
  Paper,
  ScrollArea,
  Stack,
  Switch,
  Text,
  TextInput,
  Title,
  Tooltip,
} from "@mantine/core";
import { useDisclosure } from "@mantine/hooks";
import {
  ArrowsClockwise,
  Check,
  FileText,
  FloppyDisk,
  PencilSimple,
  Plus,
  ShieldCheck,
  ShieldWarning,
  Trash,
  WarningCircle,
} from "@phosphor-icons/react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import type {
  BootstrapSnapshot,
  ParserDiagnostic,
  StoredProfile,
} from "../../shared/contracts/generated";
import { useI18n } from "../../shared/i18n/I18nProvider";
import { useDesktopData } from "../../shared/ipc/DesktopDataProvider";
import { ProfileCodeEditor } from "./ProfileCodeEditor";
import { finishSave, reconcileExternalText } from "./profileDraft";
import {
  changedLines,
  insertPreset,
  parsePresetDefinition,
  type PresetDefinition,
  type PresetInstallationState,
  presetState,
  removePreset,
  updatePreset,
} from "./profilePresets";
import classes from "./ProfilesWorkspace.module.css";

type SaveState = "saved" | "dirty" | "saving" | "error";
type PresetConfirmation = {
  id: string;
  operation: "insert-sensitive" | "remove-modified";
} | null;
type DiffPreview = {
  preset: PresetDefinition;
  nextText: string;
} | null;

export function ProfilesWorkspace({
  snapshot,
}: {
  snapshot: BootstrapSnapshot;
}) {
  const { t } = useI18n();
  const { command, error } = useDesktopData();
  const [selectedFilename, setSelectedFilename] = useState<string | null>(
    snapshot.profiles[0]?.filename ?? null,
  );
  const selected =
    snapshot.profiles.find(
      (profile) => profile.filename === selectedFilename,
    ) ?? null;
  const [draft, setDraft] = useState(selected?.text ?? "");
  const [lastSynced, setLastSynced] = useState(selected?.text ?? "");
  const [diagnostics, setDiagnostics] = useState<ParserDiagnostic[]>(
    selected?.diagnostics ?? [],
  );
  const [valid, setValid] = useState(selected?.valid ?? false);
  const [saveState, setSaveState] = useState<SaveState>("saved");
  const [autosave, setAutosave] = useState(true);
  const [externalConflict, setExternalConflict] = useState(false);
  const [deleteConfirmation, setDeleteConfirmation] = useState(false);
  const [presetConfirmation, setPresetConfirmation] =
    useState<PresetConfirmation>(null);
  const [diffPreview, setDiffPreview] = useState<DiffPreview>(null);
  const [name, setName] = useState("");
  const [createOpened, createModal] = useDisclosure(false);
  const [renameOpened, renameModal] = useDisclosure(false);
  const loadedFilename = useRef<string | null>(selected?.filename ?? null);
  const draftRef = useRef(draft);
  const lastSyncedRef = useRef(lastSynced);
  const selectedRef = useRef(selected);
  const saveRef = useRef<() => Promise<boolean>>(async () => true);

  const presets = useMemo(
    () => snapshot.presets.map(parsePresetDefinition),
    [snapshot.presets],
  );

  const save = useCallback(async () => {
    const profile = selectedRef.current;
    if (!profile || draftRef.current === lastSyncedRef.current) {
      return true;
    }
    const savingText = draftRef.current;
    setSaveState("saving");
    const saved = await command<StoredProfile>("save_profile", {
      filename: profile.filename,
      text: savingText,
    });
    if (!saved) {
      setSaveState("error");
      return false;
    }
    const finished = finishSave(draftRef.current, saved.text);
    setLastSynced(finished.lastSynced);
    lastSyncedRef.current = finished.lastSynced;
    setDiagnostics(saved.diagnostics);
    setValid(saved.valid);
    setExternalConflict(false);
    setSaveState(finished.saveState);
    return true;
  }, [command]);

  useEffect(() => {
    draftRef.current = draft;
    lastSyncedRef.current = lastSynced;
    selectedRef.current = selected;
    saveRef.current = save;
  }, [draft, lastSynced, save, selected]);

  useEffect(() => {
    if (!selected && snapshot.profiles[0]) {
      setSelectedFilename(snapshot.profiles[0].filename);
    }
  }, [selected, snapshot.profiles]);

  useEffect(() => {
    if (!selected) {
      loadedFilename.current = null;
      setDraft("");
      setLastSynced("");
      setDiagnostics([]);
      setValid(false);
      return;
    }
    if (loadedFilename.current !== selected.filename) {
      loadedFilename.current = selected.filename;
      setDraft(selected.text);
      setLastSynced(selected.text);
      setDiagnostics(selected.diagnostics);
      setValid(selected.valid);
      setSaveState("saved");
      setExternalConflict(false);
      setDeleteConfirmation(false);
      setPresetConfirmation(null);
      return;
    }
    if (
      selected.text !== lastSyncedRef.current &&
      selected.text !== draftRef.current
    ) {
      const reconciled = reconcileExternalText(
        {
          draft: draftRef.current,
          lastSynced: lastSyncedRef.current,
          externalConflict,
        },
        selected.text,
      );
      if (!reconciled.externalConflict) {
        setDraft(reconciled.draft);
        setLastSynced(reconciled.lastSynced);
        setDiagnostics(selected.diagnostics);
        setValid(selected.valid);
      }
      setExternalConflict(reconciled.externalConflict);
    }
  }, [externalConflict, selected]);

  useEffect(() => {
    if (!autosave || saveState !== "dirty") {
      return;
    }
    const timer = window.setTimeout(() => void save(), 650);
    return () => window.clearTimeout(timer);
  }, [autosave, save, saveState]);

  useEffect(
    () => () => {
      void saveRef.current();
    },
    [],
  );

  const changeDraft = (next: string) => {
    draftRef.current = next;
    setDraft(next);
    setSaveState(next === lastSyncedRef.current ? "saved" : "dirty");
    setExternalConflict(false);
  };

  const selectProfile = async (profile: StoredProfile) => {
    if (await save()) {
      setSelectedFilename(profile.filename);
    }
  };

  const createProfile = async () => {
    const created = await command<StoredProfile>("create_profile", { name });
    if (created) {
      setSelectedFilename(created.filename);
      setName("");
      createModal.close();
    }
  };

  const renameProfile = async () => {
    if (!selected?.id) {
      return;
    }
    if (!(await save())) {
      return;
    }
    const renamed = await command<StoredProfile>("rename_profile", {
      profileId: selected.id,
      name,
    });
    if (renamed) {
      setName("");
      renameModal.close();
    }
  };

  const deleteProfile = async () => {
    if (!selected?.id) {
      return;
    }
    const deleted = await command<boolean>("delete_profile", {
      profileId: selected.id,
    });
    if (deleted) {
      setSelectedFilename(null);
      setDeleteConfirmation(false);
    }
  };

  const applyPresetText = (next: string) => {
    changeDraft(next);
    setPresetConfirmation(null);
    setDiffPreview(null);
  };

  const activatePreset = (
    preset: PresetDefinition,
    state: PresetInstallationState,
  ) => {
    if (state === "absent") {
      if (preset.sensitive) {
        setPresetConfirmation({
          id: preset.id,
          operation: "insert-sensitive",
        });
      } else {
        applyPresetText(insertPreset(draft, preset));
      }
    } else if (state === "installed") {
      applyPresetText(removePreset(draft, preset));
    } else if (state === "modified") {
      setPresetConfirmation({
        id: preset.id,
        operation: "remove-modified",
      });
    } else {
      setDiffPreview({ preset, nextText: updatePreset(draft, preset) });
    }
  };

  return (
    <Box className={classes.workspace}>
      <aside className={classes.profileRail}>
        <Group justify="space-between" wrap="nowrap">
          <Title order={2}>{t("profilesTitle")}</Title>
          <Tooltip label={t("newProfile")}>
            <ActionIcon
              aria-label={t("newProfile")}
              variant="default"
              onClick={() => {
                setName("");
                createModal.open();
              }}
            >
              <Plus aria-hidden size={17} />
            </ActionIcon>
          </Tooltip>
        </Group>
        <Button
          fullWidth
          justify="flex-start"
          leftSection={<Plus aria-hidden size={17} />}
          variant="default"
          onClick={() => {
            setName("");
            createModal.open();
          }}
        >
          {t("newProfile")}
        </Button>
        <ScrollArea className={classes.profileList}>
          <Stack gap={4}>
            {snapshot.profiles.map((profile) => (
              <button
                className={classes.profileRow}
                data-active={profile.filename === selectedFilename || undefined}
                key={profile.filename}
                type="button"
                onClick={() => void selectProfile(profile)}
              >
                <FileText aria-hidden size={18} />
                <span>{profile.name}</span>
                <Box
                  aria-label={profile.valid ? t("valid") : t("invalid")}
                  className={classes.validity}
                  data-valid={profile.valid || undefined}
                />
              </button>
            ))}
          </Stack>
        </ScrollArea>
        {snapshot.profiles.every(
          (profile) => profile.filename !== "default.packignore",
        ) ? (
          <Button
            leftSection={<ArrowsClockwise aria-hidden size={16} />}
            variant="subtle"
            onClick={() => void command("restore_default_profile")}
          >
            {t("restoreDefault")}
          </Button>
        ) : null}
      </aside>

      <main className={classes.editorPanel}>
        {selected ? (
          <>
            <Group className={classes.editorHeader} justify="space-between">
              <Box miw={0}>
                <Group gap="xs" wrap="nowrap">
                  <Title order={1}>{selected.name}</Title>
                  <ActionIcon
                    aria-label={t("renameProfile")}
                    size="sm"
                    variant="subtle"
                    onClick={() => {
                      setName(selected.name);
                      renameModal.open();
                    }}
                  >
                    <PencilSimple aria-hidden size={15} />
                  </ActionIcon>
                </Group>
                <Text c="dimmed" mt={3} size="xs">
                  {selected.filename}
                </Text>
              </Box>
              <Group gap="md" wrap="nowrap">
                <SaveIndicator state={saveState} />
                <Switch
                  checked={autosave}
                  label={t("autosave")}
                  onChange={(event) => {
                    const checked = event.currentTarget.checked;
                    setAutosave(checked);
                    if (!checked) {
                      void save();
                    }
                  }}
                />
                {!autosave ? (
                  <Button
                    leftSection={<FloppyDisk aria-hidden size={16} />}
                    loading={saveState === "saving"}
                    size="xs"
                    variant="default"
                    onClick={() => void save()}
                  >
                    {t("saveNow")}
                  </Button>
                ) : null}
                <ActionIcon
                  aria-label={t("deleteProfile")}
                  color="red"
                  variant="subtle"
                  onClick={() => setDeleteConfirmation(true)}
                >
                  <Trash aria-hidden size={18} />
                </ActionIcon>
              </Group>
            </Group>

            {externalConflict ? (
              <Alert
                color="orange"
                icon={<WarningCircle aria-hidden size={19} />}
                title={t("externalChange")}
              >
                <Text size="sm">{t("externalChangeHint")}</Text>
                <Group gap="xs" mt="sm">
                  <Button
                    size="xs"
                    variant="default"
                    onClick={() => setExternalConflict(false)}
                  >
                    {t("keepDraft")}
                  </Button>
                  <Button
                    color="orange"
                    size="xs"
                    onClick={() => {
                      changeDraft(selected.text);
                      setLastSynced(selected.text);
                      lastSyncedRef.current = selected.text;
                      setDiagnostics(selected.diagnostics);
                      setValid(selected.valid);
                      setExternalConflict(false);
                    }}
                  >
                    {t("reloadDisk")}
                  </Button>
                </Group>
              </Alert>
            ) : null}

            {deleteConfirmation ? (
              <Paper className={classes.inlineConfirmation} withBorder>
                <Box>
                  <Text fw={650}>{t("deleteProfileQuestion")}</Text>
                  <Text c="dimmed" size="xs">
                    {t("deleteProfileHint")}
                  </Text>
                </Box>
                <Group gap="xs">
                  <Button
                    size="xs"
                    variant="default"
                    onClick={() => setDeleteConfirmation(false)}
                  >
                    {t("cancel")}
                  </Button>
                  <Button
                    color="red"
                    size="xs"
                    onClick={() => void deleteProfile()}
                  >
                    {t("deleteProfile")}
                  </Button>
                </Group>
              </Paper>
            ) : null}

            {saveState === "error" && error ? (
              <Alert color="red" title={t("saveFailed")}>
                {error.message}
              </Alert>
            ) : null}

            <Paper className={classes.editorSurface} withBorder>
              <ProfileCodeEditor
                diagnostics={diagnostics}
                value={draft}
                onChange={changeDraft}
              />
            </Paper>
            <Text c="dimmed" className={classes.rulesHelp} size="xs">
              {t("rulesHelp")}
            </Text>
            <Diagnostics
              diagnostics={diagnostics}
              valid={valid}
              onSelectLine={() => undefined}
            />
          </>
        ) : (
          <Paper className={classes.emptyEditor} withBorder>
            <FileText aria-hidden size={32} />
            <Text c="dimmed">{t("selectProfile")}</Text>
          </Paper>
        )}
      </main>

      <aside className={classes.presetRail}>
        <Title order={2}>{t("presets")}</Title>
        <ScrollArea className={classes.presetList}>
          <Stack gap="sm" pr="xs">
            {presets.map((preset) => {
              const state = presetState(draft, preset);
              return (
                <PresetCard
                  confirmation={
                    presetConfirmation?.id === preset.id
                      ? presetConfirmation.operation
                      : null
                  }
                  key={preset.id}
                  preset={preset}
                  state={state}
                  onActivate={() => activatePreset(preset, state)}
                  onCancel={() => setPresetConfirmation(null)}
                  onConfirm={() => {
                    if (presetConfirmation?.operation === "insert-sensitive") {
                      applyPresetText(insertPreset(draft, preset));
                    } else {
                      applyPresetText(removePreset(draft, preset));
                    }
                  }}
                  onReviewUpdate={() =>
                    setDiffPreview({
                      preset,
                      nextText: updatePreset(draft, preset),
                    })
                  }
                />
              );
            })}
          </Stack>
        </ScrollArea>
      </aside>

      <NameModal
        name={name}
        opened={createOpened}
        title={t("createProfile")}
        onClose={createModal.close}
        onNameChange={setName}
        onSubmit={() => void createProfile()}
      />
      <NameModal
        name={name}
        opened={renameOpened}
        title={t("renameProfile")}
        onClose={renameModal.close}
        onNameChange={setName}
        onSubmit={() => void renameProfile()}
      />
      <DiffModal
        preview={diffPreview}
        profileText={draft}
        onApply={() => {
          if (diffPreview) {
            applyPresetText(diffPreview.nextText);
          }
        }}
        onClose={() => setDiffPreview(null)}
      />
    </Box>
  );
}

function SaveIndicator({ state }: { state: SaveState }) {
  const { t } = useI18n();
  const label = {
    saved: t("saved"),
    dirty: t("dirty"),
    saving: t("saving"),
    error: t("saveFailed"),
  }[state];
  return (
    <Text
      aria-live="polite"
      c={state === "error" ? "red" : state === "dirty" ? "orange" : "dimmed"}
      size="xs"
    >
      {state === "saved" ? <Check aria-hidden size={13} /> : null} {label}
    </Text>
  );
}

function Diagnostics({
  diagnostics,
  valid,
}: {
  diagnostics: ParserDiagnostic[];
  valid: boolean;
  onSelectLine: (line: number) => void;
}) {
  const { t } = useI18n();
  return (
    <Paper className={classes.diagnostics} withBorder>
      <Group justify="space-between">
        <Text fw={650} size="sm">
          {t("diagnostics")}
        </Text>
        <Badge color={valid ? "green" : "red"} variant="light">
          {valid ? t("valid") : t("invalid")}
        </Badge>
      </Group>
      {diagnostics.length === 0 ? (
        <Text c="dimmed" mt="xs" size="xs">
          {t("noDiagnostics")}
        </Text>
      ) : (
        <Stack gap={4} mt="xs">
          {diagnostics.map((diagnostic, index) => (
            <Text c="red" key={`${diagnostic.code}-${index}`} size="xs">
              {diagnostic.line ? `${diagnostic.line}: ` : ""}
              {diagnostic.message}
            </Text>
          ))}
        </Stack>
      )}
    </Paper>
  );
}

function PresetCard({
  preset,
  state,
  confirmation,
  onActivate,
  onCancel,
  onConfirm,
  onReviewUpdate,
}: {
  preset: PresetDefinition;
  state: PresetInstallationState;
  confirmation: "insert-sensitive" | "remove-modified" | null;
  onActivate: () => void;
  onCancel: () => void;
  onConfirm: () => void;
  onReviewUpdate: () => void;
}) {
  const { t } = useI18n();
  const stateLabel = {
    absent: t("presetAbsent"),
    installed: t("presetInstalled"),
    modified: t("presetModified"),
    outdated: t("presetOutdated"),
  }[state];
  const actionLabel =
    state === "absent"
      ? t("presetInsert")
      : state === "installed"
        ? t("presetRemove")
        : t("presetUpdate");

  return (
    <Paper className={classes.preset} data-state={state} p="md" withBorder>
      <Group justify="space-between" wrap="nowrap">
        <Text fw={650}>{preset.name}</Text>
        {preset.sensitive ? (
          <ShieldWarning aria-label={t("presetSensitive")} color="#d97706" />
        ) : (
          <ShieldCheck aria-label={t("presetSafe")} color="#16a34a" />
        )}
      </Group>
      <Text c="dimmed" mt={5} size="xs">
        {preset.description}
      </Text>
      <Group gap={6} mt="sm">
        <Badge color={stateColor(state)} size="sm" variant="light">
          {stateLabel}
        </Badge>
        <Badge color={preset.sensitive ? "orange" : "gray"} variant="outline">
          {preset.sensitive ? t("presetSensitive") : t("presetSafe")}
        </Badge>
      </Group>
      {confirmation ? (
        <Box className={classes.presetConfirmation} mt="sm">
          <Text fw={650} size="xs">
            {confirmation === "insert-sensitive"
              ? t("presetSensitiveQuestion")
              : t("presetModifiedQuestion")}
          </Text>
          <Text c="dimmed" mt={3} size="xs">
            {confirmation === "insert-sensitive"
              ? t("presetSensitiveHint")
              : t("presetModifiedHint")}
          </Text>
          <Group gap="xs" mt="xs">
            <Button size="compact-xs" variant="default" onClick={onCancel}>
              {t("cancel")}
            </Button>
            <Button
              color={confirmation === "insert-sensitive" ? "orange" : "red"}
              size="compact-xs"
              onClick={onConfirm}
            >
              {t("confirm")}
            </Button>
          </Group>
        </Box>
      ) : (
        <Group gap="xs" mt="sm">
          <Button
            color={state === "installed" ? "red" : "blue"}
            size="compact-xs"
            variant="subtle"
            onClick={onActivate}
          >
            {actionLabel}
          </Button>
          {state === "modified" ? (
            <Button
              size="compact-xs"
              variant="default"
              onClick={onReviewUpdate}
            >
              {t("previewChanges")}
            </Button>
          ) : null}
        </Group>
      )}
    </Paper>
  );
}

function NameModal({
  opened,
  title,
  name,
  onNameChange,
  onClose,
  onSubmit,
}: {
  opened: boolean;
  title: string;
  name: string;
  onNameChange: (name: string) => void;
  onClose: () => void;
  onSubmit: () => void;
}) {
  const { t } = useI18n();
  return (
    <Modal centered opened={opened} title={title} onClose={onClose}>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          onSubmit();
        }}
      >
        <TextInput
          autoFocus
          label={t("profileName")}
          maxLength={128}
          value={name}
          onChange={(event) => onNameChange(event.currentTarget.value)}
        />
        <Group justify="flex-end" mt="lg">
          <Button variant="default" onClick={onClose}>
            {t("cancel")}
          </Button>
          <Button disabled={name.trim().length === 0} type="submit">
            {t("confirm")}
          </Button>
        </Group>
      </form>
    </Modal>
  );
}

function DiffModal({
  preview,
  profileText,
  onClose,
  onApply,
}: {
  preview: DiffPreview;
  profileText: string;
  onClose: () => void;
  onApply: () => void;
}) {
  const { t } = useI18n();
  const diff = preview
    ? changedLines(profileText, preview.nextText)
    : { removed: [], added: [] };
  return (
    <Modal
      centered
      opened={preview !== null}
      size="lg"
      title={`${t("previewChanges")}${preview ? ` — ${preview.preset.name}` : ""}`}
      onClose={onClose}
    >
      {diff.removed.length === 0 && diff.added.length === 0 ? (
        <Text c="dimmed">{t("noLineChanges")}</Text>
      ) : (
        <Stack gap="md">
          <Box>
            <Text fw={650} size="sm">
              {t("linesRemoved")}
            </Text>
            <Code block className={classes.diffRemoved}>
              {diff.removed.map((line) => `- ${line}`).join("\n") || "—"}
            </Code>
          </Box>
          <Divider />
          <Box>
            <Text fw={650} size="sm">
              {t("linesAdded")}
            </Text>
            <Code block className={classes.diffAdded}>
              {diff.added.map((line) => `+ ${line}`).join("\n") || "—"}
            </Code>
          </Box>
        </Stack>
      )}
      <Group justify="flex-end" mt="lg">
        <Button variant="default" onClick={onClose}>
          {t("cancel")}
        </Button>
        <Button onClick={onApply}>{t("applyChanges")}</Button>
      </Group>
    </Modal>
  );
}

function stateColor(state: PresetInstallationState) {
  if (state === "installed") {
    return "green";
  }
  if (state === "modified") {
    return "orange";
  }
  if (state === "outdated") {
    return "blue";
  }
  return "gray";
}
