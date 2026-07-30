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
    ActionId, ActionVersion, ArchiveActionSpec, ArchiveFormat, ArchiveOutputDirectory,
    ArchiveOutputSpec, BrowserError, BrowserNode, BrowserRoot, BrowserRootKind, BrowserSize,
    CancellationToken, CaseSensitivityConfidence, ChecksumAlgorithm, CompiledProfile,
    CompressionLevel, ConflictPolicy, DetectedCaseSensitivity, DiagnosticCode, DiagnosticSeverity,
    ExecutionControl, ExecutionEntrySource, ExecutionError, ExecutionPlan, ExecutionProgress,
    ExecutionResult, ExecutionWarning, Extensions, FileSystemBrowser, FileSystemCaseSensitivity,
    FileSystemObjectKind, FileSystemScanner, FolderId, MatchDecision, MatchReason, MatchResult,
    ModifiedBlockConfirmation, OutputPlanError, OutputReservation, ParserDiagnostic, PlanOutput,
    PresetCatalog, PresetCatalogError, PresetDefinition, PresetEditError, PresetId, PresetState,
    PresetVersion, Profile, ProfileFormatVersion, ProfileId, ProfileRule,
    RESERVATION_METADATA_VERSION, ReservationMetadata, RulePattern, RunId, ScanDisposition,
    ScanError, ScanNotice, ScanNoticeCode, ScanSink, ScanSinkError, ScanSummary, ScannedEntry,
    SensitivePresetApproval, SourceLocation, SourceSpan, UnreadablePolicy, VerificationMode,
    VerificationSpec, detect_case_sensitivity, execute_archive, normalize_preset_content,
    parse_profile, preset_content_hash, reserve_output,
};
pub use plan::{
    ActionSpec, Folder, FolderAction, Plan, PlanVersion, UnsupportedActionSpec,
    validate_filename_template,
};
pub use ports::{
    ActivePlanRepository, Clock, DEFAULT_PROFILE_FILENAME, FolderSnapshot, IdGenerator, LogLevel,
    LogRecord, LogRepository, PageRequest, PresetRepository, ProfileRepository, RepositoryError,
    RunHistoryRepository, RunRecord, RunSnapshot, SettingsRepository, StoredPreset, StoredProfile,
};
pub use preview::{PreviewCache, PreviewCacheKey, PreviewFilter, PreviewKeyError, PreviewSnapshot};
pub use request::{LatestRequest, LatestRequestRegistry};
pub use run::{
    ArchiveArtifact, ErrorCode, FolderState, FoldryError, FoldryWarning, ProgressPhase,
    ProgressSnapshot, ResultSummary, RunEvent, RunEventKind, RunOutcome, RunState, WarningCode,
};
pub use scheduler::{
    NoopRunEventSink, RunEventSink, RunExecutor, RunReporter, Scheduler, SchedulerError,
    SchedulerPorts, is_terminal, validate_transition,
};
pub use services::{
    ApplicationPorts, ApplicationServices, ApplicationState, PreviewRequest, RetentionReport,
    SystemClock, UseCaseError, UuidIdGenerator,
};
pub use settings::{
    Appearance, ArchiveDefaults, BrowserSettings, BrowserView, ExecutionSettings, HistorySettings,
    Locale, RetentionPolicy, Settings, SettingsVersion,
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
