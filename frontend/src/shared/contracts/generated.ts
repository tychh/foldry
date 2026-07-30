// Generated from foldry-application Rust transport DTOs. Do not edit.

export type JsonValue = null | boolean | number | string | Array<JsonValue> | { [key: string]: JsonValue };

export type ProfileId = string;

export type PresetId = string;

export type FolderId = string;

export type ActionId = string;

export type RunId = string;

export type ArchiveFormat = "zip" | "tar_gz" | "tar_zst";

export type CompressionLevel = "fast" | "balanced" | "maximum";

export type ConflictPolicy = "skip" | "overwrite" | "increment";

export type UnreadablePolicy = "fail" | "warn_and_skip";

export type VerificationMode = "structural" | "full";

export type ChecksumAlgorithm = "none" | "sha256";

export type ArchiveOutputDirectory = { "mode": "parent" } | { "mode": "custom", path: string, };

export type ArchiveOutputSpec = { directory: ArchiveOutputDirectory, filename: string, format: ArchiveFormat, compression: CompressionLevel, conflict_policy: ConflictPolicy, extensions: { [key in string]: JsonValue }, };

export type VerificationSpec = { mode: VerificationMode, checksum: ChecksumAlgorithm, extensions: { [key in string]: JsonValue }, };

export type ArchiveActionSpec = { version: number, output: ArchiveOutputSpec, include_root: boolean, unreadable_policy: UnreadablePolicy, verification: VerificationSpec, extensions: { [key in string]: JsonValue }, };

export type ActionSpec = { action_type: string, version: number | null, archive: ArchiveActionSpec | null, fields: { [key in string]: JsonValue }, };

export type FolderAction = { id: ActionId, enabled: boolean, profile_id_override: ProfileId | null, spec: ActionSpec, extensions: { [key in string]: JsonValue }, };

export type Folder = { id: FolderId, source: string, listed: boolean, enabled: boolean, default_profile_id: ProfileId, actions: Array<FolderAction>, extensions: { [key in string]: JsonValue }, };

export type Plan = { version: number, name: string, folders: Array<Folder>, extensions: { [key in string]: JsonValue }, };

export type Locale = "en" | "ru";

export type Appearance = "system" | "light" | "dark";

export type BrowserView = "tree" | "list";

export type ArchiveDefaults = { output_directory: string, format: ArchiveFormat, compression: CompressionLevel, conflict_policy: ConflictPolicy, include_root: boolean, unreadable_policy: UnreadablePolicy, verification_mode: VerificationMode, checksum: ChecksumAlgorithm, extensions: { [key in string]: JsonValue }, };

export type ExecutionSettings = { max_parallel_runs: number, extensions: { [key in string]: JsonValue }, };

export type RetentionPolicy = { unlimited: boolean, max_age_days: number, max_entries: number, extensions: { [key in string]: JsonValue }, };

export type HistorySettings = { runs: RetentionPolicy, logs: RetentionPolicy, extensions: { [key in string]: JsonValue }, };

export type BrowserSettings = { favorites: Array<string>, recent: Array<string>, view: BrowserView, extensions: { [key in string]: JsonValue }, };

export type Settings = { version: number, locale: Locale, appearance: Appearance, default_profile_id: ProfileId | null, archive_defaults: ArchiveDefaults, execution: ExecutionSettings, history: HistorySettings, browser: BrowserSettings, extensions: { [key in string]: JsonValue }, };

export type DiagnosticSeverity = "error" | "warning";

export type DiagnosticCode = "invalid_metadata" | "duplicate_metadata" | "invalid_rule" | "invalid_escape" | "unterminated_character_class" | "duplicate_preset_block" | "unterminated_preset_block";

export type ParserDiagnostic = { code: DiagnosticCode, severity: DiagnosticSeverity, message: string, line: number | null, start_column: number | null, end_column: number | null, };

export type Profile = { version: number, id: ProfileId, name: string, text: string, valid: boolean, diagnostics: Array<ParserDiagnostic>, };

export type MatchDecision = "include" | "exclude";

export type MatchReason = { profile_id: ProfileId, line: number, original_rule: string, preset_id: PresetId | null, };

export type MatchResult = { path: string, decision: MatchDecision, reason: MatchReason | null, };

export type FileSystemObjectKind = "directory" | "regular_file" | "symlink" | "junction_or_reparse_point" | "special_file" | "unreadable";

export type BrowserRootKind = "home" | "file_system" | "documents" | "desktop" | "downloads" | "volumes" | "system_path" | "drive";

export type BrowserRoot = { id: string, path: string, name: string, kind: BrowserRootKind, };

export type BrowserNode = { id: string, path: string, name: string, kind: FileSystemObjectKind, is_mount_point: boolean, is_network_mount: boolean, is_platform_special: boolean, available: boolean,
/**
 * Decimal Unix milliseconds string to avoid JavaScript integer precision loss.
 */
modified_at_unix_ms: string | null, };

export type BrowserSize = { path: string,
/**
 * Decimal string to avoid JavaScript integer precision loss.
 */
logical_bytes: string, partial: boolean, warnings: bigint,
/**
 * Monotonic correlation ID for the requested directory.
 */
generation: string, };

export type ScanDisposition = "included" | "excluded" | "skipped";

export type PreviewEntry = { relative_path: string, kind: FileSystemObjectKind, disposition: ScanDisposition,
/**
 * Decimal string to avoid JavaScript integer precision loss.
 */
size: string, is_mount_point: boolean, is_network_mount: boolean, reason: MatchReason | null, };

export type ScanSummary = { visited_entries: string, included_entries: string, excluded_entries: string, skipped_entries: string, included_files: string, included_directories: string, included_links: string, included_bytes: string, notices: string, };

export type PreviewFilter = "all" | "included" | "excluded" | "skipped";

export type PreviewSnapshot = { preview_id: string, created_at: string, profile_hash: string, summary: ScanSummary, };

export type PreviewPage = { entries: Array<PreviewEntry>, next_cursor: string | null, };

export type FolderState = "ready" | "invalid" | "disabled";

export type RunState = "queued" | "planning" | "running" | "paused" | "stopping" | "succeeded" | "succeeded_with_warnings" | "failed" | "stopped" | "interrupted";

export type ProgressPhase = "planning" | "archiving" | "verifying" | "publishing";

export type ProgressSnapshot = { phase: ProgressPhase,
/**
 * Decimal string to avoid JavaScript integer precision loss.
 */
completed_entries: string,
/**
 * Decimal string to avoid JavaScript integer precision loss.
 */
total_entries: string | null,
/**
 * Decimal string to avoid JavaScript integer precision loss.
 */
completed_bytes: string,
/**
 * Decimal string to avoid JavaScript integer precision loss.
 */
total_bytes: string | null, current_path: string | null, };

export type WarningCode = "zip_symlink_portability" | "junction_skipped" | "special_file_skipped" | "unreadable_entry_skipped" | "source_entry_changed";

export type ErrorCode = "invalid_configuration" | "invalid_profile" | "unsupported_action" | "source_unavailable" | "output_unavailable" | "output_conflict" | "read_failed" | "write_failed" | "verification_failed" | "cancelled" | "internal";

export type FoldryWarning = { code: WarningCode, message: string, path: string | null, extensions: { [key in string]: JsonValue }, };

export type FoldryError = { code: ErrorCode, message: string, retryable: boolean, path: string | null, extensions: { [key in string]: JsonValue }, };

export type ArchiveArtifact = { path: string,
/**
 * Decimal string to avoid JavaScript integer precision loss.
 */
size_bytes: string, checksum_sha256: string | null, };

export type RunOutcome = "succeeded" | "succeeded_with_warnings" | "failed" | "stopped" | "interrupted";

export type ResultSummary = { outcome: RunOutcome, included_entries: string, skipped_entries: string, source_bytes: string, duration_ms: string, artifact: ArchiveArtifact | null, warnings: Array<FoldryWarning>, error: FoldryError | null, };

export type RunEventKind = { "type": "state_changed", state: RunState, } | { "type": "progress", progress: ProgressSnapshot, } | { "type": "warning", warning: FoldryWarning, } | { "type": "error", error: FoldryError, } | { "type": "completed", summary: ResultSummary, };

export type RunEvent = { version: number, run_id: RunId, folder_id: FolderId, action_id: ActionId, sequence: string, occurred_at: string, event: RunEventKind, extensions: { [key in string]: JsonValue }, };

export type ValidationCode = "unsupported_document_version" | "empty_name" | "empty_source" | "duplicate_folder_id" | "duplicate_action_id" | "duplicate_source" | "empty_output_directory" | "invalid_output_filename" | "output_inside_source" | "reserved_extension_field" | "invalid_parallel_runs" | "invalid_retention";

export type ValidationIssue = { code: ValidationCode, path: string, message: string, };

export type ExecutionBlockerCode = "unsupported_action_type" | "unsupported_action_version";

export type ExecutionBlocker = { code: ExecutionBlockerCode, path: string, message: string, };

export type StoredProfile = { id: ProfileId | null, filename: string, name: string, text: string, valid: boolean, diagnostics: Array<ParserDiagnostic>, };

export type StoredPreset = { id: PresetId, filename: string, text: string, resource_version: number | null, };

export type FolderSnapshot = { id: FolderId, source: string, };

export type RunSnapshot = { folder: FolderSnapshot, action: FolderAction, effective_profile_id: ProfileId, settings: Settings, profile_hash: string, };

export type RunRecord = { run_id: RunId, folder_id: FolderId, action_id: ActionId, state: RunState, started_at: string, finished_at: string | null, snapshot: RunSnapshot, summary: ResultSummary | null, };

export type LogLevel = "trace" | "debug" | "info" | "warn" | "error";

export type LogRecord = { run_id: RunId,
/**
 * Decimal string to avoid JavaScript integer precision loss.
 */
sequence: string, occurred_at: string, level: LogLevel, message: string, path: string | null, };

export type StoragePaths = { config: string, data: string, cache: string, };

export type BootstrapSnapshot = { version: number, settings: Settings, plan: Plan, profiles: Array<StoredProfile>, presets: Array<StoredPreset>, active_runs: Array<RunRecord>, recent_runs: Array<RunRecord>, previews: Array<PreviewSnapshot>, roots: Array<BrowserRoot>, storage: StoragePaths, };

export type BrowserChildren = {
/**
 * Monotonic correlation ID for the requested directory.
 */
generation: string, nodes: Array<BrowserNode>, total: bigint, next_cursor: string | null, };

export type PreviewStarted = {
/**
 * Monotonic correlation ID for the requested folder.
 */
generation: string, snapshot: PreviewSnapshot, action: FolderAction, effective_profile_id: ProfileId, effective_profile_name: string,
/**
 * Logical bytes in regular files before applying the Ignore Profile.
 */
raw_bytes: string, raw_bytes_partial: boolean, raw_bytes_warnings: bigint, };

export type FolderAddResult = { folder: Folder, created: boolean, };

export type IpcError = { code: string, message: string, details: JsonValue | null, };

