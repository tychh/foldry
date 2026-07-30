import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type {
  BootstrapSnapshot,
  BrowserChildren,
  BrowserNode,
  BrowserSize,
  IpcError,
  LogRecord,
  PreviewEntry,
  PreviewFilter,
  PreviewPage,
  PreviewStarted,
  RunEvent,
  RunRecord,
  Settings,
  StoredProfile,
  Folder,
  FolderAction,
  FolderAddResult,
} from "../contracts/generated";
import { isTerminalRunState } from "../runs/runState";

export const RUN_EVENT_NAME = "foldry://run-event";
const DEFAULT_PROFILE_FILENAME = "default.packignore";
const DEFAULT_PROFILE_ID = "0190f5f0-7f8b-7d80-a120-4f4f9fe95c20";

export type DesktopCommand =
  | "bootstrap_snapshot"
  | "run_all_enabled"
  | "pause_all"
  | "stop_all"
  | "create_profile"
  | "rename_profile"
  | "save_profile"
  | "delete_profile"
  | "restore_default_profile"
  | "browser_children"
  | "cancel_browser_request"
  | "browser_node"
  | "browser_size"
  | "cancel_browser_size"
  | "pick_folders"
  | "add_folder"
  | "update_folder"
  | "unlist_folder"
  | "unlisted_folders"
  | "forget_folders"
  | "forget_all_unlisted_folders"
  | "profile_usage"
  | "browser_favorites"
  | "browser_recent"
  | "set_browser_view"
  | "set_favorite"
  | "add_action"
  | "update_action"
  | "remove_action"
  | "reorder_actions"
  | "save_settings"
  | "save_plan"
  | "run_action"
  | "run_folder"
  | "pause_run"
  | "resume_run"
  | "stop_run"
  | "resume_all"
  | "start_preview"
  | "preview_page"
  | "cancel_preview"
  | "history_page"
  | "run_details"
  | "logs_page"
  | "export_run_logs"
  | "repeat_run"
  | "reveal_run_output";

export type DesktopCommandArgs = Record<string, unknown>;

export interface DesktopClient {
  readonly preview: boolean;
  bootstrap(): Promise<BootstrapSnapshot>;
  command<T>(name: DesktopCommand, args?: DesktopCommandArgs): Promise<T>;
  listenRunEvents(handler: (event: RunEvent) => void): Promise<UnlistenFn>;
}

class TauriDesktopClient implements DesktopClient {
  readonly preview = false;

  bootstrap(): Promise<BootstrapSnapshot> {
    return invoke<BootstrapSnapshot>("bootstrap_snapshot");
  }

  command<T>(name: DesktopCommand, args?: DesktopCommandArgs): Promise<T> {
    return invoke<T>(name, args);
  }

  listenRunEvents(handler: (event: RunEvent) => void): Promise<UnlistenFn> {
    return listen<RunEvent>(RUN_EVENT_NAME, (event) => handler(event.payload));
  }
}

class BrowserPreviewClient implements DesktopClient {
  readonly preview = true;
  private snapshot = createPreviewSnapshot();

  async bootstrap(): Promise<BootstrapSnapshot> {
    const defaultProfile = this.ensureDefaultProfile();
    const knownProfileIds = new Set(
      this.snapshot.profiles.flatMap((profile) =>
        profile.id ? [profile.id] : [],
      ),
    );
    if (
      !this.snapshot.settings.default_profile_id ||
      !knownProfileIds.has(this.snapshot.settings.default_profile_id)
    ) {
      this.snapshot.settings.default_profile_id =
        defaultProfile.id ?? DEFAULT_PROFILE_ID;
    }
    for (const folder of this.snapshot.plan.folders) {
      if (!knownProfileIds.has(folder.default_profile_id)) {
        folder.default_profile_id = defaultProfile.id ?? DEFAULT_PROFILE_ID;
      }
      for (const action of folder.actions) {
        if (
          action.profile_id_override &&
          !knownProfileIds.has(action.profile_id_override)
        ) {
          action.profile_id_override = null;
        }
      }
    }
    return structuredClone(this.snapshot);
  }

  async command<T>(
    name: DesktopCommand,
    args: DesktopCommandArgs = {},
  ): Promise<T> {
    let result: unknown;
    if (name === "create_profile") {
      const profile = previewProfile(
        `${
          String(args.name ?? "Profile")
            .trim()
            .toLowerCase()
            .replaceAll(/[^a-z0-9]+/g, "-") || "profile"
        }.packignore`,
        String(args.name ?? "Profile").trim(),
        nextPreviewProfileId(this.snapshot.profiles.length),
      );
      this.snapshot.profiles.push(profile);
      result = profile;
    } else if (name === "rename_profile") {
      const profile = this.requireProfile(String(args.profileId));
      const nameValue = String(args.name).trim();
      profile.name = nameValue;
      profile.text = profile.text.replace(
        /^# @profile-name .*$/m,
        `# @profile-name ${nameValue}`,
      );
      result = profile;
    } else if (name === "save_profile") {
      const filename = String(args.filename);
      const text = String(args.text);
      const index = this.snapshot.profiles.findIndex(
        (profile) => profile.filename === filename,
      );
      const current = this.snapshot.profiles[index];
      const profile = previewProfile(
        filename,
        profileMetadata(text, "name") ?? current?.name ?? filename,
        profileMetadata(text, "id") ?? current?.id ?? null,
        text,
      );
      if (index < 0) {
        this.snapshot.profiles.push(profile);
      } else {
        this.snapshot.profiles[index] = profile;
      }
      result = profile;
    } else if (name === "delete_profile") {
      const index = this.snapshot.profiles.findIndex(
        (profile) => profile.id === String(args.profileId),
      );
      if (
        index >= 0 &&
        this.snapshot.profiles[index]?.filename === DEFAULT_PROFILE_FILENAME
      ) {
        throw {
          code: "conflict",
          message: "The default profile cannot be deleted",
          details: null,
        };
      }
      result = index >= 0;
      if (index >= 0) {
        this.snapshot.profiles.splice(index, 1);
      }
    } else if (name === "restore_default_profile") {
      const profile = previewProfile(
        DEFAULT_PROFILE_FILENAME,
        "Default",
        DEFAULT_PROFILE_ID,
        defaultPreviewProfileText(),
      );
      const index = this.snapshot.profiles.findIndex(
        (item) => item.filename === profile.filename,
      );
      if (index >= 0) {
        this.snapshot.profiles[index] = profile;
      } else {
        this.snapshot.profiles.unshift(profile);
      }
      result = profile;
    } else if (name === "browser_children") {
      const nodes = previewBrowserChildren(String(args.path));
      const offset = Number(args.cursor ?? 0);
      const limit = Number(args.limit ?? 250);
      const end = Math.min(nodes.length, offset + limit);
      result = {
        generation: "1",
        nodes: nodes.slice(offset, end),
        total: BigInt(nodes.length),
        next_cursor: end < nodes.length ? String(end) : null,
      } satisfies BrowserChildren;
    } else if (name === "browser_node") {
      const path = String(args.path);
      result =
        previewBrowserChildren(previewParentPath(path) ?? "").find(
          (node) => node.path === path,
        ) ?? previewBrowserNode(path, "directory");
    } else if (name === "browser_size") {
      result = {
        path: String(args.path),
        logical_bytes: "1834920448",
        partial: false,
        warnings: 0n,
        generation: "1",
      } satisfies BrowserSize;
    } else if (name === "browser_favorites") {
      result = [...this.snapshot.settings.browser.favorites];
    } else if (name === "browser_recent") {
      result = [...this.snapshot.settings.browser.recent];
    } else if (name === "set_browser_view") {
      const view = String(args.view) === "list" ? "list" : "tree";
      this.snapshot.settings.browser.view = view;
      result = view;
    } else if (name === "set_favorite") {
      const path = String(args.path);
      const favorites = this.snapshot.settings.browser.favorites.filter(
        (candidate) => candidate !== path,
      );
      if (args.favorite === true) {
        favorites.push(path);
      }
      this.snapshot.settings.browser.favorites = favorites;
      result = favorites;
    } else if (name === "profile_usage") {
      const profileId = String(args.profileId);
      result = this.snapshot.plan.folders.reduce(
        (count, folder) =>
          count +
          Number(folder.default_profile_id === profileId) +
          folder.actions.filter(
            (action) => action.profile_id_override === profileId,
          ).length,
        0,
      );
    } else if (
      name === "cancel_browser_request" ||
      name === "cancel_browser_size"
    ) {
      result = false;
    } else if (name === "pick_folders") {
      result = [];
    } else if (name === "add_folder") {
      result = this.addPreviewFolder(String(args.source), args);
    } else if (name === "update_folder") {
      const folder = structuredClone(args.folder as Folder);
      folder.default_profile_id = this.resolvePreviewProfileId(
        folder.default_profile_id,
      );
      const index = this.snapshot.plan.folders.findIndex(
        (candidate) => candidate.id === folder.id,
      );
      if (index < 0) {
        throw {
          code: "not_found",
          message: `Folder ${folder.id} was not found`,
          details: null,
        };
      }
      this.snapshot.plan.folders[index] = folder;
      result = folder;
    } else if (name === "unlist_folder") {
      const index = this.snapshot.plan.folders.findIndex(
        (folder) => folder.id === String(args.folderId),
      );
      result = index >= 0;
      if (index >= 0) {
        this.snapshot.plan.folders[index]!.listed = false;
      }
    } else if (name === "unlisted_folders") {
      result = this.snapshot.plan.folders.filter((folder) => !folder.listed);
    } else if (name === "forget_folders") {
      const ids = new Set(
        Array.isArray(args.folderIds) ? args.folderIds.map(String) : [],
      );
      const before = this.snapshot.plan.folders.length;
      if (
        this.snapshot.plan.folders.some(
          (folder) => folder.listed && ids.has(folder.id),
        )
      ) {
        throw {
          code: "conflict",
          message: "Listed folders cannot be forgotten",
          details: null,
        };
      }
      this.snapshot.plan.folders = this.snapshot.plan.folders.filter(
        (folder) => !ids.has(folder.id),
      );
      result = before - this.snapshot.plan.folders.length;
    } else if (name === "forget_all_unlisted_folders") {
      const before = this.snapshot.plan.folders.length;
      this.snapshot.plan.folders = this.snapshot.plan.folders.filter(
        (folder) => folder.listed,
      );
      result = before - this.snapshot.plan.folders.length;
    } else if (name === "add_action") {
      if (String(args.actionType) !== "archive") {
        throw {
          code: "unsupported_action",
          message: "Unsupported action type",
          details: null,
        };
      }
      const folder = this.requireFolder(String(args.folderId));
      const action = archiveAction(
        nextPreviewActionId(
          this.snapshot.plan.folders.reduce(
            (count, candidate) => count + candidate.actions.length,
            0,
          ),
        ),
        "{folder}.{date}",
        this.snapshot.settings,
      );
      action.enabled = Boolean(args.enabled);
      action.profile_id_override = args.profileIdOverride
        ? this.resolvePreviewProfileId(String(args.profileIdOverride))
        : null;
      folder.actions.push(action);
      result = action;
    } else if (name === "update_action") {
      const folder = this.requireFolder(String(args.folderId));
      const action = structuredClone(args.action as FolderAction);
      const index = folder.actions.findIndex(
        (candidate) => candidate.id === action.id,
      );
      if (index < 0) {
        throw {
          code: "not_found",
          message: `Action ${action.id} was not found`,
          details: null,
        };
      }
      folder.actions[index] = action;
      result = action;
    } else if (name === "remove_action") {
      const folder = this.requireFolder(String(args.folderId));
      const before = folder.actions.length;
      folder.actions = folder.actions.filter(
        (action) => action.id !== String(args.actionId),
      );
      result = folder.actions.length !== before;
    } else if (name === "reorder_actions") {
      const folder = this.requireFolder(String(args.folderId));
      const ids = Array.isArray(args.actionIds)
        ? args.actionIds.map(String)
        : [];
      if (
        ids.length !== folder.actions.length ||
        new Set(ids).size !== ids.length
      ) {
        throw {
          code: "invalid_request",
          message: "Reorder must contain every action exactly once",
          details: null,
        };
      }
      folder.actions = ids.map((id) =>
        folder.actions.find((action) => action.id === id)!,
      );
      result = undefined;
    } else if (name === "save_settings") {
      this.snapshot.settings = structuredClone(args.settings as Settings);
      if (
        this.snapshot.settings.default_profile_id &&
        !this.snapshot.profiles.some(
          (profile) => profile.id === this.snapshot.settings.default_profile_id,
        )
      ) {
        this.snapshot.settings.default_profile_id =
          this.ensureDefaultProfile().id ?? DEFAULT_PROFILE_ID;
      }
      result = this.snapshot.settings;
    } else if (name === "save_plan") {
      this.snapshot.plan = structuredClone(
        args.plan as BootstrapSnapshot["plan"],
      );
      for (const folder of this.snapshot.plan.folders) {
        folder.default_profile_id = this.resolvePreviewProfileId(
          folder.default_profile_id,
        );
      }
      result = this.snapshot.plan;
    } else if (name === "run_all_enabled") {
      const runs = this.snapshot.plan.folders
        .filter((folder) => folder.listed && folder.enabled)
        .flatMap((folder) =>
          folder.actions
            .filter((action) => action.enabled)
            .map((action) => ({ folder, action })),
        )
        .map(({ folder, action }, index) =>
          previewRun(
            folder,
            action,
            nextPreviewRunId(this.snapshot.active_runs.length + index),
            "queued",
          ),
        );
      this.snapshot.active_runs.push(...runs);
      result = runs;
    } else if (name === "run_folder") {
      const folder = this.snapshot.plan.folders.find(
        (candidate) => candidate.id === String(args.folderId),
      );
      if (!folder) {
        throw {
          code: "not_found",
          message: `Folder ${String(args.folderId)} was not found`,
          details: null,
        };
      }
      const runs = folder.actions
        .filter((action) => action.enabled)
        .map((action, index) =>
          previewRun(
            folder,
            action,
            nextPreviewRunId(this.snapshot.active_runs.length + index),
            "queued",
          ),
        );
      this.snapshot.active_runs.push(...runs);
      result = runs;
    } else if (name === "run_action") {
      const folder = this.snapshot.plan.folders.find(
        (candidate) => candidate.id === String(args.folderId),
      );
      const action = folder?.actions.find(
        (candidate) => candidate.id === String(args.actionId),
      );
      if (!folder || !action) {
        throw {
          code: "not_found",
          message: "Folder action was not found",
          details: null,
        };
      }
      const run = previewRun(
        folder,
        action,
        nextPreviewRunId(this.snapshot.active_runs.length),
        "queued",
      );
      this.snapshot.active_runs.push(run);
      result = run;
    } else if (name === "repeat_run") {
      const previous = this.allRuns().find(
        (candidate) => candidate.run_id === String(args.runId),
      );
      if (!previous) {
        throw {
          code: "not_found",
          message: `Run ${String(args.runId)} was not found`,
          details: null,
        };
      }
      const run = previewRun(
        this.snapshot.plan.folders.find(
          (folder) => folder.id === previous.folder_id,
        ) ?? snapshotFolderAsCurrent(previous.snapshot.folder),
        previous.snapshot.action,
        nextPreviewRunId(this.snapshot.active_runs.length + 10),
        "queued",
      );
      run.snapshot = structuredClone(previous.snapshot);
      this.snapshot.active_runs.push(run);
      result = run;
    } else if (name === "start_preview") {
      const folder = this.snapshot.plan.folders.find(
        (candidate) => candidate.id === String(args.folderId),
      );
      const action = folder?.actions.find(
        (candidate) => candidate.id === String(args.actionId),
      );
      if (!folder || !action) {
        throw {
          code: "not_found",
          message: "Folder action was not found",
          details: null,
        };
      }
      const effectiveProfileId =
        action.profile_id_override ?? folder.default_profile_id;
      result = {
        generation: "1",
        action,
        effective_profile_id: effectiveProfileId,
        effective_profile_name:
          this.snapshot.profiles.find(
            (profile) => profile.id === effectiveProfileId,
          )?.name ?? "Default",
        raw_bytes: "2454927360",
        raw_bytes_partial: false,
        raw_bytes_warnings: 0n,
        snapshot: {
          preview_id: "browser-preview",
          created_at: "2026-07-27T10:12:00Z",
          profile_hash:
            "af9a38d8f7804a9d11ea97d13863c05d7299bd94b36275bbd4a8905a85797a14",
          summary: {
            visited_entries: "12842",
            included_entries: "10316",
            excluded_entries: "2508",
            skipped_entries: "18",
            included_files: "9924",
            included_directories: "386",
            included_links: "6",
            included_bytes: "1847265280",
            notices: "1",
          },
        },
      } satisfies PreviewStarted;
    } else if (name === "preview_page") {
      const filter = String(args.filter ?? "all") as PreviewFilter;
      const offset = Number(args.cursor ?? 0);
      const limit = Number(args.limit ?? 200);
      const filtered = previewEntries().filter(
        (entry) => filter === "all" || entry.disposition === filter,
      );
      const entries = filtered.slice(offset, offset + limit);
      result = {
        entries,
        next_cursor:
          offset + entries.length < filtered.length
            ? String(offset + entries.length)
            : null,
      } satisfies PreviewPage;
    } else if (name === "cancel_preview") {
      result = true;
    } else if (name === "history_page") {
      const offset = Number(args.offset ?? 0);
      const limit = Number(args.limit ?? 50);
      const folderId = args.folderId ? String(args.folderId) : null;
      const actionId = args.actionId ? String(args.actionId) : null;
      result = this.allRuns()
        .filter(
          (run) =>
            (!folderId || run.folder_id === folderId) &&
            (!actionId || run.action_id === actionId),
        )
        .slice(offset, offset + limit);
    } else if (name === "run_details") {
      result =
        this.allRuns().find(
          (candidate) => candidate.run_id === String(args.runId),
        ) ?? null;
    } else if (name === "logs_page") {
      const offset = Number(args.offset ?? 0);
      const limit = Number(args.limit ?? 100);
      result = previewLogs(String(args.runId)).slice(offset, offset + limit);
    } else if (name === "export_run_logs") {
      result = "foldry-preview-logs.jsonl";
    } else if (name === "reveal_run_output") {
      result = undefined;
    } else if (
      name === "pause_all" ||
      name === "resume_all" ||
      name === "stop_all"
    ) {
      let changed = 0;
      for (const run of this.snapshot.active_runs) {
        const nextState =
          name === "pause_all" &&
          (run.state === "planning" || run.state === "running")
            ? "paused"
            : name === "resume_all" && run.state === "paused"
              ? "running"
              : name === "stop_all" && !isTerminalRunState(run.state)
                ? "stopped"
                : null;
        if (nextState) {
          run.state = nextState;
          if (nextState === "stopped") {
            run.finished_at = new Date().toISOString();
          }
          changed += 1;
        }
      }
      result = changed;
    } else if (
      name === "pause_run" ||
      name === "resume_run" ||
      name === "stop_run"
    ) {
      const run = this.snapshot.active_runs.find(
        (candidate) => candidate.run_id === String(args.runId),
      );
      if (run) {
        run.state =
          name === "pause_run"
            ? "paused"
            : name === "resume_run"
              ? "running"
              : "stopped";
        if (run.state === "stopped") {
          run.finished_at = new Date().toISOString();
        }
      }
      result = Boolean(run);
    }
    return result as T;
  }

  async listenRunEvents(
    _handler: (event: RunEvent) => void,
  ): Promise<UnlistenFn> {
    void _handler;
    return () => undefined;
  }

  private requireProfile(id: string): StoredProfile {
    const profile = this.snapshot.profiles.find((item) => item.id === id);
    if (!profile) {
      throw {
        code: "not_found",
        message: `Profile ${id} was not found`,
        details: null,
      };
    }
    return profile;
  }

  private requireFolder(id: string): Folder {
    const folder = this.snapshot.plan.folders.find((item) => item.id === id);
    if (!folder) {
      throw {
        code: "not_found",
        message: `Folder ${id} was not found`,
        details: null,
      };
    }
    return folder;
  }

  private ensureDefaultProfile(): StoredProfile {
    const existing = this.snapshot.profiles.find(
      (profile) => profile.filename === DEFAULT_PROFILE_FILENAME,
    );
    if (existing) {
      return existing;
    }
    const restored = previewProfile(
      DEFAULT_PROFILE_FILENAME,
      "Default",
      DEFAULT_PROFILE_ID,
      defaultPreviewProfileText(),
    );
    this.snapshot.profiles.unshift(restored);
    return restored;
  }

  private resolvePreviewProfileId(profileId: string): string {
    return this.snapshot.profiles.some((profile) => profile.id === profileId)
      ? profileId
      : (this.ensureDefaultProfile().id ?? DEFAULT_PROFILE_ID);
  }

  private allRuns(): RunRecord[] {
    return [...this.snapshot.active_runs, ...this.snapshot.recent_runs].sort(
      (left, right) => right.started_at.localeCompare(left.started_at),
    );
  }

  private addPreviewFolder(
    path: string,
    args: DesktopCommandArgs,
  ): FolderAddResult {
    const existing = this.snapshot.plan.folders.find(
      (folder) => folder.source.toLowerCase() === path.toLowerCase(),
    );
    if (existing) {
      existing.listed = true;
      return { folder: existing, created: false };
    }
    const profileId = this.resolvePreviewProfileId(
      String(args.defaultProfileId ?? DEFAULT_PROFILE_ID),
    );
    const action = archiveAction(
      nextPreviewActionId(this.snapshot.plan.folders.length),
      "{folder}.{date}",
      this.snapshot.settings,
    );
    const folder: Folder = {
      id: nextPreviewFolderId(this.snapshot.plan.folders.length),
      source: path,
      listed: true,
      enabled: true,
      default_profile_id: profileId,
      actions: [action],
      extensions: {},
    };
    this.snapshot.plan.folders.push(folder);
    const parent = previewParentPath(path);
    if (parent) {
      this.snapshot.settings.browser.recent = [
        parent,
        ...this.snapshot.settings.browser.recent.filter(
          (candidate) => candidate !== parent,
        ),
      ].slice(0, 10);
    }
    return { folder, created: true };
  }
}

function previewParentPath(path: string): string | null {
  const normalized = path.replaceAll("\\", "/").replace(/\/+$/, "");
  const separator = normalized.lastIndexOf("/");
  if (separator < 0) {
    return null;
  }
  const parent = normalized.slice(0, separator) || "/";
  return path.includes("\\") ? parent.replaceAll("/", "\\") : parent;
}

export function createDesktopClient(): DesktopClient {
  return isTauri() ? new TauriDesktopClient() : new BrowserPreviewClient();
}

export function normalizeIpcError(error: unknown): IpcError {
  if (
    typeof error === "object" &&
    error !== null &&
    "code" in error &&
    "message" in error
  ) {
    const candidate = error as Partial<IpcError>;
    return {
      code: String(candidate.code),
      message: String(candidate.message),
      details: candidate.details ?? null,
    };
  }
  return {
    code: "internal_error",
    message: error instanceof Error ? error.message : String(error),
    details: null,
  };
}

function archiveFolder(
  id: string,
  source: string,
  filename: string,
  profileId: string,
): Folder {
  return {
    id,
    source,
    listed: true,
    enabled: true,
    default_profile_id: profileId,
    actions: [archiveAction(`${id.slice(0, -1)}a`, filename)],
    extensions: {},
  };
}

function previewRun(
  folder: Folder,
  action: FolderAction,
  runId: string,
  state: RunRecord["state"],
): RunRecord {
  return {
    run_id: runId,
    folder_id: folder.id,
    action_id: action.id,
    state,
    started_at: "2026-07-27T09:30:00Z",
    finished_at: null,
    snapshot: {
      folder: { id: folder.id, source: folder.source },
      action: structuredClone(action),
      effective_profile_id:
        action.profile_id_override ?? folder.default_profile_id,
      settings: previewSettings(),
      profile_hash:
        "af9a38d8f7804a9d11ea97d13863c05d7299bd94b36275bbd4a8905a85797a14",
    },
    summary: null,
  };
}

function archiveAction(
  id: string,
  filename: string,
  settings?: Settings,
): FolderAction {
  const defaults = settings?.archive_defaults;
  return {
    id,
    enabled: false,
    profile_id_override: null,
    spec: {
      action_type: "archive",
      version: 1,
      archive: {
        version: 1,
        output: {
          directory: { mode: "parent" },
          filename,
          format: defaults?.format ?? "zip",
          compression: defaults?.compression ?? "balanced",
          conflict_policy: defaults?.conflict_policy ?? "increment",
          extensions: {},
        },
        include_root: defaults?.include_root ?? true,
        unreadable_policy: defaults?.unreadable_policy ?? "fail",
        verification: {
          mode: defaults?.verification_mode ?? "structural",
          checksum: defaults?.checksum ?? "none",
          extensions: {},
        },
        extensions: {},
      },
      fields: {},
    },
    extensions: {},
  };
}

function snapshotFolderAsCurrent(
  folder: RunRecord["snapshot"]["folder"],
): Folder {
  return {
    ...folder,
    listed: false,
    enabled: false,
    default_profile_id: DEFAULT_PROFILE_ID,
    actions: [],
    extensions: {},
  };
}

function previewSettings(): BootstrapSnapshot["settings"] {
  return {
    version: 1,
    locale: "en",
    appearance: "system",
    default_profile_id: DEFAULT_PROFILE_ID,
    archive_defaults: {
      output_directory: "D:\\Backups",
      format: "zip",
      compression: "balanced",
      conflict_policy: "increment",
      include_root: true,
      unreadable_policy: "fail",
      verification_mode: "structural",
      checksum: "none",
      extensions: {},
    },
    execution: { max_parallel_runs: 2, extensions: {} },
    history: {
      runs: {
        unlimited: false,
        max_age_days: 365,
        max_entries: 10_000,
        extensions: {},
      },
      logs: {
        unlimited: false,
        max_age_days: 90,
        max_entries: 1_000,
        extensions: {},
      },
      extensions: {},
    },
    browser: { favorites: [], recent: [], view: "tree", extensions: {} },
    extensions: {},
  };
}

function previewBrowserChildren(path: string): BrowserNode[] {
  const children: Record<string, Array<[string, BrowserNode["kind"]]>> = {
    "C:\\": [
      ["Users", "directory"],
      ["Program Files", "directory"],
      ["pagefile.sys", "regular_file"],
    ],
    "C:\\Users": [["Alice", "directory"]],
    "C:\\Users\\Alice": [
      ["Desktop", "directory"],
      ["Documents", "directory"],
      ["Downloads", "directory"],
    ],
    "C:\\Users\\Alice\\Documents": [
      ["Personal", "directory"],
      ["Work", "directory"],
      ["notes.txt", "regular_file"],
    ],
    "D:\\": [["Projects", "directory"]],
    "D:\\Projects": [["Work", "directory"]],
  };
  return (children[path] ?? []).map(([name, kind]) =>
    previewBrowserNode(
      path.endsWith("\\") ? `${path}${name}` : `${path}\\${name}`,
      kind,
    ),
  );
}

function previewBrowserNode(
  path: string,
  kind: BrowserNode["kind"],
): BrowserNode {
  return {
    id: `preview:${path.toLocaleLowerCase()}`,
    path,
    name: path.split(/[\\/]/).filter(Boolean).at(-1) ?? path,
    kind,
    is_mount_point: false,
    is_network_mount: false,
    is_platform_special: false,
    available: true,
    modified_at_unix_ms: "1785110400000",
  };
}

function createPreviewSnapshot(): BootstrapSnapshot {
  const profileId = DEFAULT_PROFILE_ID;
  const work = archiveFolder(
    "01982ce0-9381-7d55-9a28-7c932a635d24",
    "C:\\Users\\Alice\\Documents\\Work",
    "work-{date}",
    profileId,
  );
  const projects = archiveFolder(
    "01982ce0-a381-7d55-9a28-7c932a635d24",
    "D:\\Projects\\Work",
    "projects-{date}",
    profileId,
  );
  return {
    version: 1,
    settings: previewSettings(),
    plan: {
      version: 2,
      name: "Active plan",
      folders: [work, projects],
      extensions: {},
    },
    profiles: [
      {
        id: profileId,
        filename: DEFAULT_PROFILE_FILENAME,
        name: "Default",
        text: defaultPreviewProfileText(),
        valid: true,
        diagnostics: [],
      },
    ],
    presets: [
      {
        id: "python",
        filename: "python.packignore",
        text: "__pycache__/\n.pytest_cache/\n",
        resource_version: 1,
      },
      {
        id: "node",
        filename: "node.packignore",
        text: "node_modules/\n.npm/\n",
        resource_version: 1,
      },
      {
        id: "environment-secrets",
        filename: "environment-secrets.packignore",
        text:
          "# @preset-id environment-secrets\n" +
          "# @preset-version 1\n" +
          "# @preset-name Environment and secrets\n" +
          "# @preset-description Environment files and common secret material. Review before excluding.\n" +
          "# @preset-safety sensitive\n\n" +
          ".env\n.env.*\n*.secret\nsecrets/\n",
        resource_version: 1,
      },
    ],
    active_runs: [
      previewRun(
        work,
        work.actions[0]!,
        "01982ce0-b381-7d55-9a28-7c932a635d24",
        "running",
      ),
      previewRun(
        projects,
        projects.actions[0]!,
        "01982ce0-c381-7d55-9a28-7c932a635d24",
        "queued",
      ),
    ],
    recent_runs: [
      completedPreviewRun(
        work,
        "01982ce0-d381-7d55-9a28-7c932a635d24",
        "succeeded_with_warnings",
      ),
      completedPreviewRun(
        projects,
        "01982ce0-e381-7d55-9a28-7c932a635d24",
        "failed",
      ),
    ],
    previews: [],
    roots: [
      {
        id: "home",
        path: "C:\\Users\\Alice",
        name: "Home",
        kind: "home",
      },
      {
        id: "documents",
        path: "C:\\Users\\Alice\\Documents",
        name: "Documents",
        kind: "documents",
      },
      {
        id: "drive-c",
        path: "C:\\",
        name: "C:\\",
        kind: "drive",
      },
      {
        id: "drive-d",
        path: "D:\\",
        name: "D:\\",
        kind: "drive",
      },
    ],
    storage: {
      config: "C:\\Users\\Alice\\AppData\\Roaming\\Foldry",
      data: "C:\\Users\\Alice\\AppData\\Local\\Foldry",
      cache: "C:\\Users\\Alice\\AppData\\Local\\Foldry\\Cache",
    },
  };
}

function defaultPreviewProfileText(): string {
  return (
    `# @profile-id ${DEFAULT_PROFILE_ID}\n` +
    "# @profile-version 1\n" +
    "# @profile-name Default\n\n" +
    "# @preset-begin id=os-metadata version=2\n" +
    ".DS_Store\n.AppleDouble\n.LSOverride\nIcon?\n._*\n" +
    ".Spotlight-V100/\n.Trashes/\n\n" +
    "Thumbs.db\nThumbs.db:encryptable\nehthumbs.db\nDesktop.ini\n" +
    "$RECYCLE.BIN/\n*.stackdump\n\n" +
    ".directory\n.Trash-*/\n.fuse_hidden*\n.nfs*\n" +
    "# @preset-end id=os-metadata\n"
  );
}

function previewProfile(
  filename: string,
  name: string,
  id: string | null,
  text = `# @profile-id ${id}\n# @profile-version 1\n# @profile-name ${name}\n\n`,
): StoredProfile {
  const diagnostics =
    id && name
      ? []
      : [
          {
            code: "invalid_metadata" as const,
            severity: "error" as const,
            message: "Required profile metadata is missing.",
            line: 1,
            start_column: 1,
            end_column: 1,
          },
        ];
  return {
    id,
    filename,
    name,
    text,
    valid: diagnostics.length === 0,
    diagnostics,
  };
}

function profileMetadata(text: string, field: "id" | "name"): string | null {
  const marker = field === "id" ? "@profile-id" : "@profile-name";
  const line = text
    .split(/\r?\n/)
    .find((candidate) => candidate.startsWith(`# ${marker} `));
  return line?.slice(marker.length + 3).trim() || null;
}

function nextPreviewProfileId(index: number): string {
  const suffix = (0x600 + index).toString(16).padStart(4, "0");
  return `01982ce0-${suffix}-7d55-9a28-7c932a635d24`;
}

function nextPreviewFolderId(index: number): string {
  const suffix = (0x700 + index).toString(16).padStart(4, "0");
  return `01982ce0-${suffix}-7d55-9a28-7c932a635d24`;
}

function nextPreviewActionId(index: number): string {
  const suffix = (0x780 + index).toString(16).padStart(4, "0");
  return `01982ce0-${suffix}-7d55-9a28-7c932a635d24`;
}

function nextPreviewRunId(index: number): string {
  const suffix = (0x800 + index).toString(16).padStart(4, "0");
  return `01982ce0-${suffix}-7d55-9a28-7c932a635d24`;
}

function completedPreviewRun(
  folder: Folder,
  runId: string,
  state: "succeeded_with_warnings" | "failed",
): RunRecord {
  const run = previewRun(folder, folder.actions[0]!, runId, state);
  run.started_at = "2026-07-26T18:40:00Z";
  run.finished_at = "2026-07-26T18:41:42Z";
  run.summary = {
    outcome: state,
    included_entries: state === "failed" ? "231" : "10316",
    skipped_entries: state === "failed" ? "0" : "18",
    source_bytes: state === "failed" ? "9830400" : "1847265280",
    duration_ms: "102000",
    artifact:
      state === "failed"
        ? null
        : {
            path: "D:\\Backups\\work-2026-07-26.zip",
            size_bytes: "872415232",
            checksum_sha256: null,
          },
    warnings:
      state === "failed"
        ? []
        : [
            {
              code: "unreadable_entry_skipped",
              message: "An unreadable cache file was skipped.",
              path: "cache/locked.tmp",
              extensions: {},
            },
          ],
    error:
      state === "failed"
        ? {
            code: "read_failed",
            message: "A source file could not be read.",
            retryable: true,
            path: "build/output.lock",
            extensions: {},
          }
        : null,
  };
  return run;
}

function previewEntries(): PreviewEntry[] {
  const seed: PreviewEntry[] = [
    {
      relative_path: "src/main.ts",
      kind: "regular_file",
      disposition: "included",
      size: "4832",
      is_mount_point: false,
      is_network_mount: false,
      reason: null,
    },
    {
      relative_path: "src/features/folders/FoldersWorkspace.tsx",
      kind: "regular_file",
      disposition: "included",
      size: "21840",
      is_mount_point: false,
      is_network_mount: false,
      reason: null,
    },
    {
      relative_path: "node_modules/react/index.js",
      kind: "regular_file",
      disposition: "excluded",
      size: "190",
      is_mount_point: false,
      is_network_mount: false,
      reason: {
        profile_id: DEFAULT_PROFILE_ID,
        line: 7,
        original_rule: "node_modules/",
        preset_id: "node",
      },
    },
    {
      relative_path: ".git/objects",
      kind: "directory",
      disposition: "excluded",
      size: "0",
      is_mount_point: false,
      is_network_mount: false,
      reason: {
        profile_id: DEFAULT_PROFILE_ID,
        line: 6,
        original_rule: ".git/",
        preset_id: null,
      },
    },
    {
      relative_path: "cache/locked.tmp",
      kind: "unreadable",
      disposition: "skipped",
      size: "0",
      is_mount_point: false,
      is_network_mount: false,
      reason: null,
    },
  ];
  return Array.from({ length: 60 }, (_, index) => {
    const source = seed[index % seed.length]!;
    return {
      ...source,
      relative_path:
        index < seed.length
          ? source.relative_path
          : `sample-${String(index + 1).padStart(3, "0")}/${source.relative_path}`,
    };
  });
}

function previewLogs(runId: string): LogRecord[] {
  return Array.from({ length: 84 }, (_, index) => ({
    run_id: runId,
    sequence: String(index + 1),
    occurred_at: `2026-07-26T18:40:${String(index % 60).padStart(2, "0")}Z`,
    level: index === 71 ? "warn" : index === 83 ? "error" : "info",
    message:
      index === 71
        ? "Skipped unreadable source entry"
        : index === 83
          ? "Run finished"
          : `Archived entry ${index + 1}`,
    path: index % 3 === 0 ? `src/sample-${index + 1}.txt` : null,
  }));
}
