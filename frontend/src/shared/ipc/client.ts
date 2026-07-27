import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type {
  BootstrapSnapshot,
  BrowserChildren,
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
  Task,
  TaskAddResult,
} from "../contracts/generated";

export const RUN_EVENT_NAME = "foldry://run-event";

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
  | "pick_folders"
  | "add_dropped_sources"
  | "update_task"
  | "remove_task"
  | "save_settings"
  | "save_plan"
  | "run_task"
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
      result = index >= 0;
      if (index >= 0) {
        this.snapshot.profiles.splice(index, 1);
      }
    } else if (name === "restore_default_profile") {
      const profile = previewProfile(
        "default.packignore",
        "Default",
        "01982ce0-7381-7d55-9a28-7c932a635d24",
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
      result = { generation: "1", nodes: [] } satisfies BrowserChildren;
    } else if (name === "cancel_browser_request") {
      result = false;
    } else if (name === "pick_folders") {
      result = [];
    } else if (name === "add_dropped_sources") {
      const paths = Array.isArray(args.paths) ? args.paths.map(String) : [];
      result = paths.map((path) => this.addPreviewTask(path, args));
    } else if (name === "update_task") {
      const task = args.task as Task;
      const index = this.snapshot.plan.tasks.findIndex(
        (candidate) => candidate.id === task.id,
      );
      if (index < 0) {
        throw {
          code: "not_found",
          message: `Task ${task.id} was not found`,
          details: null,
        };
      }
      this.snapshot.plan.tasks[index] = structuredClone(task);
      result = task;
    } else if (name === "remove_task") {
      const index = this.snapshot.plan.tasks.findIndex(
        (task) => task.id === String(args.taskId),
      );
      result = index >= 0;
      if (index >= 0) {
        this.snapshot.plan.tasks.splice(index, 1);
      }
    } else if (name === "save_settings") {
      this.snapshot.settings = structuredClone(args.settings as Settings);
      result = this.snapshot.settings;
    } else if (name === "save_plan") {
      this.snapshot.plan = structuredClone(
        args.plan as BootstrapSnapshot["plan"],
      );
      result = this.snapshot.plan;
    } else if (name === "run_task") {
      const task = this.snapshot.plan.tasks.find(
        (candidate) => candidate.id === String(args.taskId),
      );
      if (!task) {
        throw {
          code: "not_found",
          message: `Task ${String(args.taskId)} was not found`,
          details: null,
        };
      }
      const run = previewRun(
        task,
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
        previous.snapshot.task,
        nextPreviewRunId(this.snapshot.active_runs.length + 10),
        "queued",
      );
      run.snapshot = structuredClone(previous.snapshot);
      this.snapshot.active_runs.push(run);
      result = run;
    } else if (name === "start_preview") {
      result = {
        generation: "1",
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
      result = this.allRuns().slice(offset, offset + limit);
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

  private allRuns(): RunRecord[] {
    return [...this.snapshot.active_runs, ...this.snapshot.recent_runs].sort(
      (left, right) => right.started_at.localeCompare(left.started_at),
    );
  }

  private addPreviewTask(
    path: string,
    args: DesktopCommandArgs,
  ): TaskAddResult {
    const existing = this.snapshot.plan.tasks.find(
      (task) => task.source.toLowerCase() === path.toLowerCase(),
    );
    if (existing) {
      return { task: existing, created: false };
    }
    const steps = structuredClone(args.steps as Task["steps"]);
    const task: Task = {
      id: nextPreviewTaskId(this.snapshot.plan.tasks.length),
      source: path,
      enabled: true,
      profile_id: String(args.profileId),
      steps,
      extensions: {},
    };
    this.snapshot.plan.tasks.push(task);
    return { task, created: true };
  }
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

function archiveTask(
  id: string,
  source: string,
  filename: string,
  profileId: string,
): Task {
  return {
    id,
    source,
    enabled: true,
    profile_id: profileId,
    steps: [
      {
        action_type: "archive",
        version: 1,
        archive: {
          version: 1,
          output: {
            directory: "D:\\Backups",
            filename,
            format: "zip",
            compression: "balanced",
            conflict_policy: "increment",
            extensions: {},
          },
          include_root: true,
          unreadable_policy: "fail",
          verification: {
            mode: "structural",
            checksum: "none",
            extensions: {},
          },
          extensions: {},
        },
        fields: {},
      },
    ],
    extensions: {},
  };
}

function previewRun(
  task: Task,
  runId: string,
  state: RunRecord["state"],
): RunRecord {
  return {
    run_id: runId,
    task_id: task.id,
    state,
    started_at: "2026-07-27T09:30:00Z",
    finished_at: null,
    snapshot: {
      task,
      settings: previewSettings(),
      profile_hash:
        "af9a38d8f7804a9d11ea97d13863c05d7299bd94b36275bbd4a8905a85797a14",
    },
    summary: null,
  };
}

function previewSettings(): BootstrapSnapshot["settings"] {
  return {
    version: 1,
    locale: "en",
    appearance: "system",
    default_profile_id: "01982ce0-7381-7d55-9a28-7c932a635d24",
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
    extensions: {},
  };
}

function createPreviewSnapshot(): BootstrapSnapshot {
  const profileId = "01982ce0-7381-7d55-9a28-7c932a635d24";
  const work = archiveTask(
    "01982ce0-9381-7d55-9a28-7c932a635d24",
    "C:\\Users\\Alice\\Documents\\Work",
    "work-{date}",
    profileId,
  );
  const projects = archiveTask(
    "01982ce0-a381-7d55-9a28-7c932a635d24",
    "D:\\Projects\\Work",
    "projects-{date}",
    profileId,
  );
  return {
    version: 1,
    settings: previewSettings(),
    plan: {
      version: 1,
      name: "Active plan",
      tasks: [work, projects],
      extensions: {},
    },
    profiles: [
      {
        id: profileId,
        filename: "default.packignore",
        name: "Default",
        text:
          `# @profile-id ${profileId}\n` +
          "# @profile-version 1\n" +
          "# @profile-name Default\n\n" +
          ".git/\nnode_modules/\ntarget/\n",
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
      previewRun(work, "01982ce0-b381-7d55-9a28-7c932a635d24", "running"),
      previewRun(projects, "01982ce0-c381-7d55-9a28-7c932a635d24", "queued"),
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
        name: "Alice",
        kind: "home",
      },
      {
        id: "drive-c",
        path: "C:\\",
        name: "C:\\",
        kind: "file_system",
      },
      {
        id: "drive-d",
        path: "D:\\",
        name: "D:\\",
        kind: "file_system",
      },
    ],
    storage: {
      config: "C:\\Users\\Alice\\AppData\\Roaming\\Foldry",
      data: "C:\\Users\\Alice\\AppData\\Local\\Foldry",
      cache: "C:\\Users\\Alice\\AppData\\Local\\Foldry\\Cache",
    },
  };
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

function nextPreviewTaskId(index: number): string {
  const suffix = (0x700 + index).toString(16).padStart(4, "0");
  return `01982ce0-${suffix}-7d55-9a28-7c932a635d24`;
}

function nextPreviewRunId(index: number): string {
  const suffix = (0x800 + index).toString(16).padStart(4, "0");
  return `01982ce0-${suffix}-7d55-9a28-7c932a635d24`;
}

function completedPreviewRun(
  task: Task,
  runId: string,
  state: "succeeded_with_warnings" | "failed",
): RunRecord {
  const run = previewRun(task, runId, state);
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
      relative_path: "src/features/tasks/TasksWorkspace.tsx",
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
        profile_id: "01982ce0-7381-7d55-9a28-7c932a635d24",
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
        profile_id: "01982ce0-7381-7d55-9a28-7c932a635d24",
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
