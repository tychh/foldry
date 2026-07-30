use std::path::PathBuf;

use foldry_core::{ActionId, Extensions, FolderId, RunId};
use jiff::Timestamp;
use serde::{Deserialize, Serialize};

/// Configuration-time folder readiness.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FolderState {
    Ready,
    Invalid,
    Disabled,
}

/// Runtime scheduler state.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
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

/// Coarse phase used for bounded progress updates.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgressPhase {
    Planning,
    Archiving,
    Verifying,
    Publishing,
}

/// Aggregated run progress; detailed logs are loaded separately.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ProgressSnapshot {
    pub phase: ProgressPhase,
    pub completed_entries: u64,
    pub total_entries: Option<u64>,
    pub completed_bytes: u64,
    pub total_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_path: Option<String>,
}

/// Stable non-fatal outcome category.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WarningCode {
    ZipSymlinkPortability,
    JunctionSkipped,
    SpecialFileSkipped,
    UnreadableEntrySkipped,
    SourceEntryChanged,
}

/// Stable fatal error category shared by CLI and GUI.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
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

/// One warning with optional source/output path context.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct FoldryWarning {
    pub code: WarningCode,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Extensions::is_empty", flatten)]
    pub extensions: Extensions,
}

/// One typed failure suitable for localized UI and English machine output.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct FoldryError {
    pub code: ErrorCode,
    pub message: String,
    pub retryable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Extensions::is_empty", flatten)]
    pub extensions: Extensions,
}

/// Published archive information.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ArchiveArtifact {
    pub path: PathBuf,
    pub size_bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checksum_sha256: Option<String>,
}

/// Final run outcome.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunOutcome {
    Succeeded,
    SucceededWithWarnings,
    Failed,
    Stopped,
    Interrupted,
}

/// Bounded final summary retained in history.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ResultSummary {
    pub outcome: RunOutcome,
    pub included_entries: u64,
    pub skipped_entries: u64,
    pub source_bytes: u64,
    pub duration_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<ArchiveArtifact>,
    pub warnings: Vec<FoldryWarning>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<FoldryError>,
}

/// Immediate state/event stream payload. Progress producers aggregate separately.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RunEventKind {
    StateChanged { state: RunState },
    Progress { progress: ProgressSnapshot },
    Warning { warning: FoldryWarning },
    Error { error: FoldryError },
    Completed { summary: ResultSummary },
}

/// Versioned event envelope shared by CLI JSON and Tauri.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RunEvent {
    pub version: u16,
    pub run_id: RunId,
    pub folder_id: FolderId,
    pub action_id: ActionId,
    pub sequence: u64,
    pub occurred_at: Timestamp,
    pub event: RunEventKind,
    #[serde(default, skip_serializing_if = "Extensions::is_empty", flatten)]
    pub extensions: Extensions,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_envelope_round_trips_with_an_rfc3339_timestamp() {
        let event = RunEvent {
            version: 1,
            run_id: RunId::new(),
            folder_id: FolderId::new(),
            action_id: ActionId::new(),
            sequence: 7,
            occurred_at: "2026-07-27T00:00:00Z".parse().unwrap(),
            event: RunEventKind::StateChanged {
                state: RunState::Running,
            },
            extensions: Extensions::new(),
        };

        let encoded = serde_json::to_string(&event).unwrap();
        let decoded = serde_json::from_str::<RunEvent>(&encoded).unwrap();

        assert_eq!(decoded, event);
        assert!(encoded.contains("\"type\":\"state_changed\""));
    }
}
