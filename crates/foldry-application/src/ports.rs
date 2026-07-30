use std::{fmt, path::PathBuf};

use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use crate::{
    ActionId, FolderAction, FolderId, ParserDiagnostic, Plan, PresetId, ProfileId, ResultSummary,
    RunId, RunState, Settings,
};

pub const DEFAULT_PROFILE_FILENAME: &str = "default.packignore";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepositoryError {
    pub message: String,
}

impl RepositoryError {
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for RepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for RepositoryError {}

#[derive(Clone, Debug, PartialEq)]
pub struct StoredProfile {
    pub id: Option<ProfileId>,
    pub path: PathBuf,
    pub name: String,
    pub text: String,
    pub valid: bool,
    pub diagnostics: Vec<ParserDiagnostic>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredPreset {
    pub id: PresetId,
    pub path: PathBuf,
    pub text: String,
    pub resource_version: Option<u16>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct FolderSnapshot {
    pub id: FolderId,
    pub source: PathBuf,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RunSnapshot {
    pub folder: FolderSnapshot,
    pub action: FolderAction,
    pub effective_profile_id: ProfileId,
    pub settings: Settings,
    pub profile_text: String,
    pub profile_hash: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RunRecord {
    pub run_id: RunId,
    pub folder_id: FolderId,
    pub action_id: ActionId,
    pub state: RunState,
    pub started_at: Timestamp,
    pub finished_at: Option<Timestamp>,
    pub snapshot: RunSnapshot,
    pub summary: Option<ResultSummary>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LogRecord {
    pub run_id: RunId,
    pub sequence: u64,
    pub occurred_at: Timestamp,
    pub level: LogLevel,
    pub message: String,
    pub path: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PageRequest {
    pub offset: u64,
    pub limit: u32,
}

pub trait SettingsRepository: Send + Sync {
    fn load(&self) -> Result<Option<Settings>, RepositoryError>;
    fn save(&self, settings: &Settings) -> Result<(), RepositoryError>;
}

pub trait ActivePlanRepository: Send + Sync {
    fn load(&self) -> Result<Option<Plan>, RepositoryError>;
    fn save(&self, plan: &Plan) -> Result<(), RepositoryError>;
}

pub trait ProfileRepository: Send + Sync {
    fn list(&self) -> Result<Vec<StoredProfile>, RepositoryError>;
    fn get(&self, id: ProfileId) -> Result<Option<StoredProfile>, RepositoryError>;
    fn save_text(&self, filename: &str, text: &str) -> Result<StoredProfile, RepositoryError>;
    fn delete(&self, id: ProfileId) -> Result<bool, RepositoryError>;
    fn restore_default(&self) -> Result<StoredProfile, RepositoryError>;
}

pub trait PresetRepository: Send + Sync {
    fn list(&self) -> Result<Vec<StoredPreset>, RepositoryError>;
    fn save_text(&self, filename: &str, text: &str) -> Result<StoredPreset, RepositoryError>;
    fn delete(&self, id: &PresetId) -> Result<bool, RepositoryError>;
    fn reset_from_resources(&self, id: &PresetId) -> Result<StoredPreset, RepositoryError>;
}

pub trait RunHistoryRepository: Send + Sync {
    fn insert(&self, run: &RunRecord) -> Result<(), RepositoryError>;
    fn update(&self, run: &RunRecord) -> Result<(), RepositoryError>;
    fn get(&self, run_id: RunId) -> Result<Option<RunRecord>, RepositoryError>;
    fn page_filtered(
        &self,
        page: PageRequest,
        folder_id: Option<FolderId>,
        action_id: Option<ActionId>,
    ) -> Result<Vec<RunRecord>, RepositoryError>;
    fn page(&self, page: PageRequest) -> Result<Vec<RunRecord>, RepositoryError> {
        self.page_filtered(page, None, None)
    }
    fn non_terminal_for_folder(
        &self,
        folder_id: FolderId,
    ) -> Result<Vec<RunRecord>, RepositoryError>;
    fn non_terminal_for_action(
        &self,
        folder_id: FolderId,
        action_id: ActionId,
    ) -> Result<Vec<RunRecord>, RepositoryError>;
    fn mark_unfinished_interrupted(&self, at: Timestamp) -> Result<u64, RepositoryError>;
    fn apply_retention(
        &self,
        now: Timestamp,
        max_age_days: u32,
        max_entries: u32,
        unlimited: bool,
    ) -> Result<u64, RepositoryError>;
}

pub trait LogRepository: Send + Sync {
    fn append(&self, record: &LogRecord) -> Result<(), RepositoryError>;
    fn page(&self, run_id: RunId, page: PageRequest) -> Result<Vec<LogRecord>, RepositoryError>;
    fn apply_retention(
        &self,
        now: Timestamp,
        max_age_days: u32,
        max_runs: u32,
        unlimited: bool,
    ) -> Result<u64, RepositoryError>;
}

pub trait Clock: Send + Sync {
    fn now(&self) -> Timestamp;
}

pub trait IdGenerator: Send + Sync {
    fn run_id(&self) -> RunId;
    fn folder_id(&self) -> FolderId;
    fn action_id(&self) -> ActionId;
    fn profile_id(&self) -> ProfileId;
}
