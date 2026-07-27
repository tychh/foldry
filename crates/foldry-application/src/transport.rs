//! Versioned transport DTOs and deterministic TypeScript bindings.

use std::{collections::BTreeMap, path::PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::{Config, TS};

/// Header used to identify generated frontend contracts.
pub const CONTRACTS_FILE_HEADER: &str =
    "// Generated from foldry-application Rust transport DTOs. Do not edit.\n";

macro_rules! string_id_dto {
    ($rust_name:ident, $ts_name:literal) => {
        #[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
        #[serde(transparent)]
        #[ts(rename = $ts_name)]
        pub struct $rust_name(pub String);
    };
}

string_id_dto!(ProfileIdDto, "ProfileId");
string_id_dto!(PresetIdDto, "PresetId");
string_id_dto!(TaskIdDto, "TaskId");
string_id_dto!(RunIdDto, "RunId");

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "ArchiveFormat", rename_all = "snake_case")]
pub enum ArchiveFormatDto {
    Zip,
    TarGz,
    TarZst,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "CompressionLevel", rename_all = "snake_case")]
pub enum CompressionLevelDto {
    Fast,
    Balanced,
    Maximum,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "ConflictPolicy", rename_all = "snake_case")]
pub enum ConflictPolicyDto {
    Skip,
    Overwrite,
    Increment,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "UnreadablePolicy", rename_all = "snake_case")]
pub enum UnreadablePolicyDto {
    Fail,
    WarnAndSkip,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "VerificationMode", rename_all = "snake_case")]
pub enum VerificationModeDto {
    Structural,
    Full,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "ChecksumAlgorithm", rename_all = "snake_case")]
pub enum ChecksumAlgorithmDto {
    None,
    Sha256,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, TS)]
#[ts(rename = "ArchiveOutputSpec")]
pub struct ArchiveOutputSpecDto {
    pub directory: String,
    pub filename: String,
    pub format: ArchiveFormatDto,
    pub compression: CompressionLevelDto,
    pub conflict_policy: ConflictPolicyDto,
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, TS)]
#[ts(rename = "VerificationSpec")]
pub struct VerificationSpecDto {
    pub mode: VerificationModeDto,
    pub checksum: ChecksumAlgorithmDto,
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, TS)]
#[ts(rename = "ArchiveActionSpec")]
pub struct ArchiveActionSpecDto {
    pub version: u16,
    pub output: ArchiveOutputSpecDto,
    pub include_root: bool,
    pub unreadable_policy: UnreadablePolicyDto,
    pub verification: VerificationSpecDto,
    pub extensions: BTreeMap<String, Value>,
}

/// Transport representation keeps unknown actions explicit and JSON-safe.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, TS)]
#[ts(rename = "ActionSpec")]
pub struct ActionSpecDto {
    pub action_type: String,
    pub version: Option<u16>,
    pub archive: Option<ArchiveActionSpecDto>,
    pub fields: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, TS)]
#[ts(rename = "Task")]
pub struct TaskDto {
    pub id: TaskIdDto,
    pub source: String,
    pub enabled: bool,
    pub profile_id: ProfileIdDto,
    pub steps: Vec<ActionSpecDto>,
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, TS)]
#[ts(rename = "Plan")]
pub struct PlanDto {
    pub version: u16,
    pub name: String,
    pub tasks: Vec<TaskDto>,
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "Locale", rename_all = "snake_case")]
pub enum LocaleDto {
    En,
    Ru,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "Appearance", rename_all = "snake_case")]
pub enum AppearanceDto {
    System,
    Light,
    Dark,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, TS)]
#[ts(rename = "ArchiveDefaults")]
pub struct ArchiveDefaultsDto {
    pub output_directory: String,
    pub format: ArchiveFormatDto,
    pub compression: CompressionLevelDto,
    pub conflict_policy: ConflictPolicyDto,
    pub include_root: bool,
    pub unreadable_policy: UnreadablePolicyDto,
    pub verification_mode: VerificationModeDto,
    pub checksum: ChecksumAlgorithmDto,
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, TS)]
#[ts(rename = "ExecutionSettings")]
pub struct ExecutionSettingsDto {
    pub max_parallel_runs: u16,
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, TS)]
#[ts(rename = "RetentionPolicy")]
pub struct RetentionPolicyDto {
    pub unlimited: bool,
    pub max_age_days: u32,
    pub max_entries: u32,
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, TS)]
#[ts(rename = "HistorySettings")]
pub struct HistorySettingsDto {
    pub runs: RetentionPolicyDto,
    pub logs: RetentionPolicyDto,
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, TS)]
#[ts(rename = "Settings")]
pub struct SettingsDto {
    pub version: u16,
    pub locale: LocaleDto,
    pub appearance: AppearanceDto,
    pub default_profile_id: Option<ProfileIdDto>,
    pub archive_defaults: ArchiveDefaultsDto,
    pub execution: ExecutionSettingsDto,
    pub history: HistorySettingsDto,
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "DiagnosticSeverity", rename_all = "snake_case")]
pub enum DiagnosticSeverityDto {
    Error,
    Warning,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "DiagnosticCode", rename_all = "snake_case")]
pub enum DiagnosticCodeDto {
    InvalidMetadata,
    DuplicateMetadata,
    InvalidRule,
    InvalidEscape,
    UnterminatedCharacterClass,
    DuplicatePresetBlock,
    UnterminatedPresetBlock,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, TS)]
#[ts(rename = "ParserDiagnostic")]
pub struct ParserDiagnosticDto {
    pub code: DiagnosticCodeDto,
    pub severity: DiagnosticSeverityDto,
    pub message: String,
    pub line: Option<u32>,
    pub start_column: Option<u32>,
    pub end_column: Option<u32>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, TS)]
#[ts(rename = "Profile")]
pub struct ProfileDto {
    pub version: u16,
    pub id: ProfileIdDto,
    pub name: String,
    pub text: String,
    pub valid: bool,
    pub diagnostics: Vec<ParserDiagnosticDto>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "MatchDecision", rename_all = "snake_case")]
pub enum MatchDecisionDto {
    Include,
    Exclude,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[ts(rename = "MatchReason")]
pub struct MatchReasonDto {
    pub profile_id: ProfileIdDto,
    pub line: u32,
    pub original_rule: String,
    pub preset_id: Option<PresetIdDto>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, TS)]
#[ts(rename = "MatchResult")]
pub struct MatchResultDto {
    pub path: String,
    pub decision: MatchDecisionDto,
    pub reason: Option<MatchReasonDto>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "FileSystemObjectKind", rename_all = "snake_case")]
pub enum FileSystemObjectKindDto {
    Directory,
    RegularFile,
    Symlink,
    JunctionOrReparsePoint,
    SpecialFile,
    Unreadable,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "BrowserRootKind", rename_all = "snake_case")]
pub enum BrowserRootKindDto {
    Home,
    FileSystem,
    Favorite,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[ts(rename = "BrowserRoot")]
pub struct BrowserRootDto {
    pub id: String,
    pub path: String,
    pub name: String,
    pub kind: BrowserRootKindDto,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[ts(rename = "BrowserNode")]
pub struct BrowserNodeDto {
    pub id: String,
    pub path: String,
    pub name: String,
    pub kind: FileSystemObjectKindDto,
    pub is_mount_point: bool,
    pub is_network_mount: bool,
    pub is_platform_special: bool,
    pub available: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "ScanDisposition", rename_all = "snake_case")]
pub enum ScanDispositionDto {
    Included,
    Excluded,
    Skipped,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[ts(rename = "PreviewEntry")]
pub struct PreviewEntryDto {
    pub relative_path: String,
    pub kind: FileSystemObjectKindDto,
    pub disposition: ScanDispositionDto,
    /// Decimal string to avoid JavaScript integer precision loss.
    pub size: String,
    pub is_mount_point: bool,
    pub is_network_mount: bool,
    pub reason: Option<MatchReasonDto>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[ts(rename = "ScanSummary")]
pub struct ScanSummaryDto {
    pub visited_entries: String,
    pub included_entries: String,
    pub excluded_entries: String,
    pub skipped_entries: String,
    pub included_files: String,
    pub included_directories: String,
    pub included_links: String,
    pub included_bytes: String,
    pub notices: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "PreviewFilter", rename_all = "snake_case")]
pub enum PreviewFilterDto {
    All,
    Included,
    Excluded,
    Skipped,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[ts(rename = "PreviewSnapshot")]
pub struct PreviewSnapshotDto {
    pub preview_id: String,
    pub created_at: String,
    pub profile_hash: String,
    pub summary: ScanSummaryDto,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[ts(rename = "PreviewPage")]
pub struct PreviewPageDto {
    pub entries: Vec<PreviewEntryDto>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "TaskState", rename_all = "snake_case")]
pub enum TaskStateDto {
    Ready,
    Invalid,
    Disabled,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "RunState", rename_all = "snake_case")]
pub enum RunStateDto {
    Queued,
    Planning,
    Running,
    Paused,
    Stopping,
    Succeeded,
    SucceededWithWarnings,
    Failed,
    Stopped,
    Interrupted,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "ProgressPhase", rename_all = "snake_case")]
pub enum ProgressPhaseDto {
    Planning,
    Archiving,
    Verifying,
    Publishing,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, TS)]
#[ts(rename = "ProgressSnapshot")]
pub struct ProgressSnapshotDto {
    pub phase: ProgressPhaseDto,
    /// Decimal string to avoid JavaScript integer precision loss.
    pub completed_entries: String,
    /// Decimal string to avoid JavaScript integer precision loss.
    pub total_entries: Option<String>,
    /// Decimal string to avoid JavaScript integer precision loss.
    pub completed_bytes: String,
    /// Decimal string to avoid JavaScript integer precision loss.
    pub total_bytes: Option<String>,
    pub current_path: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "WarningCode", rename_all = "snake_case")]
pub enum WarningCodeDto {
    ZipSymlinkPortability,
    JunctionSkipped,
    SpecialFileSkipped,
    UnreadableEntrySkipped,
    SourceEntryChanged,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "ErrorCode", rename_all = "snake_case")]
pub enum ErrorCodeDto {
    InvalidConfiguration,
    InvalidProfile,
    UnsupportedAction,
    SourceUnavailable,
    OutputUnavailable,
    OutputConflict,
    ReadFailed,
    WriteFailed,
    VerificationFailed,
    Cancelled,
    Internal,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, TS)]
#[ts(rename = "FoldryWarning")]
pub struct FoldryWarningDto {
    pub code: WarningCodeDto,
    pub message: String,
    pub path: Option<String>,
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, TS)]
#[ts(rename = "FoldryError")]
pub struct FoldryErrorDto {
    pub code: ErrorCodeDto,
    pub message: String,
    pub retryable: bool,
    pub path: Option<String>,
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, TS)]
#[ts(rename = "ArchiveArtifact")]
pub struct ArchiveArtifactDto {
    pub path: String,
    /// Decimal string to avoid JavaScript integer precision loss.
    pub size_bytes: String,
    pub checksum_sha256: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "RunOutcome", rename_all = "snake_case")]
pub enum RunOutcomeDto {
    Succeeded,
    SucceededWithWarnings,
    Failed,
    Stopped,
    Interrupted,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, TS)]
#[ts(rename = "ResultSummary")]
pub struct ResultSummaryDto {
    pub outcome: RunOutcomeDto,
    pub included_entries: String,
    pub skipped_entries: String,
    pub source_bytes: String,
    pub duration_ms: String,
    pub artifact: Option<ArchiveArtifactDto>,
    pub warnings: Vec<FoldryWarningDto>,
    pub error: Option<FoldryErrorDto>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(rename = "RunEventKind", tag = "type", rename_all = "snake_case")]
pub enum RunEventKindDto {
    StateChanged { state: RunStateDto },
    Progress { progress: ProgressSnapshotDto },
    Warning { warning: FoldryWarningDto },
    Error { error: FoldryErrorDto },
    Completed { summary: ResultSummaryDto },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, TS)]
#[ts(rename = "RunEvent")]
pub struct RunEventDto {
    pub version: u16,
    pub run_id: RunIdDto,
    pub task_id: TaskIdDto,
    pub sequence: String,
    pub occurred_at: String,
    pub event: RunEventKindDto,
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "ValidationCode", rename_all = "snake_case")]
pub enum ValidationCodeDto {
    UnsupportedDocumentVersion,
    EmptyName,
    EmptySource,
    DuplicateTaskId,
    DuplicateSource,
    InvalidStepCount,
    EmptyOutputDirectory,
    EmptyOutputFilename,
    ReservedExtensionField,
    InvalidParallelRuns,
    InvalidRetention,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[ts(rename = "ValidationIssue")]
pub struct ValidationIssueDto {
    pub code: ValidationCodeDto,
    pub path: String,
    pub message: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "ExecutionBlockerCode", rename_all = "snake_case")]
pub enum ExecutionBlockerCodeDto {
    UnsupportedActionType,
    UnsupportedActionVersion,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[ts(rename = "ExecutionBlocker")]
pub struct ExecutionBlockerDto {
    pub code: ExecutionBlockerCodeDto,
    pub path: String,
    pub message: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, TS)]
#[ts(rename = "StoredProfile")]
pub struct StoredProfileDto {
    pub id: Option<ProfileIdDto>,
    pub filename: String,
    pub name: String,
    pub text: String,
    pub valid: bool,
    pub diagnostics: Vec<ParserDiagnosticDto>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[ts(rename = "StoredPreset")]
pub struct StoredPresetDto {
    pub id: PresetIdDto,
    pub filename: String,
    pub text: String,
    pub resource_version: Option<u16>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, TS)]
#[ts(rename = "RunSnapshot")]
pub struct RunSnapshotDto {
    pub task: TaskDto,
    pub settings: SettingsDto,
    pub profile_hash: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, TS)]
#[ts(rename = "RunRecord")]
pub struct RunRecordDto {
    pub run_id: RunIdDto,
    pub task_id: TaskIdDto,
    pub state: RunStateDto,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub snapshot: RunSnapshotDto,
    pub summary: Option<ResultSummaryDto>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "LogLevel", rename_all = "snake_case")]
pub enum LogLevelDto {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[ts(rename = "LogRecord")]
pub struct LogRecordDto {
    pub run_id: RunIdDto,
    /// Decimal string to avoid JavaScript integer precision loss.
    pub sequence: String,
    pub occurred_at: String,
    pub level: LogLevelDto,
    pub message: String,
    pub path: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[ts(rename = "StoragePaths")]
pub struct StoragePathsDto {
    pub config: String,
    pub data: String,
    pub cache: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, TS)]
#[ts(rename = "BootstrapSnapshot")]
pub struct BootstrapSnapshotDto {
    pub version: u16,
    pub settings: SettingsDto,
    pub plan: PlanDto,
    pub profiles: Vec<StoredProfileDto>,
    pub presets: Vec<StoredPresetDto>,
    pub active_runs: Vec<RunRecordDto>,
    pub recent_runs: Vec<RunRecordDto>,
    pub previews: Vec<PreviewSnapshotDto>,
    pub roots: Vec<BrowserRootDto>,
    pub storage: StoragePathsDto,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[ts(rename = "BrowserChildren")]
pub struct BrowserChildrenDto {
    /// Monotonic correlation ID for the requested directory.
    pub generation: String,
    pub nodes: Vec<BrowserNodeDto>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, TS)]
#[ts(rename = "PreviewStarted")]
pub struct PreviewStartedDto {
    /// Monotonic correlation ID for the requested task.
    pub generation: String,
    pub snapshot: PreviewSnapshotDto,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, TS)]
#[ts(rename = "TaskAddResult")]
pub struct TaskAddResultDto {
    pub task: TaskDto,
    pub created: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, TS)]
#[ts(rename = "IpcError")]
pub struct IpcErrorDto {
    pub code: String,
    pub message: String,
    pub details: Option<serde_json::Value>,
}

impl From<crate::ArchiveFormat> for ArchiveFormatDto {
    fn from(value: crate::ArchiveFormat) -> Self {
        match value {
            crate::ArchiveFormat::Zip => Self::Zip,
            crate::ArchiveFormat::TarGz => Self::TarGz,
            crate::ArchiveFormat::TarZst => Self::TarZst,
        }
    }
}

impl From<crate::CompressionLevel> for CompressionLevelDto {
    fn from(value: crate::CompressionLevel) -> Self {
        match value {
            crate::CompressionLevel::Fast => Self::Fast,
            crate::CompressionLevel::Balanced => Self::Balanced,
            crate::CompressionLevel::Maximum => Self::Maximum,
        }
    }
}

impl From<crate::ConflictPolicy> for ConflictPolicyDto {
    fn from(value: crate::ConflictPolicy) -> Self {
        match value {
            crate::ConflictPolicy::Skip => Self::Skip,
            crate::ConflictPolicy::Overwrite => Self::Overwrite,
            crate::ConflictPolicy::Increment => Self::Increment,
        }
    }
}

impl From<crate::UnreadablePolicy> for UnreadablePolicyDto {
    fn from(value: crate::UnreadablePolicy) -> Self {
        match value {
            crate::UnreadablePolicy::Fail => Self::Fail,
            crate::UnreadablePolicy::WarnAndSkip => Self::WarnAndSkip,
        }
    }
}

impl From<crate::VerificationMode> for VerificationModeDto {
    fn from(value: crate::VerificationMode) -> Self {
        match value {
            crate::VerificationMode::Structural => Self::Structural,
            crate::VerificationMode::Full => Self::Full,
        }
    }
}

impl From<crate::ChecksumAlgorithm> for ChecksumAlgorithmDto {
    fn from(value: crate::ChecksumAlgorithm) -> Self {
        match value {
            crate::ChecksumAlgorithm::None => Self::None,
            crate::ChecksumAlgorithm::Sha256 => Self::Sha256,
        }
    }
}

impl From<&crate::ArchiveOutputSpec> for ArchiveOutputSpecDto {
    fn from(value: &crate::ArchiveOutputSpec) -> Self {
        Self {
            directory: value.directory.to_string_lossy().into_owned(),
            filename: value.filename.clone(),
            format: value.format.into(),
            compression: value.compression.into(),
            conflict_policy: value.conflict_policy.into(),
            extensions: value.extensions.clone(),
        }
    }
}

impl From<&crate::VerificationSpec> for VerificationSpecDto {
    fn from(value: &crate::VerificationSpec) -> Self {
        Self {
            mode: value.mode.into(),
            checksum: value.checksum.into(),
            extensions: value.extensions.clone(),
        }
    }
}

impl From<&crate::ArchiveActionSpec> for ArchiveActionSpecDto {
    fn from(value: &crate::ArchiveActionSpec) -> Self {
        Self {
            version: value.version.0,
            output: (&value.output).into(),
            include_root: value.include_root,
            unreadable_policy: value.unreadable_policy.into(),
            verification: (&value.verification).into(),
            extensions: value.extensions.clone(),
        }
    }
}

impl From<&crate::ActionSpec> for ActionSpecDto {
    fn from(value: &crate::ActionSpec) -> Self {
        match value {
            crate::ActionSpec::Archive(spec) => Self {
                action_type: "archive".into(),
                version: Some(spec.version.0),
                archive: Some(spec.into()),
                fields: BTreeMap::new(),
            },
            crate::ActionSpec::Unsupported(spec) => Self {
                action_type: spec.action_type.clone(),
                version: spec.version,
                archive: None,
                fields: spec.fields.clone(),
            },
        }
    }
}

impl From<&crate::Task> for TaskDto {
    fn from(value: &crate::Task) -> Self {
        Self {
            id: TaskIdDto(value.id.to_string()),
            source: value.source.to_string_lossy().into_owned(),
            enabled: value.enabled,
            profile_id: ProfileIdDto(value.profile_id.to_string()),
            steps: value.steps.iter().map(Into::into).collect(),
            extensions: value.extensions.clone(),
        }
    }
}

impl From<&crate::Plan> for PlanDto {
    fn from(value: &crate::Plan) -> Self {
        Self {
            version: value.version.0,
            name: value.name.clone(),
            tasks: value.tasks.iter().map(Into::into).collect(),
            extensions: value.extensions.clone(),
        }
    }
}

impl From<crate::FileSystemObjectKind> for FileSystemObjectKindDto {
    fn from(value: crate::FileSystemObjectKind) -> Self {
        match value {
            crate::FileSystemObjectKind::Directory => Self::Directory,
            crate::FileSystemObjectKind::RegularFile => Self::RegularFile,
            crate::FileSystemObjectKind::Symlink => Self::Symlink,
            crate::FileSystemObjectKind::JunctionOrReparsePoint => Self::JunctionOrReparsePoint,
            crate::FileSystemObjectKind::SpecialFile => Self::SpecialFile,
            crate::FileSystemObjectKind::Unreadable => Self::Unreadable,
        }
    }
}

impl From<crate::BrowserRootKind> for BrowserRootKindDto {
    fn from(value: crate::BrowserRootKind) -> Self {
        match value {
            crate::BrowserRootKind::Home => Self::Home,
            crate::BrowserRootKind::FileSystem => Self::FileSystem,
            crate::BrowserRootKind::Favorite => Self::Favorite,
        }
    }
}

impl From<&crate::BrowserRoot> for BrowserRootDto {
    fn from(value: &crate::BrowserRoot) -> Self {
        Self {
            id: value.id.clone(),
            path: value.path.to_string_lossy().into_owned(),
            name: value.name.clone(),
            kind: value.kind.into(),
        }
    }
}

impl From<&crate::BrowserNode> for BrowserNodeDto {
    fn from(value: &crate::BrowserNode) -> Self {
        Self {
            id: value.id.clone(),
            path: value.path.to_string_lossy().into_owned(),
            name: value.name.clone(),
            kind: value.kind.into(),
            is_mount_point: value.is_mount_point,
            is_network_mount: value.is_network_mount,
            is_platform_special: value.is_platform_special,
            available: value.available,
        }
    }
}

impl From<crate::ScanDisposition> for ScanDispositionDto {
    fn from(value: crate::ScanDisposition) -> Self {
        match value {
            crate::ScanDisposition::Included => Self::Included,
            crate::ScanDisposition::Excluded => Self::Excluded,
            crate::ScanDisposition::Skipped => Self::Skipped,
        }
    }
}

impl From<&crate::ScannedEntry> for PreviewEntryDto {
    fn from(value: &crate::ScannedEntry) -> Self {
        Self {
            relative_path: value.relative_path.clone(),
            kind: value.kind.into(),
            disposition: value.disposition.into(),
            size: value.size.to_string(),
            is_mount_point: value.is_mount_point,
            is_network_mount: value.is_network_mount,
            reason: value.reason.as_ref().map(Into::into),
        }
    }
}

impl From<&crate::ScanSummary> for ScanSummaryDto {
    fn from(value: &crate::ScanSummary) -> Self {
        Self {
            visited_entries: value.visited_entries.to_string(),
            included_entries: value.included_entries.to_string(),
            excluded_entries: value.excluded_entries.to_string(),
            skipped_entries: value.skipped_entries.to_string(),
            included_files: value.included_files.to_string(),
            included_directories: value.included_directories.to_string(),
            included_links: value.included_links.to_string(),
            included_bytes: value.included_bytes.to_string(),
            notices: value.notices.to_string(),
        }
    }
}

impl From<crate::PreviewFilter> for PreviewFilterDto {
    fn from(value: crate::PreviewFilter) -> Self {
        match value {
            crate::PreviewFilter::All => Self::All,
            crate::PreviewFilter::Included => Self::Included,
            crate::PreviewFilter::Excluded => Self::Excluded,
            crate::PreviewFilter::Skipped => Self::Skipped,
        }
    }
}

impl From<&crate::PreviewSnapshot> for PreviewSnapshotDto {
    fn from(value: &crate::PreviewSnapshot) -> Self {
        Self {
            preview_id: value.manifest_id.clone(),
            created_at: value.created_at.clone(),
            profile_hash: value.cache_key.profile_hash.clone(),
            summary: (&value.summary).into(),
        }
    }
}

impl From<crate::Locale> for LocaleDto {
    fn from(value: crate::Locale) -> Self {
        match value {
            crate::Locale::En => Self::En,
            crate::Locale::Ru => Self::Ru,
        }
    }
}

impl From<crate::Appearance> for AppearanceDto {
    fn from(value: crate::Appearance) -> Self {
        match value {
            crate::Appearance::System => Self::System,
            crate::Appearance::Light => Self::Light,
            crate::Appearance::Dark => Self::Dark,
        }
    }
}

impl From<&crate::ArchiveDefaults> for ArchiveDefaultsDto {
    fn from(value: &crate::ArchiveDefaults) -> Self {
        Self {
            output_directory: value.output_directory.to_string_lossy().into_owned(),
            format: value.format.into(),
            compression: value.compression.into(),
            conflict_policy: value.conflict_policy.into(),
            include_root: value.include_root,
            unreadable_policy: value.unreadable_policy.into(),
            verification_mode: value.verification_mode.into(),
            checksum: value.checksum.into(),
            extensions: value.extensions.clone(),
        }
    }
}

impl From<&crate::RetentionPolicy> for RetentionPolicyDto {
    fn from(value: &crate::RetentionPolicy) -> Self {
        Self {
            unlimited: value.unlimited,
            max_age_days: value.max_age_days,
            max_entries: value.max_entries,
            extensions: value.extensions.clone(),
        }
    }
}

impl From<&crate::Settings> for SettingsDto {
    fn from(value: &crate::Settings) -> Self {
        Self {
            version: value.version.0,
            locale: value.locale.into(),
            appearance: value.appearance.into(),
            default_profile_id: value
                .default_profile_id
                .map(|id| ProfileIdDto(id.to_string())),
            archive_defaults: (&value.archive_defaults).into(),
            execution: ExecutionSettingsDto {
                max_parallel_runs: value.execution.max_parallel_runs,
                extensions: value.execution.extensions.clone(),
            },
            history: HistorySettingsDto {
                runs: (&value.history.runs).into(),
                logs: (&value.history.logs).into(),
                extensions: value.history.extensions.clone(),
            },
            extensions: value.extensions.clone(),
        }
    }
}

impl From<crate::DiagnosticSeverity> for DiagnosticSeverityDto {
    fn from(value: crate::DiagnosticSeverity) -> Self {
        match value {
            crate::DiagnosticSeverity::Error => Self::Error,
            crate::DiagnosticSeverity::Warning => Self::Warning,
        }
    }
}

impl From<crate::DiagnosticCode> for DiagnosticCodeDto {
    fn from(value: crate::DiagnosticCode) -> Self {
        match value {
            crate::DiagnosticCode::InvalidMetadata => Self::InvalidMetadata,
            crate::DiagnosticCode::DuplicateMetadata => Self::DuplicateMetadata,
            crate::DiagnosticCode::InvalidRule => Self::InvalidRule,
            crate::DiagnosticCode::InvalidEscape => Self::InvalidEscape,
            crate::DiagnosticCode::UnterminatedCharacterClass => Self::UnterminatedCharacterClass,
            crate::DiagnosticCode::DuplicatePresetBlock => Self::DuplicatePresetBlock,
            crate::DiagnosticCode::UnterminatedPresetBlock => Self::UnterminatedPresetBlock,
        }
    }
}

impl From<&crate::ParserDiagnostic> for ParserDiagnosticDto {
    fn from(value: &crate::ParserDiagnostic) -> Self {
        Self {
            code: value.code.into(),
            severity: value.severity.into(),
            message: value.message.clone(),
            line: value.span.map(|span| span.start.line),
            start_column: value.span.map(|span| span.start.column),
            end_column: value.span.map(|span| span.end.column),
        }
    }
}

impl ProfileDto {
    /// Builds the editor DTO without moving parsed domain state.
    #[must_use]
    pub fn from_profile(
        profile: &crate::Profile,
        text: String,
        diagnostics: &[crate::ParserDiagnostic],
    ) -> Self {
        Self {
            version: profile.version.0,
            id: ProfileIdDto(profile.id.to_string()),
            name: profile.name.clone(),
            text,
            valid: diagnostics
                .iter()
                .all(|diagnostic| diagnostic.severity != crate::DiagnosticSeverity::Error),
            diagnostics: diagnostics.iter().map(Into::into).collect(),
        }
    }
}

impl From<crate::MatchDecision> for MatchDecisionDto {
    fn from(value: crate::MatchDecision) -> Self {
        match value {
            crate::MatchDecision::Include => Self::Include,
            crate::MatchDecision::Exclude => Self::Exclude,
        }
    }
}

impl From<&crate::MatchReason> for MatchReasonDto {
    fn from(value: &crate::MatchReason) -> Self {
        Self {
            profile_id: ProfileIdDto(value.profile_id.to_string()),
            line: value.line,
            original_rule: value.original_rule.clone(),
            preset_id: value
                .preset_id
                .as_ref()
                .map(|id| PresetIdDto(id.to_string())),
        }
    }
}

impl From<&crate::MatchResult> for MatchResultDto {
    fn from(value: &crate::MatchResult) -> Self {
        Self {
            path: value.path.clone(),
            decision: value.decision.into(),
            reason: value.reason.as_ref().map(Into::into),
        }
    }
}

impl From<crate::TaskState> for TaskStateDto {
    fn from(value: crate::TaskState) -> Self {
        match value {
            crate::TaskState::Ready => Self::Ready,
            crate::TaskState::Invalid => Self::Invalid,
            crate::TaskState::Disabled => Self::Disabled,
        }
    }
}

impl From<crate::RunState> for RunStateDto {
    fn from(value: crate::RunState) -> Self {
        match value {
            crate::RunState::Queued => Self::Queued,
            crate::RunState::Planning => Self::Planning,
            crate::RunState::Running => Self::Running,
            crate::RunState::Paused => Self::Paused,
            crate::RunState::Stopping => Self::Stopping,
            crate::RunState::Succeeded => Self::Succeeded,
            crate::RunState::SucceededWithWarnings => Self::SucceededWithWarnings,
            crate::RunState::Failed => Self::Failed,
            crate::RunState::Stopped => Self::Stopped,
            crate::RunState::Interrupted => Self::Interrupted,
        }
    }
}

impl From<crate::ProgressPhase> for ProgressPhaseDto {
    fn from(value: crate::ProgressPhase) -> Self {
        match value {
            crate::ProgressPhase::Planning => Self::Planning,
            crate::ProgressPhase::Archiving => Self::Archiving,
            crate::ProgressPhase::Verifying => Self::Verifying,
            crate::ProgressPhase::Publishing => Self::Publishing,
        }
    }
}

impl From<&crate::ProgressSnapshot> for ProgressSnapshotDto {
    fn from(value: &crate::ProgressSnapshot) -> Self {
        Self {
            phase: value.phase.into(),
            completed_entries: value.completed_entries.to_string(),
            total_entries: value.total_entries.map(|count| count.to_string()),
            completed_bytes: value.completed_bytes.to_string(),
            total_bytes: value.total_bytes.map(|count| count.to_string()),
            current_path: value.current_path.clone(),
        }
    }
}

impl From<&crate::FoldryWarning> for FoldryWarningDto {
    fn from(value: &crate::FoldryWarning) -> Self {
        Self {
            code: warning_code(value.code),
            message: value.message.clone(),
            path: value.path.clone(),
            extensions: value.extensions.clone(),
        }
    }
}

impl From<&crate::FoldryError> for FoldryErrorDto {
    fn from(value: &crate::FoldryError) -> Self {
        Self {
            code: error_code(value.code),
            message: value.message.clone(),
            retryable: value.retryable,
            path: value.path.clone(),
            extensions: value.extensions.clone(),
        }
    }
}

impl From<&crate::ArchiveArtifact> for ArchiveArtifactDto {
    fn from(value: &crate::ArchiveArtifact) -> Self {
        Self {
            path: value.path.to_string_lossy().into_owned(),
            size_bytes: value.size_bytes.to_string(),
            checksum_sha256: value.checksum_sha256.clone(),
        }
    }
}

impl From<&crate::ResultSummary> for ResultSummaryDto {
    fn from(value: &crate::ResultSummary) -> Self {
        Self {
            outcome: run_outcome(value.outcome),
            included_entries: value.included_entries.to_string(),
            skipped_entries: value.skipped_entries.to_string(),
            source_bytes: value.source_bytes.to_string(),
            duration_ms: value.duration_ms.to_string(),
            artifact: value.artifact.as_ref().map(Into::into),
            warnings: value.warnings.iter().map(Into::into).collect(),
            error: value.error.as_ref().map(Into::into),
        }
    }
}

impl From<&crate::RunEventKind> for RunEventKindDto {
    fn from(value: &crate::RunEventKind) -> Self {
        match value {
            crate::RunEventKind::StateChanged { state } => Self::StateChanged {
                state: (*state).into(),
            },
            crate::RunEventKind::Progress { progress } => Self::Progress {
                progress: progress.into(),
            },
            crate::RunEventKind::Warning { warning } => Self::Warning {
                warning: warning.into(),
            },
            crate::RunEventKind::Error { error } => Self::Error {
                error: error.into(),
            },
            crate::RunEventKind::Completed { summary } => Self::Completed {
                summary: summary.into(),
            },
        }
    }
}

impl From<&crate::RunEvent> for RunEventDto {
    fn from(value: &crate::RunEvent) -> Self {
        Self {
            version: value.version,
            run_id: RunIdDto(value.run_id.to_string()),
            task_id: TaskIdDto(value.task_id.to_string()),
            sequence: value.sequence.to_string(),
            occurred_at: value.occurred_at.to_string(),
            event: (&value.event).into(),
            extensions: value.extensions.clone(),
        }
    }
}

impl From<&crate::ValidationIssue> for ValidationIssueDto {
    fn from(value: &crate::ValidationIssue) -> Self {
        Self {
            code: validation_code(value.code),
            path: value.path.clone(),
            message: value.message.clone(),
        }
    }
}

impl From<&crate::ExecutionBlocker> for ExecutionBlockerDto {
    fn from(value: &crate::ExecutionBlocker) -> Self {
        Self {
            code: execution_blocker_code(value.code),
            path: value.path.clone(),
            message: value.message.clone(),
        }
    }
}

fn warning_code(code: crate::WarningCode) -> WarningCodeDto {
    match code {
        crate::WarningCode::ZipSymlinkPortability => WarningCodeDto::ZipSymlinkPortability,
        crate::WarningCode::JunctionSkipped => WarningCodeDto::JunctionSkipped,
        crate::WarningCode::SpecialFileSkipped => WarningCodeDto::SpecialFileSkipped,
        crate::WarningCode::UnreadableEntrySkipped => WarningCodeDto::UnreadableEntrySkipped,
        crate::WarningCode::SourceEntryChanged => WarningCodeDto::SourceEntryChanged,
    }
}

fn error_code(code: crate::ErrorCode) -> ErrorCodeDto {
    match code {
        crate::ErrorCode::InvalidConfiguration => ErrorCodeDto::InvalidConfiguration,
        crate::ErrorCode::InvalidProfile => ErrorCodeDto::InvalidProfile,
        crate::ErrorCode::UnsupportedAction => ErrorCodeDto::UnsupportedAction,
        crate::ErrorCode::SourceUnavailable => ErrorCodeDto::SourceUnavailable,
        crate::ErrorCode::OutputUnavailable => ErrorCodeDto::OutputUnavailable,
        crate::ErrorCode::OutputConflict => ErrorCodeDto::OutputConflict,
        crate::ErrorCode::ReadFailed => ErrorCodeDto::ReadFailed,
        crate::ErrorCode::WriteFailed => ErrorCodeDto::WriteFailed,
        crate::ErrorCode::VerificationFailed => ErrorCodeDto::VerificationFailed,
        crate::ErrorCode::Cancelled => ErrorCodeDto::Cancelled,
        crate::ErrorCode::Internal => ErrorCodeDto::Internal,
    }
}

fn run_outcome(outcome: crate::RunOutcome) -> RunOutcomeDto {
    match outcome {
        crate::RunOutcome::Succeeded => RunOutcomeDto::Succeeded,
        crate::RunOutcome::SucceededWithWarnings => RunOutcomeDto::SucceededWithWarnings,
        crate::RunOutcome::Failed => RunOutcomeDto::Failed,
        crate::RunOutcome::Stopped => RunOutcomeDto::Stopped,
        crate::RunOutcome::Interrupted => RunOutcomeDto::Interrupted,
    }
}

fn validation_code(code: crate::ValidationCode) -> ValidationCodeDto {
    match code {
        crate::ValidationCode::UnsupportedDocumentVersion => {
            ValidationCodeDto::UnsupportedDocumentVersion
        }
        crate::ValidationCode::EmptyName => ValidationCodeDto::EmptyName,
        crate::ValidationCode::EmptySource => ValidationCodeDto::EmptySource,
        crate::ValidationCode::DuplicateTaskId => ValidationCodeDto::DuplicateTaskId,
        crate::ValidationCode::DuplicateSource => ValidationCodeDto::DuplicateSource,
        crate::ValidationCode::InvalidStepCount => ValidationCodeDto::InvalidStepCount,
        crate::ValidationCode::EmptyOutputDirectory => ValidationCodeDto::EmptyOutputDirectory,
        crate::ValidationCode::EmptyOutputFilename => ValidationCodeDto::EmptyOutputFilename,
        crate::ValidationCode::ReservedExtensionField => ValidationCodeDto::ReservedExtensionField,
        crate::ValidationCode::InvalidParallelRuns => ValidationCodeDto::InvalidParallelRuns,
        crate::ValidationCode::InvalidRetention => ValidationCodeDto::InvalidRetention,
    }
}

fn execution_blocker_code(code: crate::ExecutionBlockerCode) -> ExecutionBlockerCodeDto {
    match code {
        crate::ExecutionBlockerCode::UnsupportedActionType => {
            ExecutionBlockerCodeDto::UnsupportedActionType
        }
        crate::ExecutionBlockerCode::UnsupportedActionVersion => {
            ExecutionBlockerCodeDto::UnsupportedActionVersion
        }
    }
}

impl From<&crate::StoredProfile> for StoredProfileDto {
    fn from(value: &crate::StoredProfile) -> Self {
        Self {
            id: value.id.map(|id| ProfileIdDto(id.to_string())),
            filename: value
                .path
                .file_name()
                .map_or_else(String::new, |name| name.to_string_lossy().into_owned()),
            name: value.name.clone(),
            text: value.text.clone(),
            valid: value.valid,
            diagnostics: value.diagnostics.iter().map(Into::into).collect(),
        }
    }
}

impl From<&crate::StoredPreset> for StoredPresetDto {
    fn from(value: &crate::StoredPreset) -> Self {
        Self {
            id: PresetIdDto(value.id.to_string()),
            filename: value
                .path
                .file_name()
                .map_or_else(String::new, |name| name.to_string_lossy().into_owned()),
            text: value.text.clone(),
            resource_version: value.resource_version,
        }
    }
}

impl From<&crate::RunSnapshot> for RunSnapshotDto {
    fn from(value: &crate::RunSnapshot) -> Self {
        Self {
            task: (&value.task).into(),
            settings: (&value.settings).into(),
            profile_hash: value.profile_hash.clone(),
        }
    }
}

impl From<&crate::RunRecord> for RunRecordDto {
    fn from(value: &crate::RunRecord) -> Self {
        Self {
            run_id: RunIdDto(value.run_id.to_string()),
            task_id: TaskIdDto(value.task_id.to_string()),
            state: value.state.into(),
            started_at: value.started_at.to_string(),
            finished_at: value.finished_at.map(|timestamp| timestamp.to_string()),
            snapshot: (&value.snapshot).into(),
            summary: value.summary.as_ref().map(Into::into),
        }
    }
}

impl From<crate::LogLevel> for LogLevelDto {
    fn from(value: crate::LogLevel) -> Self {
        match value {
            crate::LogLevel::Trace => Self::Trace,
            crate::LogLevel::Debug => Self::Debug,
            crate::LogLevel::Info => Self::Info,
            crate::LogLevel::Warn => Self::Warn,
            crate::LogLevel::Error => Self::Error,
        }
    }
}

impl From<&crate::LogRecord> for LogRecordDto {
    fn from(value: &crate::LogRecord) -> Self {
        Self {
            run_id: RunIdDto(value.run_id.to_string()),
            sequence: value.sequence.to_string(),
            occurred_at: value.occurred_at.to_string(),
            level: value.level.into(),
            message: value.message.clone(),
            path: value.path.clone(),
        }
    }
}

impl TryFrom<SettingsDto> for crate::Settings {
    type Error = String;

    fn try_from(value: SettingsDto) -> Result<Self, Self::Error> {
        Ok(Self {
            version: crate::SettingsVersion(value.version),
            locale: match value.locale {
                LocaleDto::En => crate::Locale::En,
                LocaleDto::Ru => crate::Locale::Ru,
            },
            appearance: match value.appearance {
                AppearanceDto::System => crate::Appearance::System,
                AppearanceDto::Light => crate::Appearance::Light,
                AppearanceDto::Dark => crate::Appearance::Dark,
            },
            default_profile_id: value
                .default_profile_id
                .map(|id| parse_id("profile", &id.0))
                .transpose()?,
            archive_defaults: crate::ArchiveDefaults {
                output_directory: PathBuf::from(value.archive_defaults.output_directory),
                format: value.archive_defaults.format.into(),
                compression: value.archive_defaults.compression.into(),
                conflict_policy: value.archive_defaults.conflict_policy.into(),
                include_root: value.archive_defaults.include_root,
                unreadable_policy: value.archive_defaults.unreadable_policy.into(),
                verification_mode: value.archive_defaults.verification_mode.into(),
                checksum: value.archive_defaults.checksum.into(),
                extensions: value.archive_defaults.extensions,
            },
            execution: crate::ExecutionSettings {
                max_parallel_runs: value.execution.max_parallel_runs,
                extensions: value.execution.extensions,
            },
            history: crate::HistorySettings {
                runs: retention_from_dto(value.history.runs),
                logs: retention_from_dto(value.history.logs),
                extensions: value.history.extensions,
            },
            extensions: value.extensions,
        })
    }
}

impl TryFrom<PlanDto> for crate::Plan {
    type Error = String;

    fn try_from(value: PlanDto) -> Result<Self, Self::Error> {
        Ok(Self {
            version: crate::PlanVersion(value.version),
            name: value.name,
            tasks: value
                .tasks
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
            extensions: value.extensions,
        })
    }
}

impl TryFrom<TaskDto> for crate::Task {
    type Error = String;

    fn try_from(value: TaskDto) -> Result<Self, Self::Error> {
        Ok(Self {
            id: parse_id("task", &value.id.0)?,
            source: PathBuf::from(value.source),
            enabled: value.enabled,
            profile_id: parse_id("profile", &value.profile_id.0)?,
            steps: value
                .steps
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
            extensions: value.extensions,
        })
    }
}

impl TryFrom<ActionSpecDto> for crate::ActionSpec {
    type Error = String;

    fn try_from(value: ActionSpecDto) -> Result<Self, Self::Error> {
        if value.action_type == "archive" {
            if !value.fields.is_empty() {
                return Err("archive action cannot contain unsupported fields".into());
            }
            let archive = value
                .archive
                .ok_or_else(|| "archive action payload is missing".to_owned())?;
            if value
                .version
                .is_some_and(|version| version != archive.version)
            {
                return Err("archive action versions disagree".into());
            }
            return Ok(Self::Archive(archive.into()));
        }
        if value.archive.is_some() {
            return Err("unsupported action cannot contain an archive payload".into());
        }
        if value.action_type.trim().is_empty() {
            return Err("action type cannot be empty".into());
        }
        Ok(Self::Unsupported(crate::UnsupportedActionSpec {
            action_type: value.action_type,
            version: value.version,
            fields: value.fields,
        }))
    }
}

impl From<ArchiveActionSpecDto> for crate::ArchiveActionSpec {
    fn from(value: ArchiveActionSpecDto) -> Self {
        Self {
            version: crate::ActionVersion(value.version),
            output: crate::ArchiveOutputSpec {
                directory: PathBuf::from(value.output.directory),
                filename: value.output.filename,
                format: value.output.format.into(),
                compression: value.output.compression.into(),
                conflict_policy: value.output.conflict_policy.into(),
                extensions: value.output.extensions,
            },
            include_root: value.include_root,
            unreadable_policy: value.unreadable_policy.into(),
            verification: crate::VerificationSpec {
                mode: value.verification.mode.into(),
                checksum: value.verification.checksum.into(),
                extensions: value.verification.extensions,
            },
            extensions: value.extensions,
        }
    }
}

impl From<ArchiveFormatDto> for crate::ArchiveFormat {
    fn from(value: ArchiveFormatDto) -> Self {
        match value {
            ArchiveFormatDto::Zip => Self::Zip,
            ArchiveFormatDto::TarGz => Self::TarGz,
            ArchiveFormatDto::TarZst => Self::TarZst,
        }
    }
}

impl From<CompressionLevelDto> for crate::CompressionLevel {
    fn from(value: CompressionLevelDto) -> Self {
        match value {
            CompressionLevelDto::Fast => Self::Fast,
            CompressionLevelDto::Balanced => Self::Balanced,
            CompressionLevelDto::Maximum => Self::Maximum,
        }
    }
}

impl From<ConflictPolicyDto> for crate::ConflictPolicy {
    fn from(value: ConflictPolicyDto) -> Self {
        match value {
            ConflictPolicyDto::Skip => Self::Skip,
            ConflictPolicyDto::Overwrite => Self::Overwrite,
            ConflictPolicyDto::Increment => Self::Increment,
        }
    }
}

impl From<UnreadablePolicyDto> for crate::UnreadablePolicy {
    fn from(value: UnreadablePolicyDto) -> Self {
        match value {
            UnreadablePolicyDto::Fail => Self::Fail,
            UnreadablePolicyDto::WarnAndSkip => Self::WarnAndSkip,
        }
    }
}

impl From<VerificationModeDto> for crate::VerificationMode {
    fn from(value: VerificationModeDto) -> Self {
        match value {
            VerificationModeDto::Structural => Self::Structural,
            VerificationModeDto::Full => Self::Full,
        }
    }
}

impl From<ChecksumAlgorithmDto> for crate::ChecksumAlgorithm {
    fn from(value: ChecksumAlgorithmDto) -> Self {
        match value {
            ChecksumAlgorithmDto::None => Self::None,
            ChecksumAlgorithmDto::Sha256 => Self::Sha256,
        }
    }
}

fn retention_from_dto(value: RetentionPolicyDto) -> crate::RetentionPolicy {
    crate::RetentionPolicy {
        unlimited: value.unlimited,
        max_age_days: value.max_age_days,
        max_entries: value.max_entries,
        extensions: value.extensions,
    }
}

fn parse_id<T>(kind: &str, value: &str) -> Result<T, String>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    value
        .parse()
        .map_err(|error| format!("invalid {kind} ID: {error}"))
}

/// Generates one deterministic, reviewable TypeScript contract file.
#[must_use]
pub fn typescript_bindings() -> String {
    let config = Config::default();
    let mut output = String::from(CONTRACTS_FILE_HEADER);
    output.push('\n');
    output.push_str(
        "export type JsonValue = null | boolean | number | string | Array<JsonValue> | { [key: string]: JsonValue };\n\n",
    );

    macro_rules! declaration {
        ($type:ty) => {{
            output.push_str("export ");
            output.push_str(&<$type as TS>::decl(&config));
            output.push_str("\n\n");
        }};
    }

    declaration!(ProfileIdDto);
    declaration!(PresetIdDto);
    declaration!(TaskIdDto);
    declaration!(RunIdDto);
    declaration!(ArchiveFormatDto);
    declaration!(CompressionLevelDto);
    declaration!(ConflictPolicyDto);
    declaration!(UnreadablePolicyDto);
    declaration!(VerificationModeDto);
    declaration!(ChecksumAlgorithmDto);
    declaration!(ArchiveOutputSpecDto);
    declaration!(VerificationSpecDto);
    declaration!(ArchiveActionSpecDto);
    declaration!(ActionSpecDto);
    declaration!(TaskDto);
    declaration!(PlanDto);
    declaration!(LocaleDto);
    declaration!(AppearanceDto);
    declaration!(ArchiveDefaultsDto);
    declaration!(ExecutionSettingsDto);
    declaration!(RetentionPolicyDto);
    declaration!(HistorySettingsDto);
    declaration!(SettingsDto);
    declaration!(DiagnosticSeverityDto);
    declaration!(DiagnosticCodeDto);
    declaration!(ParserDiagnosticDto);
    declaration!(ProfileDto);
    declaration!(MatchDecisionDto);
    declaration!(MatchReasonDto);
    declaration!(MatchResultDto);
    declaration!(FileSystemObjectKindDto);
    declaration!(BrowserRootKindDto);
    declaration!(BrowserRootDto);
    declaration!(BrowserNodeDto);
    declaration!(ScanDispositionDto);
    declaration!(PreviewEntryDto);
    declaration!(ScanSummaryDto);
    declaration!(PreviewFilterDto);
    declaration!(PreviewSnapshotDto);
    declaration!(PreviewPageDto);
    declaration!(TaskStateDto);
    declaration!(RunStateDto);
    declaration!(ProgressPhaseDto);
    declaration!(ProgressSnapshotDto);
    declaration!(WarningCodeDto);
    declaration!(ErrorCodeDto);
    declaration!(FoldryWarningDto);
    declaration!(FoldryErrorDto);
    declaration!(ArchiveArtifactDto);
    declaration!(RunOutcomeDto);
    declaration!(ResultSummaryDto);
    declaration!(RunEventKindDto);
    declaration!(RunEventDto);
    declaration!(ValidationCodeDto);
    declaration!(ValidationIssueDto);
    declaration!(ExecutionBlockerCodeDto);
    declaration!(ExecutionBlockerDto);
    declaration!(StoredProfileDto);
    declaration!(StoredPresetDto);
    declaration!(RunSnapshotDto);
    declaration!(RunRecordDto);
    declaration!(LogLevelDto);
    declaration!(LogRecordDto);
    declaration!(StoragePathsDto);
    declaration!(BootstrapSnapshotDto);
    declaration!(BrowserChildrenDto);
    declaration!(PreviewStartedDto);
    declaration!(TaskAddResultDto);
    declaration!(IpcErrorDto);

    output
}

#[cfg(test)]
mod tests {
    use crate::{Extensions, Plan, PlanVersion, Settings};

    use super::{PlanDto, SettingsDto, typescript_bindings};

    #[test]
    fn generated_contracts_are_stable_and_exported() {
        let bindings = typescript_bindings();

        assert!(bindings.contains("export type Plan ="));
        assert!(bindings.contains("export type Settings ="));
        assert!(bindings.contains("export type RunEvent ="));
        assert!(bindings.contains("export type BrowserNode ="));
        assert!(bindings.contains("export type PreviewPage ="));
        assert!(bindings.contains("export type BootstrapSnapshot ="));
        assert!(bindings.contains("export type IpcError ="));
        assert!(bindings.ends_with("\n\n"));
    }

    #[test]
    fn desktop_settings_input_round_trips_to_the_application_model() {
        let settings = Settings::default();
        let restored = Settings::try_from(SettingsDto::from(&settings)).expect("settings input");

        assert_eq!(restored, settings);
    }

    #[test]
    fn desktop_plan_input_round_trips_to_the_application_model() {
        let plan = Plan {
            version: PlanVersion::CURRENT,
            name: "Active plan".into(),
            tasks: Vec::new(),
            extensions: Extensions::new(),
        };
        let restored = Plan::try_from(PlanDto::from(&plan)).expect("plan input");

        assert_eq!(restored, plan);
    }
}
