use std::{
    fmt, fs,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
};

use jiff::Timestamp;
use sha2::{Digest, Sha256};

use crate::{
    ActionSpec, ActivePlanRepository, Clock, ContractValidation, Extensions, IdGenerator,
    LogRecord, LogRepository, PageRequest, ParserDiagnostic, Plan, PlanVersion, PresetId,
    PresetRepository, ProfileId, ProfileRepository, RepositoryError, RunHistoryRepository, RunId,
    RunRecord, RunSnapshot, RunState, Settings, SettingsRepository, StoredPreset, StoredProfile,
    Task, TaskId,
};

pub struct ApplicationPorts {
    pub settings: Box<dyn SettingsRepository>,
    pub active_plan: Box<dyn ActivePlanRepository>,
    pub profiles: Box<dyn ProfileRepository>,
    pub presets: Box<dyn PresetRepository>,
    pub history: Box<dyn RunHistoryRepository>,
    pub logs: Box<dyn LogRepository>,
    pub clock: Box<dyn Clock>,
    pub ids: Box<dyn IdGenerator>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ApplicationState {
    pub settings: Settings,
    pub active_plan: Plan,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PreviewRequest {
    pub task: Task,
    pub profile: StoredProfile,
    pub action: ActionSpec,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetentionReport {
    pub deleted_runs: u64,
    pub deleted_logs: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum UseCaseError {
    Repository(RepositoryError),
    NotFound(String),
    Conflict(String),
    Invalid(String),
    InvalidProfile {
        profile_id: ProfileId,
        diagnostics: Vec<ParserDiagnostic>,
    },
}

impl fmt::Display for UseCaseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Repository(error) => error.fmt(formatter),
            Self::NotFound(message) | Self::Conflict(message) | Self::Invalid(message) => {
                formatter.write_str(message)
            }
            Self::InvalidProfile { profile_id, .. } => {
                write!(formatter, "profile {profile_id} is invalid")
            }
        }
    }
}

impl std::error::Error for UseCaseError {}

impl From<RepositoryError> for UseCaseError {
    fn from(error: RepositoryError) -> Self {
        Self::Repository(error)
    }
}

pub struct ApplicationServices {
    ports: ApplicationPorts,
    state: Mutex<ApplicationState>,
}

impl ApplicationServices {
    pub fn bootstrap(ports: ApplicationPorts) -> Result<Self, UseCaseError> {
        let settings = match ports.settings.load()? {
            Some(settings) => settings,
            None => {
                let settings = Settings::default();
                ports.settings.save(&settings)?;
                settings
            }
        };
        let active_plan = match ports.active_plan.load()? {
            Some(plan) => plan,
            None => {
                let plan = empty_plan();
                ports.active_plan.save(&plan)?;
                plan
            }
        };
        validate_settings(&settings)?;
        validate_plan(&active_plan)?;
        Ok(Self {
            ports,
            state: Mutex::new(ApplicationState {
                settings,
                active_plan,
            }),
        })
    }

    pub fn state(&self) -> Result<ApplicationState, UseCaseError> {
        Ok(self.lock_state()?.clone())
    }

    pub fn save_settings(&self, settings: Settings) -> Result<(), UseCaseError> {
        validate_settings(&settings)?;
        self.ports.settings.save(&settings)?;
        self.lock_state()?.settings = settings;
        Ok(())
    }

    pub fn save_active_plan(&self, mut plan: Plan) -> Result<(), UseCaseError> {
        for task in &mut plan.tasks {
            task.source = canonical_directory(&task.source)?;
            self.require_valid_profile(task.profile_id)?;
        }
        validate_plan(&plan)?;
        self.ports.active_plan.save(&plan)?;
        self.lock_state()?.active_plan = plan;
        Ok(())
    }

    pub fn add_task(
        &self,
        source: PathBuf,
        enabled: bool,
        profile_id: ProfileId,
        steps: Vec<ActionSpec>,
    ) -> Result<Task, UseCaseError> {
        let canonical_source = canonical_directory(&source)?;
        self.require_valid_profile(profile_id)?;
        let task = Task {
            id: self.ports.ids.task_id(),
            source: canonical_source,
            enabled,
            profile_id,
            steps,
            extensions: Extensions::new(),
        };
        let mut state = self.lock_state()?;
        ensure_unique_source(&state.active_plan, &task, None)?;
        let mut next = state.active_plan.clone();
        next.tasks.push(task.clone());
        validate_plan(&next)?;
        self.ports.active_plan.save(&next)?;
        state.active_plan = next;
        Ok(task)
    }

    pub fn update_task(&self, task: Task) -> Result<(), UseCaseError> {
        let mut task = task;
        task.source = canonical_directory(&task.source)?;
        self.require_valid_profile(task.profile_id)?;
        let mut state = self.lock_state()?;
        let index = state
            .active_plan
            .tasks
            .iter()
            .position(|candidate| candidate.id == task.id)
            .ok_or_else(|| UseCaseError::NotFound(format!("task {} not found", task.id)))?;
        ensure_unique_source(&state.active_plan, &task, Some(task.id))?;
        let mut next = state.active_plan.clone();
        next.tasks[index] = task;
        validate_plan(&next)?;
        self.ports.active_plan.save(&next)?;
        state.active_plan = next;
        Ok(())
    }

    pub fn remove_task(&self, task_id: TaskId) -> Result<bool, UseCaseError> {
        let mut state = self.lock_state()?;
        let mut next = state.active_plan.clone();
        let original_len = next.tasks.len();
        next.tasks.retain(|task| task.id != task_id);
        if next.tasks.len() == original_len {
            return Ok(false);
        }
        self.ports.active_plan.save(&next)?;
        state.active_plan = next;
        Ok(true)
    }

    pub fn profiles(&self) -> Result<Vec<StoredProfile>, UseCaseError> {
        self.ports.profiles.list().map_err(Into::into)
    }

    pub fn create_profile(&self, name: &str) -> Result<StoredProfile, UseCaseError> {
        let name = validate_profile_name(name)?;
        let id = self.ports.ids.profile_id();
        let existing = self.ports.profiles.list()?;
        let base = profile_filename_stem(name);
        let mut suffix = 1_u32;
        let filename = loop {
            let candidate = if suffix == 1 {
                format!("{base}.packignore")
            } else {
                format!("{base}-{suffix}.packignore")
            };
            if existing.iter().all(|profile| {
                profile.path.file_name().and_then(|value| value.to_str())
                    != Some(candidate.as_str())
            }) {
                break candidate;
            }
            suffix += 1;
        };
        let text = format!("# @profile-id {id}\n# @profile-version 1\n# @profile-name {name}\n\n");
        self.ports
            .profiles
            .save_text(&filename, &text)
            .map_err(Into::into)
    }

    pub fn rename_profile(
        &self,
        profile_id: ProfileId,
        name: &str,
    ) -> Result<StoredProfile, UseCaseError> {
        let name = validate_profile_name(name)?;
        let profile = self
            .ports
            .profiles
            .get(profile_id)?
            .ok_or_else(|| UseCaseError::NotFound(format!("profile {profile_id} not found")))?;
        let filename = profile
            .path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| UseCaseError::Invalid("profile filename is not UTF-8".into()))?;
        let mut replaced = false;
        let text = profile
            .text
            .split_inclusive('\n')
            .map(|line| {
                if !replaced && line.starts_with("# @profile-name ") {
                    replaced = true;
                    if line.ends_with('\n') {
                        format!("# @profile-name {name}\n")
                    } else {
                        format!("# @profile-name {name}")
                    }
                } else {
                    line.to_owned()
                }
            })
            .collect::<String>();
        if !replaced {
            return Err(UseCaseError::Invalid(
                "profile does not contain @profile-name metadata".into(),
            ));
        }
        self.ports
            .profiles
            .save_text(filename, &text)
            .map_err(Into::into)
    }

    pub fn save_profile_text(
        &self,
        filename: &str,
        text: &str,
    ) -> Result<StoredProfile, UseCaseError> {
        self.ports
            .profiles
            .save_text(filename, text)
            .map_err(Into::into)
    }

    pub fn delete_profile(&self, profile_id: ProfileId) -> Result<bool, UseCaseError> {
        if self
            .lock_state()?
            .active_plan
            .tasks
            .iter()
            .any(|task| task.profile_id == profile_id)
        {
            return Err(UseCaseError::Conflict(format!(
                "profile {profile_id} is used by an active task"
            )));
        }
        self.ports.profiles.delete(profile_id).map_err(Into::into)
    }

    pub fn restore_default_profile(&self) -> Result<StoredProfile, UseCaseError> {
        self.ports.profiles.restore_default().map_err(Into::into)
    }

    pub fn presets(&self) -> Result<Vec<StoredPreset>, UseCaseError> {
        self.ports.presets.list().map_err(Into::into)
    }

    pub fn save_preset_text(
        &self,
        filename: &str,
        text: &str,
    ) -> Result<StoredPreset, UseCaseError> {
        self.ports
            .presets
            .save_text(filename, text)
            .map_err(Into::into)
    }

    pub fn delete_preset(&self, preset_id: &PresetId) -> Result<bool, UseCaseError> {
        self.ports.presets.delete(preset_id).map_err(Into::into)
    }

    pub fn reset_preset(&self, preset_id: &PresetId) -> Result<StoredPreset, UseCaseError> {
        self.ports
            .presets
            .reset_from_resources(preset_id)
            .map_err(Into::into)
    }

    pub fn prepare_preview(&self, task_id: TaskId) -> Result<PreviewRequest, UseCaseError> {
        let state = self.lock_state()?;
        let task = find_task(&state.active_plan, task_id)?.clone();
        let profile = self.require_valid_profile(task.profile_id)?;
        let action = executable_action(&task)?.clone();
        Ok(PreviewRequest {
            task,
            profile,
            action,
        })
    }

    pub fn prepare_run_current(&self, task_id: TaskId) -> Result<RunRecord, UseCaseError> {
        let state = self.lock_state()?;
        let task = find_task(&state.active_plan, task_id)?.clone();
        if !task.enabled {
            return Err(UseCaseError::Invalid(format!(
                "task {} is disabled",
                task.id
            )));
        }
        executable_action(&task)?;
        let profile = self.require_valid_profile(task.profile_id)?;
        self.insert_queued_run(RunSnapshot {
            task,
            settings: state.settings.clone(),
            profile_hash: sha256(&profile.text),
            profile_text: profile.text,
        })
    }

    pub fn prepare_all_enabled(&self) -> Result<Vec<RunRecord>, UseCaseError> {
        let state = self.lock_state()?.clone();
        let mut snapshots = Vec::new();
        for task in state
            .active_plan
            .tasks
            .into_iter()
            .filter(|task| task.enabled)
        {
            executable_action(&task)?;
            let profile = self.require_valid_profile(task.profile_id)?;
            snapshots.push(RunSnapshot {
                task,
                settings: state.settings.clone(),
                profile_hash: sha256(&profile.text),
                profile_text: profile.text,
            });
        }
        snapshots
            .into_iter()
            .map(|snapshot| self.insert_queued_run(snapshot))
            .collect()
    }

    pub fn repeat_run(&self, previous_run_id: RunId) -> Result<RunRecord, UseCaseError> {
        let previous =
            self.ports.history.get(previous_run_id)?.ok_or_else(|| {
                UseCaseError::NotFound(format!("run {previous_run_id} not found"))
            })?;
        executable_action(&previous.snapshot.task)?;
        self.insert_queued_run(previous.snapshot)
    }

    pub fn history(&self, page: PageRequest) -> Result<Vec<RunRecord>, UseCaseError> {
        self.ports.history.page(page).map_err(Into::into)
    }

    pub fn run(&self, run_id: RunId) -> Result<Option<RunRecord>, UseCaseError> {
        self.ports.history.get(run_id).map_err(Into::into)
    }

    pub fn logs(&self, run_id: RunId, page: PageRequest) -> Result<Vec<LogRecord>, UseCaseError> {
        self.ports.logs.page(run_id, page).map_err(Into::into)
    }

    pub fn append_log(&self, record: &LogRecord) -> Result<(), UseCaseError> {
        self.ports.logs.append(record).map_err(Into::into)
    }

    pub fn update_run(&self, run: &RunRecord) -> Result<(), UseCaseError> {
        self.ports.history.update(run).map_err(Into::into)
    }

    pub fn apply_retention(&self) -> Result<RetentionReport, UseCaseError> {
        let settings = self.lock_state()?.settings.history.clone();
        let now = self.ports.clock.now();
        let deleted_logs = self.ports.logs.apply_retention(
            now,
            settings.logs.max_age_days,
            settings.logs.max_entries,
            settings.logs.unlimited,
        )?;
        let deleted_runs = self.ports.history.apply_retention(
            now,
            settings.runs.max_age_days,
            settings.runs.max_entries,
            settings.runs.unlimited,
        )?;
        Ok(RetentionReport {
            deleted_runs,
            deleted_logs,
        })
    }

    fn require_valid_profile(&self, profile_id: ProfileId) -> Result<StoredProfile, UseCaseError> {
        let profile = self
            .ports
            .profiles
            .get(profile_id)?
            .ok_or_else(|| UseCaseError::NotFound(format!("profile {profile_id} not found")))?;
        if !profile.valid {
            return Err(UseCaseError::InvalidProfile {
                profile_id,
                diagnostics: profile.diagnostics,
            });
        }
        Ok(profile)
    }

    fn insert_queued_run(&self, snapshot: RunSnapshot) -> Result<RunRecord, UseCaseError> {
        let run = RunRecord {
            run_id: self.ports.ids.run_id(),
            task_id: snapshot.task.id,
            state: RunState::Queued,
            started_at: self.ports.clock.now(),
            finished_at: None,
            snapshot,
            summary: None,
        };
        self.ports.history.insert(&run)?;
        Ok(run)
    }

    fn lock_state(&self) -> Result<MutexGuard<'_, ApplicationState>, UseCaseError> {
        self.state
            .lock()
            .map_err(|_| UseCaseError::Invalid("application state lock is poisoned".into()))
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        Timestamp::now()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct UuidIdGenerator;

impl IdGenerator for UuidIdGenerator {
    fn run_id(&self) -> RunId {
        RunId::new()
    }

    fn task_id(&self) -> TaskId {
        TaskId::new()
    }

    fn profile_id(&self) -> ProfileId {
        ProfileId::new()
    }
}

fn empty_plan() -> Plan {
    Plan {
        version: PlanVersion::CURRENT,
        name: "Active plan".into(),
        tasks: Vec::new(),
        extensions: Extensions::new(),
    }
}

fn validate_profile_name(name: &str) -> Result<&str, UseCaseError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(UseCaseError::Invalid("profile name cannot be empty".into()));
    }
    if name.chars().count() > 128 {
        return Err(UseCaseError::Invalid(
            "profile name cannot exceed 128 characters".into(),
        ));
    }
    if name.contains(['\r', '\n']) {
        return Err(UseCaseError::Invalid(
            "profile name cannot contain a line break".into(),
        ));
    }
    Ok(name)
}

fn profile_filename_stem(name: &str) -> String {
    let mut stem = String::new();
    let mut separator = false;
    for character in name.chars() {
        if character.is_ascii_alphanumeric() {
            stem.push(character.to_ascii_lowercase());
            separator = false;
        } else if !separator && !stem.is_empty() {
            stem.push('-');
            separator = true;
        }
    }
    while stem.ends_with('-') {
        stem.pop();
    }
    if stem.is_empty() {
        "profile".into()
    } else {
        stem
    }
}

fn canonical_directory(path: &Path) -> Result<PathBuf, UseCaseError> {
    let canonical = fs::canonicalize(path).map_err(|error| {
        UseCaseError::Invalid(format!(
            "source directory {} is unavailable: {error}",
            path.display()
        ))
    })?;
    if !canonical.is_dir() {
        return Err(UseCaseError::Invalid(format!(
            "source {} is not a directory",
            path.display()
        )));
    }
    Ok(canonical)
}

fn ensure_unique_source(
    plan: &Plan,
    task: &Task,
    except: Option<TaskId>,
) -> Result<(), UseCaseError> {
    if let Some(existing) = plan
        .tasks
        .iter()
        .find(|existing| except != Some(existing.id) && existing.source == task.source)
    {
        return Err(UseCaseError::Conflict(format!(
            "source {} is already used by task {}",
            task.source.display(),
            existing.id
        )));
    }
    Ok(())
}

fn find_task(plan: &Plan, task_id: TaskId) -> Result<&Task, UseCaseError> {
    plan.tasks
        .iter()
        .find(|task| task.id == task_id)
        .ok_or_else(|| UseCaseError::NotFound(format!("task {task_id} not found")))
}

fn executable_action(task: &Task) -> Result<&ActionSpec, UseCaseError> {
    match task.steps.as_slice() {
        [action @ ActionSpec::Archive(_)] => Ok(action),
        [ActionSpec::Unsupported(action)] => Err(UseCaseError::Invalid(format!(
            "action type `{}` is unsupported",
            action.action_type
        ))),
        _ => Err(UseCaseError::Invalid(format!(
            "task {} requires exactly one archive action",
            task.id
        ))),
    }
}

fn validate_settings(settings: &Settings) -> Result<(), UseCaseError> {
    let issues = settings.validate();
    if issues.is_empty() {
        Ok(())
    } else {
        Err(UseCaseError::Invalid(
            issues
                .into_iter()
                .map(|issue| format!("{}: {}", issue.path, issue.message))
                .collect::<Vec<_>>()
                .join("; "),
        ))
    }
}

fn validate_plan(plan: &Plan) -> Result<(), UseCaseError> {
    let issues = plan.validate();
    if issues.is_empty() {
        Ok(())
    } else {
        Err(UseCaseError::Invalid(
            issues
                .into_iter()
                .map(|issue| format!("{}: {}", issue.path, issue.message))
                .collect::<Vec<_>>()
                .join("; "),
        ))
    }
}

fn sha256(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}
