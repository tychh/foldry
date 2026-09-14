#![forbid(unsafe_code)]

mod plan;
mod ports;
mod preview;
mod request;
mod run;
mod scheduler;
mod services;
mod settings;
pub mod transport;
mod validation;

pub use foldry_core::{
    ARCHIVE_FORMAT_CAPABILITIES, ActionId, ActionVersion, ArchiveActionSpec, ArchiveFormat,
    ArchiveFormatCapabilities, ArchiveOutputDirectory, ArchiveOutputSpec, BrowserError,
    BrowserNode, BrowserRoot, BrowserRootKind, BrowserSize, CancellationToken,
    CaseSensitivityConfidence, ChecksumAlgorithm, CompiledProfile, CompressionLevel,
    ConflictPolicy, DetectedCaseSensitivity, DiagnosticCode, DiagnosticSeverity,
    EffectiveProfileSnapshot, ExecutionControl, ExecutionEntrySource, ExecutionError,
    ExecutionPhase, ExecutionPlan, ExecutionProgress, ExecutionResult, ExecutionWarning,
    Extensions, FileSystemBrowser, FileSystemCaseSensitivity, FileSystemObjectKind,
    FileSystemScanner, FolderId, LEGACY_RESERVATION_METADATA_VERSION, MatchDecision, MatchReason,
    MatchResult, ModifiedBlockConfirmation, OutputPlanError, OutputReservation, ParserDiagnostic,
    PlanOutput, PresetCatalog, PresetCatalogError, PresetDefinition, PresetEditError, PresetId,
    PresetState, PresetVersion, Profile, ProfileFormatVersion, ProfileId, ProfileRule,
    RESERVATION_METADATA_VERSION, ReservationMetadata, ReservationOwner, ReservationOwnerKind,
    ResolvedGitignore, RulePattern, RuleSource, RunId, ScanDisposition, ScanError, ScanNotice,
    ScanNoticeCode, ScanSink, ScanSinkError, ScanSummary, ScannedEntry, SensitivePresetApproval,
    SourceLocation, SourceSpan, UnreadablePolicy, VerificationMode, VerificationSpec,
    detect_case_sensitivity, execute_archive, normalize_preset_content, parse_profile,
    preset_content_hash, reserve_output, reserve_output_owned, resolve_effective_profile,
};
pub use plan::{
    ActionSpec, Folder, FolderAction, Plan, PlanVersion, UnsupportedActionSpec,
    validate_filename_template,
};
pub use ports::{
    ActionCheckpoint, ActionOperationalState, ActivePlanRepository, Clock,
    DEFAULT_PROFILE_FILENAME, FolderSnapshot, IdGenerator, LogLevel, LogRecord, LogRepository,
    OutputDirectoryRegistry, PageRequest, PresetRepository, ProfileRepository, RepositoryError,
    RunHistoryRepository, RunRecord, RunSnapshot, SettingsRepository, SourceFingerprintSummary,
    StoredPreset, StoredProfile, TerminalRunCommit,
};
pub use preview::{PreviewCache, PreviewCacheKey, PreviewFilter, PreviewKeyError, PreviewSnapshot};
pub use request::{LatestRequest, LatestRequestRegistry};
pub use run::{
    ArchiveArtifact, ErrorCode, FolderState, FoldryError, FoldryWarning, ProgressPhase,
    ProgressSnapshot, ResultSummary, RunEvent, RunEventKind, RunOutcome, RunState, SkipReason,
    WarningCode,
};
pub use scheduler::{
    NoopRunEventSink, RunEventSink, RunExecutor, RunReporter, ScheduledRun, Scheduler,
    SchedulerError, SchedulerPorts, SchedulerSnapshot, is_terminal, validate_transition,
};
pub use services::{
    ApplicationPorts, ApplicationServices, ApplicationState, ChangeState, FolderAvailability,
    FolderOperationalSummary, PreviewRequest, RetentionReport, SystemClock, UseCaseError,
    UuidIdGenerator,
};
pub use settings::{
    Appearance, ArchiveDefaults, BrowserSettings, BrowserView, ExecutionSettings, FolderSortMode,
    HistorySettings, Locale, RetentionPolicy, Settings, SettingsVersion,
};
pub use transport::{CONTRACTS_FILE_HEADER, typescript_bindings};
pub use validation::{
    ContractValidation, ExecutionBlocker, ExecutionBlockerCode, ValidationCode, ValidationIssue,
};

/// Returns the current application bootstrap status.
#[must_use]
pub const fn workspace_status() -> &'static str {
    foldry_core::workspace_status()
}

#[cfg(test)]
mod tests {
    use super::workspace_status;

    #[test]
    fn delegates_status_to_core() {
        assert_eq!(workspace_status(), foldry_core::workspace_status());
    }
}
