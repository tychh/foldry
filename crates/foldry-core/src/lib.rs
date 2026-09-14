#![forbid(unsafe_code)]

mod archive;
mod browser;
mod diagnostics;
mod execution;
mod filesystem;
mod ids;
mod matcher;
mod output;
mod parser;
mod preset;
mod profile;
mod writer;

pub use archive::{
    ARCHIVE_FORMAT_CAPABILITIES, ActionVersion, ArchiveActionSpec, ArchiveFormat,
    ArchiveFormatCapabilities, ArchiveOutputDirectory, ArchiveOutputSpec, ChecksumAlgorithm,
    CompressionLevel, ConflictPolicy, UnreadablePolicy, VerificationMode, VerificationSpec,
};
pub use browser::{
    BrowserError, BrowserNode, BrowserRoot, BrowserRootKind, BrowserSize, FileSystemBrowser,
};
pub use diagnostics::{
    DiagnosticCode, DiagnosticSeverity, ParserDiagnostic, SourceLocation, SourceSpan,
};
pub use execution::{
    ExecutionControl, ExecutionEntrySource, ExecutionError, ExecutionPhase, ExecutionPlan,
    ExecutionProgress, ExecutionResult, ExecutionWarning, execute_archive,
};
pub use filesystem::{
    CancellationToken, CaseSensitivityConfidence, DetectedCaseSensitivity, FileSystemObjectKind,
    FileSystemScanner, ScanDisposition, ScanError, ScanNotice, ScanNoticeCode, ScanSink,
    ScanSinkError, ScanSummary, ScannedEntry, detect_case_sensitivity,
};
pub use ids::{ActionId, FolderId, IdParseError, PresetId, PresetIdParseError, ProfileId, RunId};
pub use matcher::{
    CompiledProfile, EffectiveProfileError, FileSystemCaseSensitivity, MatchPathError,
    normalize_relative_path, resolve_effective_profile,
};
pub use output::{
    LEGACY_RESERVATION_METADATA_VERSION, OutputPlanError, OutputReservation, PlanOutput,
    RESERVATION_METADATA_VERSION, ReservationMetadata, ReservationOwner, ReservationOwnerKind,
    reserve_output, reserve_output_owned,
};
pub use parser::{ProfileParseResult, parse_profile};
pub use preset::{
    ModifiedBlockConfirmation, PresetCatalog, PresetCatalogError, PresetDefinition,
    PresetEditError, PresetState, PresetVersion, SensitivePresetApproval, normalize_preset_content,
    preset_content_hash,
};
pub use profile::{
    EffectiveProfileSnapshot, MatchDecision, MatchReason, MatchResult, Profile,
    ProfileFormatVersion, ProfileRule, ResolvedGitignore, RulePattern, RuleSource,
};
pub use writer::{ArchiveWriteError, ArchiveWriterBackend, codec_level, create_archive_writer};

use std::collections::BTreeMap;

use serde_json::Value;

/// Forward-compatible fields preserved by versioned public contracts.
pub type Extensions = BTreeMap<String, Value>;

/// Human-readable status retained for the bootstrap CLI and desktop command.
#[must_use]
pub const fn workspace_status() -> &'static str {
    "Workspace ready"
}

#[cfg(test)]
mod tests {
    use super::workspace_status;

    #[test]
    fn reports_a_ready_workspace() {
        assert_eq!(workspace_status(), "Workspace ready");
    }
}
