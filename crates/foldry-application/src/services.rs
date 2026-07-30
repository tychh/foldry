use std::{
    fmt, fs,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
};

use jiff::Timestamp;
use sha2::{Digest, Sha256};

use crate::{
    ActionId, ActionSpec, ActionVersion, ActivePlanRepository, ArchiveActionSpec,
    ArchiveOutputDirectory, ArchiveOutputSpec, BrowserView, Clock, ContractValidation,
    DEFAULT_PROFILE_FILENAME, Extensions, Folder, FolderAction, FolderId, FolderSnapshot,
    IdGenerator, LogRecord, LogRepository, PageRequest, ParserDiagnostic, Plan, PlanVersion,
    PresetId, PresetRepository, ProfileId, ProfileRepository, RepositoryError,
    RunHistoryRepository, RunId, RunRecord, RunSnapshot, RunState, Settings, SettingsRepository,
    StoredPreset, StoredProfile, VerificationSpec,
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
    pub folder: Folder,
    pub profile: StoredProfile,
    pub action: FolderAction,
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
    run_preparation: Mutex<()>,
}

impl ApplicationServices {
    pub fn bootstrap(ports: ApplicationPorts) -> Result<Self, UseCaseError> {
        let default_profile = ensure_default_profile(ports.profiles.as_ref())?;
        let profiles = ports.profiles.list()?;
        let known_profile = |id: ProfileId| {
            profiles
                .iter()
                .any(|profile| profile.id.is_some_and(|candidate| candidate == id))
        };
        let (mut settings, mut settings_changed) = match ports.settings.load()? {
            Some(settings) => (settings, false),
            None => (Settings::default(), true),
        };
        let (mut active_plan, mut active_plan_changed) = match ports.active_plan.load()? {
            Some(plan) => (plan, false),
            None => (empty_plan(), true),
        };

        if let Some(default_id) = default_profile.id {
            if settings.default_profile_id.is_none()
                || settings
                    .default_profile_id
                    .is_some_and(|profile_id| !known_profile(profile_id))
            {
                settings.default_profile_id = Some(default_id);
                settings_changed = true;
            }
            for folder in &mut active_plan.folders {
                if !known_profile(folder.default_profile_id) {
                    folder.default_profile_id = default_id;
                    active_plan_changed = true;
                }
                for action in &mut folder.actions {
                    if action
                        .profile_id_override
                        .is_some_and(|profile_id| !known_profile(profile_id))
                    {
                        action.profile_id_override = None;
                        active_plan_changed = true;
                    }
                }
            }
        }

        validate_settings(&settings)?;
        validate_plan(&active_plan)?;
        if settings_changed {
            ports.settings.save(&settings)?;
        }
        if active_plan_changed {
            ports.active_plan.save(&active_plan)?;
        }
        Ok(Self {
            ports,
            state: Mutex::new(ApplicationState {
                settings,
                active_plan,
            }),
            run_preparation: Mutex::new(()),
        })
    }

    pub fn state(&self) -> Result<ApplicationState, UseCaseError> {
        Ok(self.lock_state()?.clone())
    }

    pub fn save_settings(&self, mut settings: Settings) -> Result<(), UseCaseError> {
        if let Some(profile_id) = settings.default_profile_id {
            settings.default_profile_id =
                Some(valid_profile_id(&self.require_valid_profile(profile_id)?)?);
        }
        normalize_browser_settings(&mut settings);
        validate_settings(&settings)?;
        self.ports.settings.save(&settings)?;
        self.lock_state()?.settings = settings;
        Ok(())
    }

    pub fn save_active_plan(&self, mut plan: Plan) -> Result<(), UseCaseError> {
        for folder in &mut plan.folders {
            folder.source = canonical_directory(&folder.source)?;
            folder.default_profile_id =
                valid_profile_id(&self.require_valid_profile(folder.default_profile_id)?)?;
            for action in &mut folder.actions {
                if let Some(profile_id) = action.profile_id_override {
                    action.profile_id_override =
                        Some(valid_profile_id(&self.require_valid_profile(profile_id)?)?);
                }
                canonicalize_action_output(&folder.source, action)?;
            }
        }
        validate_plan(&plan)?;
        self.ports.active_plan.save(&plan)?;
        self.lock_state()?.active_plan = plan;
        Ok(())
    }

    pub fn add_folder(
        &self,
        source: PathBuf,
        default_profile_id: Option<ProfileId>,
    ) -> Result<Folder, UseCaseError> {
        let canonical_source = canonical_directory(&source)?;
        let default_profile_id = match default_profile_id {
            Some(profile_id) => valid_profile_id(&self.require_valid_profile(profile_id)?)?,
            None => {
                let configured = self.lock_state()?.settings.default_profile_id;
                match configured {
                    Some(profile_id) => valid_profile_id(&self.require_valid_profile(profile_id)?)?,
                    None => valid_profile_id(&self.ensure_default_profile()?)?,
                }
            }
        };
        let mut state = self.lock_state()?;
        if let Some(index) = state
            .active_plan
            .folders
            .iter()
            .position(|folder| folder.source == canonical_source)
        {
            let mut next = state.active_plan.clone();
            next.folders[index].listed = true;
            let folder = next.folders[index].clone();
            let mut settings = state.settings.clone();
            remember_recent_parent(&mut settings, &canonical_source);
            self.ports.active_plan.save(&next)?;
            self.ports.settings.save(&settings)?;
            state.active_plan = next;
            state.settings = settings;
            return Ok(folder);
        }
        let action = default_archive_action(self.ports.ids.action_id(), &state.settings);
        let folder = Folder {
            id: self.ports.ids.folder_id(),
            source: canonical_source,
            listed: true,
            enabled: true,
            default_profile_id,
            actions: vec![action],
            extensions: Extensions::new(),
        };
        ensure_unique_source(&state.active_plan, &folder, None)?;
        let mut next = state.active_plan.clone();
        next.folders.push(folder.clone());
        let mut settings = state.settings.clone();
        remember_recent_parent(&mut settings, &folder.source);
        validate_plan(&next)?;
        self.ports.active_plan.save(&next)?;
        self.ports.settings.save(&settings)?;
        state.active_plan = next;
        state.settings = settings;
        Ok(folder)
    }

    pub fn update_folder(&self, folder: Folder) -> Result<(), UseCaseError> {
        let mut folder = folder;
        folder.source = canonical_directory(&folder.source)?;
        folder.default_profile_id =
            valid_profile_id(&self.require_valid_profile(folder.default_profile_id)?)?;
        for action in &mut folder.actions {
            if let Some(profile_id) = action.profile_id_override {
                action.profile_id_override =
                    Some(valid_profile_id(&self.require_valid_profile(profile_id)?)?);
            }
            canonicalize_action_output(&folder.source, action)?;
        }
        let mut state = self.lock_state()?;
        let index = state
            .active_plan
            .folders
            .iter()
            .position(|candidate| candidate.id == folder.id)
            .ok_or_else(|| UseCaseError::NotFound(format!("folder {} not found", folder.id)))?;
        ensure_unique_source(&state.active_plan, &folder, Some(folder.id))?;
        let mut next = state.active_plan.clone();
        next.folders[index] = folder;
        validate_plan(&next)?;
        self.ports.active_plan.save(&next)?;
        state.active_plan = next;
        Ok(())
    }

    pub fn unlist_folder(&self, folder_id: FolderId) -> Result<bool, UseCaseError> {
        self.ensure_folder_has_no_active_runs(folder_id)?;
        let mut state = self.lock_state()?;
        let mut next = state.active_plan.clone();
        let folder = next
            .folders
            .iter_mut()
            .find(|folder| folder.id == folder_id)
            .ok_or_else(|| UseCaseError::NotFound(format!("folder {folder_id} not found")))?;
        if !folder.listed {
            return Ok(false);
        }
        folder.listed = false;
        self.ports.active_plan.save(&next)?;
        state.active_plan = next;
        Ok(true)
    }

    pub fn unlisted_folders(&self) -> Result<Vec<Folder>, UseCaseError> {
        Ok(self
            .lock_state()?
            .active_plan
            .folders
            .iter()
            .filter(|folder| !folder.listed)
            .cloned()
            .collect())
    }

    pub fn forget_folders(&self, folder_ids: &[FolderId]) -> Result<u64, UseCaseError> {
        let requested = folder_ids
            .iter()
            .copied()
            .collect::<std::collections::HashSet<_>>();
        for folder_id in &requested {
            self.ensure_folder_has_no_active_runs(*folder_id)?;
        }
        let mut state = self.lock_state()?;
        if let Some(folder) = state
            .active_plan
            .folders
            .iter()
            .find(|folder| folder.listed && requested.contains(&folder.id))
        {
            return Err(UseCaseError::Conflict(format!(
                "listed folder {} cannot be forgotten",
                folder.id
            )));
        }
        let mut next = state.active_plan.clone();
        let original_len = next.folders.len();
        next.folders
            .retain(|folder| !requested.contains(&folder.id));
        let removed = u64::try_from(original_len - next.folders.len()).unwrap_or(u64::MAX);
        if removed > 0 {
            self.ports.active_plan.save(&next)?;
            state.active_plan = next;
        }
        Ok(removed)
    }

    pub fn forget_all_unlisted_folders(&self) -> Result<u64, UseCaseError> {
        let folder_ids = self
            .unlisted_folders()?
            .into_iter()
            .map(|folder| folder.id)
            .collect::<Vec<_>>();
        self.forget_folders(&folder_ids)
    }

    pub fn add_action(
        &self,
        folder_id: FolderId,
        enabled: bool,
        profile_id_override: Option<ProfileId>,
        spec: ActionSpec,
    ) -> Result<FolderAction, UseCaseError> {
        let profile_id_override = profile_id_override
            .map(|profile_id| {
                self.require_valid_profile(profile_id)
                    .and_then(|profile| valid_profile_id(&profile))
            })
            .transpose()?;
        let mut action = FolderAction {
            id: self.ports.ids.action_id(),
            enabled,
            profile_id_override,
            spec,
            extensions: Extensions::new(),
        };
        let mut state = self.lock_state()?;
        let mut next = state.active_plan.clone();
        let folder = find_folder_mut(&mut next, folder_id)?;
        canonicalize_action_output(&folder.source, &mut action)?;
        folder.actions.push(action.clone());
        validate_plan(&next)?;
        self.ports.active_plan.save(&next)?;
        state.active_plan = next;
        Ok(action)
    }

    pub fn add_archive_action(
        &self,
        folder_id: FolderId,
        enabled: bool,
        profile_id_override: Option<ProfileId>,
    ) -> Result<FolderAction, UseCaseError> {
        let settings = self.lock_state()?.settings.clone();
        let mut action = default_archive_action(self.ports.ids.action_id(), &settings);
        action.enabled = enabled;
        action.profile_id_override = profile_id_override
            .map(|profile_id| {
                self.require_valid_profile(profile_id)
                    .and_then(|profile| valid_profile_id(&profile))
            })
            .transpose()?;
        let mut state = self.lock_state()?;
        let mut next = state.active_plan.clone();
        let folder = find_folder_mut(&mut next, folder_id)?;
        canonicalize_action_output(&folder.source, &mut action)?;
        folder.actions.push(action.clone());
        validate_plan(&next)?;
        self.ports.active_plan.save(&next)?;
        state.active_plan = next;
        Ok(action)
    }

    pub fn update_action(
        &self,
        folder_id: FolderId,
        mut action: FolderAction,
    ) -> Result<(), UseCaseError> {
        if let Some(profile_id) = action.profile_id_override {
            action.profile_id_override =
                Some(valid_profile_id(&self.require_valid_profile(profile_id)?)?);
        }
        let mut state = self.lock_state()?;
        let mut next = state.active_plan.clone();
        let folder = find_folder_mut(&mut next, folder_id)?;
        canonicalize_action_output(&folder.source, &mut action)?;
        let index = folder
            .actions
            .iter()
            .position(|candidate| candidate.id == action.id)
            .ok_or_else(|| UseCaseError::NotFound(format!("action {} not found", action.id)))?;
        folder.actions[index] = action;
        validate_plan(&next)?;
        self.ports.active_plan.save(&next)?;
        state.active_plan = next;
        Ok(())
    }

    pub fn remove_action(
        &self,
        folder_id: FolderId,
        action_id: ActionId,
    ) -> Result<bool, UseCaseError> {
        self.ensure_action_has_no_active_runs(folder_id, action_id)?;
        let mut state = self.lock_state()?;
        let mut next = state.active_plan.clone();
        let folder = find_folder_mut(&mut next, folder_id)?;
        let original_len = folder.actions.len();
        folder.actions.retain(|action| action.id != action_id);
        if folder.actions.len() == original_len {
            return Ok(false);
        }
        self.ports.active_plan.save(&next)?;
        state.active_plan = next;
        Ok(true)
    }

    pub fn reorder_actions(
        &self,
        folder_id: FolderId,
        ordered_action_ids: &[ActionId],
    ) -> Result<(), UseCaseError> {
        let mut state = self.lock_state()?;
        let mut next = state.active_plan.clone();
        let folder = find_folder_mut(&mut next, folder_id)?;
        if ordered_action_ids.len() != folder.actions.len() {
            return Err(UseCaseError::Invalid(
                "reorder must contain every action exactly once".into(),
            ));
        }
        let mut remaining = std::mem::take(&mut folder.actions);
        let mut reordered = Vec::with_capacity(remaining.len());
        for action_id in ordered_action_ids {
            let index = remaining
                .iter()
                .position(|action| action.id == *action_id)
                .ok_or_else(|| {
                    UseCaseError::Invalid(format!(
                        "action {action_id} is missing or duplicated in reorder"
                    ))
                })?;
            reordered.push(remaining.remove(index));
        }
        if !remaining.is_empty() {
            return Err(UseCaseError::Invalid(
                "reorder must contain every action exactly once".into(),
            ));
        }
        folder.actions = reordered;
        self.ports.active_plan.save(&next)?;
        state.active_plan = next;
        Ok(())
    }

    pub fn profiles(&self) -> Result<Vec<StoredProfile>, UseCaseError> {
        self.ensure_default_profile()?;
        self.ports.profiles.list().map_err(Into::into)
    }

    pub fn profile_usage(&self, profile_id: ProfileId) -> Result<u64, UseCaseError> {
        let state = self.lock_state()?;
        Ok(state
            .active_plan
            .folders
            .iter()
            .map(|folder| {
                u64::from(folder.default_profile_id == profile_id)
                    + folder
                        .actions
                        .iter()
                        .filter(|action| action.profile_id_override == Some(profile_id))
                        .count() as u64
            })
            .sum())
    }

    pub fn browser_favorites(&self) -> Result<Vec<PathBuf>, UseCaseError> {
        Ok(self.lock_state()?.settings.browser.favorites.clone())
    }

    pub fn browser_recent(&self) -> Result<Vec<PathBuf>, UseCaseError> {
        Ok(self
            .lock_state()?
            .settings
            .browser
            .recent
            .iter()
            .filter(|path| path.is_dir())
            .cloned()
            .collect())
    }

    pub fn set_browser_view(&self, view: BrowserView) -> Result<BrowserView, UseCaseError> {
        let mut state = self.lock_state()?;
        let mut settings = state.settings.clone();
        settings.browser.view = view;
        self.ports.settings.save(&settings)?;
        state.settings = settings;
        Ok(view)
    }

    pub fn set_favorite(
        &self,
        path: PathBuf,
        favorite: bool,
    ) -> Result<Vec<PathBuf>, UseCaseError> {
        let canonical = if favorite {
            canonical_directory(&path)?
        } else {
            fs::canonicalize(&path).unwrap_or(path)
        };
        let mut state = self.lock_state()?;
        let mut settings = state.settings.clone();
        settings
            .browser
            .favorites
            .retain(|candidate| candidate != &canonical);
        if favorite {
            settings.browser.favorites.push(canonical);
        }
        self.ports.settings.save(&settings)?;
        let favorites = settings.browser.favorites.clone();
        state.settings = settings;
        Ok(favorites)
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
        if let Some(profile) = self.ports.profiles.get(profile_id)?
            && is_default_profile(&profile)
        {
            return Err(UseCaseError::Conflict(
                "the default profile cannot be deleted".into(),
            ));
        }
        if !self.ports.profiles.delete(profile_id)? {
            return Ok(false);
        }
        let default_id = valid_profile_id(&self.ensure_default_profile()?)?;
        let mut state = self.lock_state()?;
        let mut next_plan = state.active_plan.clone();
        let mut plan_changed = false;
        for folder in &mut next_plan.folders {
            if folder.default_profile_id == profile_id {
                folder.default_profile_id = default_id;
                plan_changed = true;
            }
            for action in &mut folder.actions {
                if action.profile_id_override == Some(profile_id) {
                    action.profile_id_override = None;
                    plan_changed = true;
                }
            }
        }
        let mut next_settings = state.settings.clone();
        let settings_changed = next_settings.default_profile_id == Some(profile_id);
        if settings_changed {
            next_settings.default_profile_id = Some(default_id);
        }
        if plan_changed {
            self.ports.active_plan.save(&next_plan)?;
            state.active_plan = next_plan;
        }
        if settings_changed {
            self.ports.settings.save(&next_settings)?;
            state.settings = next_settings;
        }
        Ok(true)
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

    pub fn prepare_preview(
        &self,
        folder_id: FolderId,
        action_id: ActionId,
    ) -> Result<PreviewRequest, UseCaseError> {
        let state = self.lock_state()?;
        let folder = find_folder(&state.active_plan, folder_id)?.clone();
        let action = find_action(&folder, action_id)?.clone();
        executable_action(&action)?;
        let profile = self.require_valid_profile(action.effective_profile_id(&folder))?;
        Ok(PreviewRequest {
            folder,
            profile,
            action,
        })
    }

    pub fn prepare_run_current(
        &self,
        folder_id: FolderId,
        action_id: ActionId,
    ) -> Result<RunRecord, UseCaseError> {
        let state = self.lock_state()?;
        let folder = find_folder(&state.active_plan, folder_id)?;
        let action = find_action(folder, action_id)?;
        executable_action(action)?;
        let profile = self.require_valid_profile(action.effective_profile_id(folder))?;
        self.insert_queued_run(run_snapshot(folder, action, &state.settings, profile)?)
    }

    pub fn prepare_folder_enabled(
        &self,
        folder_id: FolderId,
    ) -> Result<Vec<RunRecord>, UseCaseError> {
        let state = self.lock_state()?.clone();
        let folder = find_folder(&state.active_plan, folder_id)?;
        let mut snapshots = Vec::new();
        for action in folder.actions.iter().filter(|action| action.enabled) {
            executable_action(action)?;
            let profile = self.require_valid_profile(action.effective_profile_id(folder))?;
            snapshots.push(run_snapshot(folder, action, &state.settings, profile)?);
        }
        snapshots
            .into_iter()
            .map(|snapshot| self.insert_queued_run(snapshot))
            .collect()
    }

    pub fn prepare_all_enabled(&self) -> Result<Vec<RunRecord>, UseCaseError> {
        let state = self.lock_state()?.clone();
        let mut snapshots = Vec::new();
        for folder in state
            .active_plan
            .folders
            .iter()
            .filter(|folder| folder.listed && folder.enabled)
        {
            for action in folder.actions.iter().filter(|action| action.enabled) {
                executable_action(action)?;
                let profile = self.require_valid_profile(action.effective_profile_id(folder))?;
                snapshots.push(run_snapshot(folder, action, &state.settings, profile)?);
            }
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
        executable_action(&previous.snapshot.action)?;
        self.insert_queued_run(previous.snapshot)
    }

    pub fn history(&self, page: PageRequest) -> Result<Vec<RunRecord>, UseCaseError> {
        self.ports.history.page(page).map_err(Into::into)
    }

    pub fn history_filtered(
        &self,
        page: PageRequest,
        folder_id: Option<FolderId>,
        action_id: Option<ActionId>,
    ) -> Result<Vec<RunRecord>, UseCaseError> {
        self.ports
            .history
            .page_filtered(page, folder_id, action_id)
            .map_err(Into::into)
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
        let profile = match self.ports.profiles.get(profile_id)? {
            Some(profile) => profile,
            None => self.ensure_default_profile()?,
        };
        if !profile.valid {
            return Err(UseCaseError::InvalidProfile {
                profile_id: profile.id.unwrap_or(profile_id),
                diagnostics: profile.diagnostics,
            });
        }
        Ok(profile)
    }

    fn ensure_folder_has_no_active_runs(&self, folder_id: FolderId) -> Result<(), UseCaseError> {
        let active = self.ports.history.non_terminal_for_folder(folder_id)?;
        if active.is_empty() {
            Ok(())
        } else {
            Err(UseCaseError::Conflict(format!(
                "folder {folder_id} has {} non-terminal run(s)",
                active.len()
            )))
        }
    }

    fn ensure_action_has_no_active_runs(
        &self,
        folder_id: FolderId,
        action_id: ActionId,
    ) -> Result<(), UseCaseError> {
        let active = self
            .ports
            .history
            .non_terminal_for_action(folder_id, action_id)?;
        if active.is_empty() {
            Ok(())
        } else {
            Err(UseCaseError::Conflict(format!(
                "action {action_id} has {} non-terminal run(s)",
                active.len()
            )))
        }
    }

    fn ensure_default_profile(&self) -> Result<StoredProfile, UseCaseError> {
        ensure_default_profile(self.ports.profiles.as_ref())
    }

    fn insert_queued_run(&self, mut snapshot: RunSnapshot) -> Result<RunRecord, UseCaseError> {
        canonicalize_action_output(&snapshot.folder.source, &mut snapshot.action)?;
        let _preparation = self
            .run_preparation
            .lock()
            .map_err(|_| UseCaseError::Invalid("run preparation lock is poisoned".into()))?;
        if let Some(existing) = self
            .ports
            .history
            .non_terminal_for_action(snapshot.folder.id, snapshot.action.id)?
            .into_iter()
            .next()
        {
            return Ok(existing);
        }
        let run = RunRecord {
            run_id: self.ports.ids.run_id(),
            folder_id: snapshot.folder.id,
            action_id: snapshot.action.id,
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

    fn folder_id(&self) -> FolderId {
        FolderId::new()
    }

    fn action_id(&self) -> ActionId {
        ActionId::new()
    }

    fn profile_id(&self) -> ProfileId {
        ProfileId::new()
    }
}

fn ensure_default_profile(
    repository: &dyn ProfileRepository,
) -> Result<StoredProfile, UseCaseError> {
    if let Some(profile) = repository.list()?.into_iter().find(is_default_profile) {
        Ok(profile)
    } else {
        repository.restore_default().map_err(Into::into)
    }
}

fn is_default_profile(profile: &StoredProfile) -> bool {
    profile.path.file_name().and_then(|name| name.to_str()) == Some(DEFAULT_PROFILE_FILENAME)
}

fn valid_profile_id(profile: &StoredProfile) -> Result<ProfileId, UseCaseError> {
    profile.id.ok_or_else(|| {
        UseCaseError::Invalid(format!(
            "valid profile {} does not contain an ID",
            profile.path.display()
        ))
    })
}

fn empty_plan() -> Plan {
    Plan {
        version: PlanVersion::CURRENT,
        name: "Active plan".into(),
        folders: Vec::new(),
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

fn normalize_browser_settings(settings: &mut Settings) {
    deduplicate_paths(&mut settings.browser.favorites);
    deduplicate_paths(&mut settings.browser.recent);
    settings.browser.recent.truncate(10);
}

fn deduplicate_paths(paths: &mut Vec<PathBuf>) {
    let mut unique = std::collections::HashSet::new();
    paths.retain(|path| unique.insert(path.clone()));
}

fn remember_recent_parent(settings: &mut Settings, source: &Path) {
    let Some(parent) = source.parent() else {
        return;
    };
    settings
        .browser
        .recent
        .retain(|candidate| candidate != parent);
    settings.browser.recent.insert(0, parent.to_path_buf());
    settings.browser.recent.truncate(10);
}

fn ensure_unique_source(
    plan: &Plan,
    folder: &Folder,
    except: Option<FolderId>,
) -> Result<(), UseCaseError> {
    if let Some(existing) = plan
        .folders
        .iter()
        .find(|existing| except != Some(existing.id) && existing.source == folder.source)
    {
        return Err(UseCaseError::Conflict(format!(
            "source {} is already used by folder {}",
            folder.source.display(),
            existing.id
        )));
    }
    Ok(())
}

fn find_folder(plan: &Plan, folder_id: FolderId) -> Result<&Folder, UseCaseError> {
    plan.folders
        .iter()
        .find(|folder| folder.id == folder_id)
        .ok_or_else(|| UseCaseError::NotFound(format!("folder {folder_id} not found")))
}

fn find_folder_mut(plan: &mut Plan, folder_id: FolderId) -> Result<&mut Folder, UseCaseError> {
    plan.folders
        .iter_mut()
        .find(|folder| folder.id == folder_id)
        .ok_or_else(|| UseCaseError::NotFound(format!("folder {folder_id} not found")))
}

fn find_action(folder: &Folder, action_id: ActionId) -> Result<&FolderAction, UseCaseError> {
    folder
        .actions
        .iter()
        .find(|action| action.id == action_id)
        .ok_or_else(|| UseCaseError::NotFound(format!("action {action_id} not found")))
}

fn executable_action(action: &FolderAction) -> Result<&ActionSpec, UseCaseError> {
    match &action.spec {
        spec @ ActionSpec::Archive(_) => Ok(spec),
        ActionSpec::Unsupported(action) => Err(UseCaseError::Invalid(format!(
            "action type `{}` is unsupported",
            action.action_type
        ))),
    }
}

fn run_snapshot(
    folder: &Folder,
    action: &FolderAction,
    settings: &Settings,
    profile: StoredProfile,
) -> Result<RunSnapshot, UseCaseError> {
    let effective_profile_id = valid_profile_id(&profile)?;
    Ok(RunSnapshot {
        folder: FolderSnapshot {
            id: folder.id,
            source: folder.source.clone(),
        },
        action: action.clone(),
        effective_profile_id,
        settings: settings.clone(),
        profile_hash: sha256(&profile.text),
        profile_text: profile.text,
    })
}

fn default_archive_action(action_id: ActionId, settings: &Settings) -> FolderAction {
    FolderAction {
        id: action_id,
        enabled: false,
        profile_id_override: None,
        spec: ActionSpec::Archive(ArchiveActionSpec {
            version: ActionVersion::V1,
            output: ArchiveOutputSpec {
                directory: ArchiveOutputDirectory::Parent,
                filename: "{folder}.{date}".into(),
                format: settings.archive_defaults.format,
                compression: settings.archive_defaults.compression,
                conflict_policy: settings.archive_defaults.conflict_policy,
                extensions: Extensions::new(),
            },
            include_root: settings.archive_defaults.include_root,
            unreadable_policy: settings.archive_defaults.unreadable_policy,
            verification: VerificationSpec {
                mode: settings.archive_defaults.verification_mode,
                checksum: settings.archive_defaults.checksum,
                extensions: Extensions::new(),
            },
            extensions: Extensions::new(),
        }),
        extensions: Extensions::new(),
    }
}

fn canonicalize_action_output(
    source: &Path,
    action: &mut FolderAction,
) -> Result<(), UseCaseError> {
    let ActionSpec::Archive(spec) = &mut action.spec else {
        return Ok(());
    };
    let ArchiveOutputDirectory::Custom { path } = &mut spec.output.directory else {
        return Ok(());
    };
    let canonical = canonical_directory(path)?;
    if canonical == source || canonical.starts_with(source) {
        return Err(UseCaseError::Invalid(format!(
            "custom archive output {} cannot equal or be inside source {}",
            canonical.display(),
            source.display()
        )));
    }
    *path = canonical;
    Ok(())
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
