use std::{
    collections::HashMap,
    fs,
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
};

use foldry_application::{
    ActionId, ActionSpec, ApplicationPorts, ApplicationServices, CancellationToken, Clock,
    CompiledProfile, FileSystemBrowser, Folder, FolderId, LatestRequestRegistry, LogRepository,
    PageRequest, Plan, PresetId, PreviewCacheKey, PreviewFilter, PreviewSnapshot, ProfileId,
    RunEvent, RunEventSink, RunHistoryRepository, RunRecord, RunState, Scheduler, SchedulerPorts,
    Settings, SystemClock, UseCaseError, UuidIdGenerator, detect_case_sensitivity, parse_profile,
    transport::{
        BootstrapSnapshotDto, BrowserChildrenDto, BrowserNodeDto, BrowserRootDto, BrowserSizeDto,
        BrowserViewDto, FolderActionDto, FolderAddResultDto, FolderDto, IpcErrorDto, LogRecordDto,
        PlanDto, PreviewEntryDto, PreviewFilterDto, PreviewPageDto, PreviewStartedDto,
        ProfileIdDto, RunEventDto, RunRecordDto, SettingsDto, StoragePathsDto, StoredPresetDto,
        StoredProfileDto,
    },
};
use foldry_storage::{
    AppDirectories, ArchiveRunExecutor, DirectoryOverrides, FileActivePlanRepository,
    FilePresetRepository, FileProfileRepository, FileSettingsRepository, ManifestCursor,
    ManifestHandle, SqliteRepository, SystemProcessProbe, initialize_resource_copies,
    reconcile_startup, scan_to_manifest,
};
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

pub const RUN_EVENT_NAME: &str = "foldry://run-event";
#[cfg(test)]
const COMMAND_NAMES: &[&str] = &[
    "bootstrap_snapshot",
    "browser_roots",
    "browser_children",
    "cancel_browser_request",
    "browser_node",
    "browser_size",
    "cancel_browser_size",
    "save_settings",
    "save_plan",
    "add_folder",
    "add_dropped_sources",
    "update_folder",
    "unlist_folder",
    "unlisted_folders",
    "forget_folders",
    "forget_all_unlisted_folders",
    "profile_usage",
    "browser_favorites",
    "browser_recent",
    "set_browser_view",
    "set_favorite",
    "add_action",
    "update_action",
    "remove_action",
    "reorder_actions",
    "create_profile",
    "rename_profile",
    "save_profile",
    "delete_profile",
    "restore_default_profile",
    "save_preset",
    "delete_preset",
    "reset_preset",
    "start_preview",
    "preview_page",
    "cancel_preview",
    "run_action",
    "run_folder",
    "run_all_enabled",
    "repeat_run",
    "scheduler_snapshot",
    "pause_run",
    "resume_run",
    "stop_run",
    "pause_all",
    "resume_all",
    "stop_all",
    "history_page",
    "run_details",
    "logs_page",
    "export_run_logs",
    "pick_folders",
    "reveal_run_output",
];

#[derive(Clone)]
pub struct DesktopState {
    services: Arc<ApplicationServices>,
    scheduler: Arc<Scheduler>,
    directories: AppDirectories,
    previews: Arc<Mutex<HashMap<(FolderId, ActionId), PreviewArtifact>>>,
    preview_requests: Arc<Mutex<LatestRequestRegistry<(FolderId, ActionId)>>>,
    browser_requests: Arc<Mutex<LatestRequestRegistry<String>>>,
    browser_size_requests: Arc<Mutex<LatestRequestRegistry<String>>>,
}

struct PreviewArtifact {
    snapshot: PreviewSnapshot,
    handle: ManifestHandle,
}

struct AppRunEventSink {
    app: AppHandle,
}

impl RunEventSink for AppRunEventSink {
    fn publish(&self, event: RunEvent) {
        let _ = self.app.emit(RUN_EVENT_NAME, RunEventDto::from(&event));
    }
}

impl DesktopState {
    pub fn open(app: &AppHandle) -> Result<Self, String> {
        let directories =
            AppDirectories::resolve(&DirectoryOverrides::default()).map_err(display_error)?;
        let resource_directory = resource_directory(app)?;
        Self::open_with(
            &directories,
            &resource_directory,
            Arc::new(AppRunEventSink { app: app.clone() }),
        )
    }

    fn open_with(
        directories: &AppDirectories,
        resource_directory: &Path,
        events: Arc<dyn RunEventSink>,
    ) -> Result<Self, String> {
        directories.ensure_layout().map_err(display_error)?;
        initialize_resource_copies(resource_directory, &directories.config)
            .map_err(display_error)?;

        let reconciliation_db =
            SqliteRepository::open(&directories.database()).map_err(display_error)?;
        let active_plan_repository = FileActivePlanRepository::new(directories.active_plan());
        let active_plan = foldry_application::ActivePlanRepository::load(&active_plan_repository)
            .map_err(display_error)?
            .unwrap_or_else(empty_plan);
        reconcile_startup(
            &reconciliation_db,
            SystemClock.now(),
            &output_directories(&active_plan),
            &directories.manifests(),
            24 * 60 * 60,
            &SystemProcessProbe,
        )
        .map_err(display_error)?;

        let services = Arc::new(
            ApplicationServices::bootstrap(ApplicationPorts {
                settings: Box::new(FileSettingsRepository::new(directories.settings())),
                active_plan: Box::new(active_plan_repository),
                profiles: Box::new(FileProfileRepository::new(
                    directories.profiles(),
                    resource_directory.join("profiles/default.packignore"),
                )),
                presets: Box::new(FilePresetRepository::new(
                    directories.presets(),
                    resource_directory.join("presets"),
                )),
                history: Box::new(
                    SqliteRepository::open(&directories.database()).map_err(display_error)?,
                ),
                logs: Box::new(
                    SqliteRepository::open(&directories.database()).map_err(display_error)?,
                ),
                clock: Box::new(SystemClock),
                ids: Box::new(UuidIdGenerator),
            })
            .map_err(display_error)?,
        );
        services.apply_retention().map_err(display_error)?;

        let scheduler_repository =
            Arc::new(SqliteRepository::open(&directories.database()).map_err(display_error)?);
        let history: Arc<dyn RunHistoryRepository> = scheduler_repository.clone();
        let logs: Arc<dyn LogRepository> = scheduler_repository;
        let max_parallel_runs = services
            .state()
            .map_err(display_error)?
            .settings
            .execution
            .max_parallel_runs;
        let scheduler = Scheduler::start(
            SchedulerPorts {
                history,
                logs,
                clock: Arc::new(SystemClock),
                executor: Arc::new(ArchiveRunExecutor::new(directories.manifests())),
                events,
            },
            max_parallel_runs,
        )
        .map_err(display_error)?;

        Ok(Self {
            services,
            scheduler: Arc::new(scheduler),
            directories: directories.clone(),
            previews: Arc::new(Mutex::new(HashMap::new())),
            preview_requests: Arc::new(Mutex::new(LatestRequestRegistry::default())),
            browser_requests: Arc::new(Mutex::new(LatestRequestRegistry::default())),
            browser_size_requests: Arc::new(Mutex::new(LatestRequestRegistry::default())),
        })
    }

    fn bootstrap(&self) -> Result<BootstrapSnapshotDto, IpcErrorDto> {
        let state = self.services.state().map_err(ipc_use_case_error)?;
        let profiles = self.services.profiles().map_err(ipc_use_case_error)?;
        let presets = self.services.presets().map_err(ipc_use_case_error)?;
        let active_runs = self
            .scheduler
            .records()
            .map_err(ipc_scheduler_error)?
            .iter()
            .map(Into::into)
            .collect();
        let recent_runs = self
            .services
            .history(page_request(0, 100)?)
            .map_err(ipc_use_case_error)?
            .iter()
            .map(Into::into)
            .collect();
        let roots = FileSystemBrowser::roots(home_directory().as_deref())
            .iter()
            .map(Into::into)
            .collect();
        Ok(BootstrapSnapshotDto {
            version: 1,
            settings: (&state.settings).into(),
            plan: (&state.active_plan).into(),
            profiles: profiles.iter().map(Into::into).collect(),
            presets: presets.iter().map(Into::into).collect(),
            active_runs,
            recent_runs,
            previews: lock(&self.previews, "previews")?
                .values()
                .map(|preview| (&preview.snapshot).into())
                .collect(),
            roots,
            storage: StoragePathsDto {
                config: path_text(&self.directories.config),
                data: path_text(&self.directories.data),
                cache: path_text(&self.directories.cache),
            },
        })
    }

    fn browser_children(
        &self,
        path: &str,
        cursor: Option<&str>,
        limit: Option<u64>,
    ) -> Result<BrowserChildrenDto, IpcErrorDto> {
        let directory = canonical_directory(path)?;
        let key = path_text(&directory);
        let request = lock(&self.browser_requests, "browser requests")?.begin(key.clone());
        let result = FileSystemBrowser::direct_children(&directory, &request.cancellation);
        let current =
            lock(&self.browser_requests, "browser requests")?.finish(&key, request.generation);
        if !current {
            return Err(ipc_error("cancelled", "browser request was superseded"));
        }
        let all_nodes = result.map_err(|error| ipc_error("filesystem_error", error.to_string()))?;
        let total = u64::try_from(all_nodes.len()).unwrap_or(u64::MAX);
        let offset = cursor
            .map(|value| {
                value
                    .parse::<usize>()
                    .map_err(|_| ipc_error("validation_error", "invalid browser cursor"))
            })
            .transpose()?
            .unwrap_or(0)
            .min(all_nodes.len());
        let limit = usize::try_from(limit.unwrap_or(250).clamp(1, 500)).unwrap_or(250);
        let end = offset.saturating_add(limit).min(all_nodes.len());
        let nodes = all_nodes[offset..end].iter().map(Into::into).collect();
        Ok(BrowserChildrenDto {
            generation: request.generation.to_string(),
            nodes,
            total,
            next_cursor: (end < all_nodes.len()).then(|| end.to_string()),
        })
    }

    fn browser_size(&self, path: &str) -> Result<BrowserSizeDto, IpcErrorDto> {
        let directory = canonical_directory(path)?;
        let key = path_text(&directory);
        let request =
            lock(&self.browser_size_requests, "browser size requests")?.begin(key.clone());
        let result = FileSystemBrowser::directory_size(&directory, &request.cancellation);
        let current = lock(&self.browser_size_requests, "browser size requests")?
            .finish(&key, request.generation);
        if !current {
            return Err(ipc_error(
                "cancelled",
                "browser size request was superseded",
            ));
        }
        let result = result.map_err(|error| ipc_error("filesystem_error", error.to_string()))?;
        Ok(BrowserSizeDto {
            path: key,
            logical_bytes: result.logical_bytes.to_string(),
            partial: result.partial,
            warnings: result.warnings,
            generation: request.generation.to_string(),
        })
    }

    fn start_preview(
        &self,
        folder_id: FolderId,
        action_id: ActionId,
    ) -> Result<PreviewStartedDto, IpcErrorDto> {
        let preview_id = (folder_id, action_id);
        let request = lock(&self.preview_requests, "preview requests")?.begin(preview_id);
        let preview = self
            .services
            .prepare_preview(folder_id, action_id)
            .map_err(ipc_use_case_error)?;
        let effective_profile_id = preview.profile.id.ok_or_else(|| {
            ipc_error(
                "invalid_profile",
                "effective profile is missing a profile identifier",
            )
        })?;
        let raw_size =
            FileSystemBrowser::directory_size(&preview.folder.source, &request.cancellation)
                .map_err(|error| ipc_error("preview_error", error.to_string()))?;
        let parsed = parse_profile(&preview.profile.text);
        let profile = parsed.profile.ok_or_else(|| {
            ipc_error_details(
                "invalid_profile",
                "profile cannot be used for preview",
                json!(parsed.diagnostics),
            )
        })?;
        let case = detect_case_sensitivity(&preview.folder.source)
            .map_err(|error| ipc_error("filesystem_error", error.to_string()))?;
        let matcher = CompiledProfile::new(&profile, case.value)
            .map_err(|message| ipc_error("invalid_profile", message))?;
        let cache_key = PreviewCacheKey::build(
            folder_id,
            action_id,
            &preview.profile.text,
            &preview.folder.source,
            &preview.action.spec,
        )
        .map_err(|error| ipc_error("preview_error", error.to_string()))?;
        let manifest_id = format!("preview-{}-{}-{}", folder_id, action_id, request.generation);
        let (handle, summary) = scan_to_manifest(
            &self.directories.manifests(),
            &manifest_id,
            &preview.folder.source,
            &matcher,
            &request.cancellation,
        )
        .map_err(|error| ipc_error("preview_error", error.to_string()))?;
        let current = lock(&self.preview_requests, "preview requests")?
            .finish(&preview_id, request.generation);
        if !current {
            let _ = handle.remove();
            return Err(ipc_error("cancelled", "preview request was superseded"));
        }
        let snapshot = PreviewSnapshot::new(cache_key, manifest_id, summary);
        let previous = lock(&self.previews, "previews")?.insert(
            preview_id,
            PreviewArtifact {
                snapshot: snapshot.clone(),
                handle,
            },
        );
        if let Some(previous) = previous {
            let _ = previous.handle.remove();
        }
        Ok(PreviewStartedDto {
            generation: request.generation.to_string(),
            snapshot: (&snapshot).into(),
            action: (&preview.action).into(),
            effective_profile_id: ProfileIdDto(effective_profile_id.to_string()),
            effective_profile_name: preview.profile.name,
            raw_bytes: raw_size.logical_bytes.to_string(),
            raw_bytes_partial: raw_size.partial,
            raw_bytes_warnings: raw_size.warnings,
        })
    }
}

#[tauri::command]
pub fn bootstrap_snapshot(
    state: State<'_, DesktopState>,
) -> Result<BootstrapSnapshotDto, IpcErrorDto> {
    state.bootstrap()
}

#[tauri::command]
pub fn browser_roots() -> Vec<BrowserRootDto> {
    FileSystemBrowser::roots(home_directory().as_deref())
        .iter()
        .map(Into::into)
        .collect()
}

#[tauri::command]
pub async fn browser_children(
    path: String,
    cursor: Option<String>,
    limit: Option<u64>,
    state: State<'_, DesktopState>,
) -> Result<BrowserChildrenDto, IpcErrorDto> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        state.browser_children(&path, cursor.as_deref(), limit)
    })
    .await
    .map_err(|error| ipc_error("internal_error", error.to_string()))?
}

#[tauri::command]
pub fn cancel_browser_request(
    path: String,
    state: State<'_, DesktopState>,
) -> Result<bool, IpcErrorDto> {
    let directory = canonical_directory(&path)?;
    Ok(lock(&state.browser_requests, "browser requests")?.cancel(&path_text(&directory)))
}

#[tauri::command]
pub async fn browser_node(path: String) -> Result<BrowserNodeDto, IpcErrorDto> {
    tauri::async_runtime::spawn_blocking(move || {
        BrowserNodeDto::from(&FileSystemBrowser::node(Path::new(&path)))
    })
    .await
    .map_err(|error| ipc_error("internal_error", error.to_string()))
}

#[tauri::command]
pub async fn browser_size(
    path: String,
    state: State<'_, DesktopState>,
) -> Result<BrowserSizeDto, IpcErrorDto> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.browser_size(&path))
        .await
        .map_err(|error| ipc_error("internal_error", error.to_string()))?
}

#[tauri::command]
pub fn cancel_browser_size(
    path: String,
    state: State<'_, DesktopState>,
) -> Result<bool, IpcErrorDto> {
    let directory = fs::canonicalize(&path).unwrap_or_else(|_| PathBuf::from(path));
    Ok(lock(&state.browser_size_requests, "browser size requests")?.cancel(&path_text(&directory)))
}

#[tauri::command]
pub fn save_settings(
    settings: SettingsDto,
    state: State<'_, DesktopState>,
) -> Result<SettingsDto, IpcErrorDto> {
    let settings =
        Settings::try_from(settings).map_err(|message| ipc_error("invalid_request", message))?;
    state
        .services
        .save_settings(settings.clone())
        .map_err(ipc_use_case_error)?;
    state
        .scheduler
        .set_max_parallel_runs(settings.execution.max_parallel_runs)
        .map_err(ipc_scheduler_error)?;
    Ok((&settings).into())
}

#[tauri::command]
pub fn save_plan(plan: PlanDto, state: State<'_, DesktopState>) -> Result<PlanDto, IpcErrorDto> {
    let plan = Plan::try_from(plan).map_err(|message| ipc_error("invalid_request", message))?;
    state
        .services
        .save_active_plan(plan.clone())
        .map_err(ipc_use_case_error)?;
    Ok((&plan).into())
}

#[tauri::command]
pub fn add_folder(
    source: String,
    default_profile_id: Option<String>,
    state: State<'_, DesktopState>,
) -> Result<FolderAddResultDto, IpcErrorDto> {
    let default_profile_id = default_profile_id
        .map(|profile_id| parse_id::<ProfileId>("profile", &profile_id))
        .transpose()?;
    let canonical_source = canonical_directory(&source)?;
    let created = !state
        .services
        .state()
        .map_err(ipc_use_case_error)?
        .active_plan
        .folders
        .iter()
        .any(|folder| folder.source == canonical_source);
    let folder = state
        .services
        .add_folder(canonical_source, default_profile_id)
        .map_err(ipc_use_case_error)?;
    Ok(FolderAddResultDto {
        folder: (&folder).into(),
        created,
    })
}

#[tauri::command]
pub fn add_dropped_sources(
    paths: Vec<String>,
    default_profile_id: Option<String>,
    state: State<'_, DesktopState>,
) -> Result<Vec<FolderAddResultDto>, IpcErrorDto> {
    if paths.len() > 256 {
        return Err(ipc_error(
            "invalid_request",
            "at most 256 dropped paths are accepted",
        ));
    }
    let mut results = Vec::new();
    for path in paths {
        if Path::new(&path).is_dir() {
            results.push(add_folder(path, default_profile_id.clone(), state.clone())?);
        }
    }
    Ok(results)
}

#[tauri::command]
pub fn update_folder(
    folder: FolderDto,
    state: State<'_, DesktopState>,
) -> Result<FolderDto, IpcErrorDto> {
    let folder =
        Folder::try_from(folder).map_err(|message| ipc_error("invalid_request", message))?;
    state
        .services
        .update_folder(folder.clone())
        .map_err(ipc_use_case_error)?;
    invalidate_folder_previews(state.inner(), folder.id)?;
    Ok((&folder).into())
}

#[tauri::command]
pub fn unlist_folder(
    folder_id: String,
    cancel_queued: Option<bool>,
    state: State<'_, DesktopState>,
) -> Result<bool, IpcErrorDto> {
    let folder_id = parse_id("folder", &folder_id)?;
    cancel_queued_folder_runs(state.inner(), &[folder_id], cancel_queued.unwrap_or(false))?;
    let removed = state
        .services
        .unlist_folder(folder_id)
        .map_err(ipc_use_case_error)?;
    if removed {
        invalidate_folder_previews(state.inner(), folder_id)?;
    }
    Ok(removed)
}

#[tauri::command]
pub fn unlisted_folders(state: State<'_, DesktopState>) -> Result<Vec<FolderDto>, IpcErrorDto> {
    state
        .services
        .unlisted_folders()
        .map(|folders| folders.iter().map(Into::into).collect())
        .map_err(ipc_use_case_error)
}

#[tauri::command]
pub fn forget_folders(
    folder_ids: Vec<String>,
    cancel_queued: Option<bool>,
    state: State<'_, DesktopState>,
) -> Result<u64, IpcErrorDto> {
    let folder_ids = folder_ids
        .iter()
        .map(|folder_id| parse_id("folder", folder_id))
        .collect::<Result<Vec<_>, _>>()?;
    cancel_queued_folder_runs(state.inner(), &folder_ids, cancel_queued.unwrap_or(false))?;
    let removed = state
        .services
        .forget_folders(&folder_ids)
        .map_err(ipc_use_case_error)?;
    for folder_id in folder_ids {
        invalidate_folder_previews(state.inner(), folder_id)?;
    }
    Ok(removed)
}

#[tauri::command]
pub fn forget_all_unlisted_folders(
    cancel_queued: Option<bool>,
    state: State<'_, DesktopState>,
) -> Result<u64, IpcErrorDto> {
    let folder_ids = state
        .services
        .unlisted_folders()
        .map_err(ipc_use_case_error)?
        .into_iter()
        .map(|folder| folder.id)
        .collect::<Vec<_>>();
    cancel_queued_folder_runs(state.inner(), &folder_ids, cancel_queued.unwrap_or(false))?;
    let removed = state
        .services
        .forget_all_unlisted_folders()
        .map_err(ipc_use_case_error)?;
    for folder_id in folder_ids {
        invalidate_folder_previews(state.inner(), folder_id)?;
    }
    Ok(removed)
}

#[tauri::command]
pub fn add_action(
    folder_id: String,
    action_type: String,
    enabled: bool,
    profile_id_override: Option<String>,
    state: State<'_, DesktopState>,
) -> Result<FolderActionDto, IpcErrorDto> {
    let profile_id_override = profile_id_override
        .map(|profile_id| parse_id("profile", &profile_id))
        .transpose()?;
    if action_type != "archive" {
        return Err(ipc_error(
            "unsupported_action",
            format!("action type `{action_type}` is unsupported"),
        ));
    }
    state
        .services
        .add_archive_action(
            parse_id("folder", &folder_id)?,
            enabled,
            profile_id_override,
        )
        .map(|action| (&action).into())
        .map_err(ipc_use_case_error)
}

#[tauri::command]
pub fn profile_usage(
    profile_id: String,
    state: State<'_, DesktopState>,
) -> Result<u64, IpcErrorDto> {
    state
        .services
        .profile_usage(parse_id("profile", &profile_id)?)
        .map_err(ipc_use_case_error)
}

#[tauri::command]
pub fn browser_favorites(state: State<'_, DesktopState>) -> Result<Vec<String>, IpcErrorDto> {
    state
        .services
        .browser_favorites()
        .map(|paths| paths.iter().map(|path| path_text(path)).collect())
        .map_err(ipc_use_case_error)
}

#[tauri::command]
pub fn browser_recent(state: State<'_, DesktopState>) -> Result<Vec<String>, IpcErrorDto> {
    state
        .services
        .browser_recent()
        .map(|paths| paths.iter().map(|path| path_text(path)).collect())
        .map_err(ipc_use_case_error)
}

#[tauri::command]
pub fn set_browser_view(
    view: BrowserViewDto,
    state: State<'_, DesktopState>,
) -> Result<BrowserViewDto, IpcErrorDto> {
    state
        .services
        .set_browser_view(view.into())
        .map(Into::into)
        .map_err(ipc_use_case_error)
}

#[tauri::command]
pub fn set_favorite(
    path: String,
    favorite: bool,
    state: State<'_, DesktopState>,
) -> Result<Vec<String>, IpcErrorDto> {
    state
        .services
        .set_favorite(PathBuf::from(path), favorite)
        .map(|paths| paths.iter().map(|path| path_text(path)).collect())
        .map_err(ipc_use_case_error)
}

#[tauri::command]
pub fn update_action(
    folder_id: String,
    action: FolderActionDto,
    state: State<'_, DesktopState>,
) -> Result<FolderActionDto, IpcErrorDto> {
    let action = foldry_application::FolderAction::try_from(action)
        .map_err(|message| ipc_error("invalid_request", message))?;
    state
        .services
        .update_action(parse_id("folder", &folder_id)?, action.clone())
        .map_err(ipc_use_case_error)?;
    invalidate_action_preview(state.inner(), parse_id("folder", &folder_id)?, action.id)?;
    Ok((&action).into())
}

#[tauri::command]
pub fn remove_action(
    folder_id: String,
    action_id: String,
    state: State<'_, DesktopState>,
) -> Result<bool, IpcErrorDto> {
    let folder_id = parse_id("folder", &folder_id)?;
    let action_id = parse_id("action", &action_id)?;
    let removed = state
        .services
        .remove_action(folder_id, action_id)
        .map_err(ipc_use_case_error)?;
    if removed {
        invalidate_action_preview(state.inner(), folder_id, action_id)?;
    }
    Ok(removed)
}

#[tauri::command]
pub fn reorder_actions(
    folder_id: String,
    action_ids: Vec<String>,
    state: State<'_, DesktopState>,
) -> Result<(), IpcErrorDto> {
    let action_ids = action_ids
        .iter()
        .map(|action_id| parse_id("action", action_id))
        .collect::<Result<Vec<_>, _>>()?;
    state
        .services
        .reorder_actions(parse_id("folder", &folder_id)?, &action_ids)
        .map_err(ipc_use_case_error)
}

#[tauri::command]
pub fn create_profile(
    name: String,
    state: State<'_, DesktopState>,
) -> Result<StoredProfileDto, IpcErrorDto> {
    state
        .services
        .create_profile(&name)
        .map(|profile| (&profile).into())
        .map_err(ipc_use_case_error)
}

#[tauri::command]
pub fn rename_profile(
    profile_id: String,
    name: String,
    state: State<'_, DesktopState>,
) -> Result<StoredProfileDto, IpcErrorDto> {
    state
        .services
        .rename_profile(parse_id("profile", &profile_id)?, &name)
        .map(|profile| (&profile).into())
        .map_err(ipc_use_case_error)
}

#[tauri::command]
pub fn save_profile(
    filename: String,
    text: String,
    state: State<'_, DesktopState>,
) -> Result<StoredProfileDto, IpcErrorDto> {
    let profile = state
        .services
        .save_profile_text(&filename, &text)
        .map_err(ipc_use_case_error)?;
    invalidate_all_previews(state.inner())?;
    Ok((&profile).into())
}

#[tauri::command]
pub fn delete_profile(
    profile_id: String,
    state: State<'_, DesktopState>,
) -> Result<bool, IpcErrorDto> {
    let deleted = state
        .services
        .delete_profile(parse_id("profile", &profile_id)?)
        .map_err(ipc_use_case_error)?;
    if deleted {
        invalidate_all_previews(state.inner())?;
    }
    Ok(deleted)
}

#[tauri::command]
pub fn restore_default_profile(
    state: State<'_, DesktopState>,
) -> Result<StoredProfileDto, IpcErrorDto> {
    let profile = state
        .services
        .restore_default_profile()
        .map_err(ipc_use_case_error)?;
    invalidate_all_previews(state.inner())?;
    Ok((&profile).into())
}

#[tauri::command]
pub fn save_preset(
    filename: String,
    text: String,
    state: State<'_, DesktopState>,
) -> Result<StoredPresetDto, IpcErrorDto> {
    state
        .services
        .save_preset_text(&filename, &text)
        .map(|preset| (&preset).into())
        .map_err(ipc_use_case_error)
}

#[tauri::command]
pub fn delete_preset(
    preset_id: String,
    state: State<'_, DesktopState>,
) -> Result<bool, IpcErrorDto> {
    state
        .services
        .delete_preset(&parse_id::<PresetId>("preset", &preset_id)?)
        .map_err(ipc_use_case_error)
}

#[tauri::command]
pub fn reset_preset(
    preset_id: String,
    state: State<'_, DesktopState>,
) -> Result<StoredPresetDto, IpcErrorDto> {
    state
        .services
        .reset_preset(&parse_id::<PresetId>("preset", &preset_id)?)
        .map(|preset| (&preset).into())
        .map_err(ipc_use_case_error)
}

#[tauri::command]
pub async fn start_preview(
    folder_id: String,
    action_id: String,
    state: State<'_, DesktopState>,
) -> Result<PreviewStartedDto, IpcErrorDto> {
    let folder_id = parse_id("folder", &folder_id)?;
    let action_id = parse_id("action", &action_id)?;
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.start_preview(folder_id, action_id))
        .await
        .map_err(|error| ipc_error("internal_error", error.to_string()))?
}

#[tauri::command]
pub fn preview_page(
    folder_id: String,
    action_id: String,
    cursor: Option<String>,
    limit: u32,
    filter: PreviewFilterDto,
    state: State<'_, DesktopState>,
) -> Result<PreviewPageDto, IpcErrorDto> {
    let folder_id = parse_id("folder", &folder_id)?;
    let action_id = parse_id("action", &action_id)?;
    let cursor = cursor
        .map_or(Ok(ManifestCursor::default()), |token| {
            ManifestCursor::from_token(&token)
        })
        .map_err(|error| ipc_error("invalid_request", error.to_string()))?;
    let filter = match filter {
        PreviewFilterDto::All => PreviewFilter::All,
        PreviewFilterDto::Included => PreviewFilter::Included,
        PreviewFilterDto::Excluded => PreviewFilter::Excluded,
        PreviewFilterDto::Skipped => PreviewFilter::Skipped,
    };
    let previews = lock(&state.previews, "previews")?;
    let preview = previews
        .get(&(folder_id, action_id))
        .ok_or_else(|| ipc_error("not_found", "preview is not available"))?;
    let page = preview
        .handle
        .page(
            cursor,
            usize::try_from(limit).unwrap_or(usize::MAX),
            filter,
            &CancellationToken::default(),
        )
        .map_err(|error| ipc_error("preview_error", error.to_string()))?;
    Ok(PreviewPageDto {
        entries: page.entries.iter().map(PreviewEntryDto::from).collect(),
        next_cursor: page.next_cursor.map(ManifestCursor::token),
    })
}

#[tauri::command]
pub fn cancel_preview(
    folder_id: String,
    action_id: String,
    state: State<'_, DesktopState>,
) -> Result<bool, IpcErrorDto> {
    let folder_id = parse_id("folder", &folder_id)?;
    let action_id = parse_id("action", &action_id)?;
    let preview_id = (folder_id, action_id);
    let cancelled = lock(&state.preview_requests, "preview requests")?.cancel(&preview_id);
    let removed = lock(&state.previews, "previews")?.remove(&preview_id);
    let had_cached_preview = removed.is_some();
    if let Some(preview) = removed {
        let _ = preview.handle.remove();
    }
    Ok(cancelled || had_cached_preview)
}

#[tauri::command]
pub fn run_action(
    folder_id: String,
    action_id: String,
    state: State<'_, DesktopState>,
) -> Result<RunRecordDto, IpcErrorDto> {
    let run = state
        .services
        .prepare_run_current(
            parse_id("folder", &folder_id)?,
            parse_id("action", &action_id)?,
        )
        .map_err(ipc_use_case_error)?;
    enqueue_if_queued(state.inner(), &run)?;
    Ok((&run).into())
}

#[tauri::command]
pub fn run_folder(
    folder_id: String,
    state: State<'_, DesktopState>,
) -> Result<Vec<RunRecordDto>, IpcErrorDto> {
    let runs = state
        .services
        .prepare_folder_enabled(parse_id("folder", &folder_id)?)
        .map_err(ipc_use_case_error)?;
    for run in &runs {
        enqueue_if_queued(state.inner(), run)?;
    }
    Ok(runs.iter().map(Into::into).collect())
}

#[tauri::command]
pub fn run_all_enabled(state: State<'_, DesktopState>) -> Result<Vec<RunRecordDto>, IpcErrorDto> {
    let runs = state
        .services
        .prepare_all_enabled()
        .map_err(ipc_use_case_error)?;
    for run in &runs {
        enqueue_if_queued(state.inner(), run)?;
    }
    Ok(runs.iter().map(Into::into).collect())
}

#[tauri::command]
pub fn repeat_run(
    run_id: String,
    state: State<'_, DesktopState>,
) -> Result<RunRecordDto, IpcErrorDto> {
    let run = state
        .services
        .repeat_run(parse_id("run", &run_id)?)
        .map_err(ipc_use_case_error)?;
    enqueue_if_queued(state.inner(), &run)?;
    Ok((&run).into())
}

#[tauri::command]
pub fn scheduler_snapshot(
    state: State<'_, DesktopState>,
) -> Result<Vec<RunRecordDto>, IpcErrorDto> {
    state
        .scheduler
        .records()
        .map(|runs| runs.iter().map(Into::into).collect())
        .map_err(ipc_scheduler_error)
}

macro_rules! run_command {
    ($name:ident, $method:ident) => {
        #[tauri::command]
        pub fn $name(run_id: String, state: State<'_, DesktopState>) -> Result<bool, IpcErrorDto> {
            state
                .scheduler
                .$method(parse_id("run", &run_id)?)
                .map_err(ipc_scheduler_error)
        }
    };
}

run_command!(pause_run, pause);
run_command!(resume_run, resume);
run_command!(stop_run, stop);

macro_rules! global_run_command {
    ($name:ident, $method:ident) => {
        #[tauri::command]
        pub fn $name(state: State<'_, DesktopState>) -> Result<u64, IpcErrorDto> {
            state.scheduler.$method().map_err(ipc_scheduler_error)
        }
    };
}

global_run_command!(pause_all, pause_all);
global_run_command!(resume_all, resume_all);
global_run_command!(stop_all, stop_all);

#[tauri::command]
pub fn history_page(
    offset: u64,
    limit: u32,
    folder_id: Option<String>,
    action_id: Option<String>,
    state: State<'_, DesktopState>,
) -> Result<Vec<RunRecordDto>, IpcErrorDto> {
    let folder_id = folder_id
        .as_deref()
        .map(|id| parse_id("folder", id))
        .transpose()?;
    let action_id = action_id
        .as_deref()
        .map(|id| parse_id("action", id))
        .transpose()?;
    state
        .services
        .history_filtered(page_request(offset, limit)?, folder_id, action_id)
        .map(|runs| runs.iter().map(Into::into).collect())
        .map_err(ipc_use_case_error)
}

#[tauri::command]
pub fn run_details(
    run_id: String,
    state: State<'_, DesktopState>,
) -> Result<Option<RunRecordDto>, IpcErrorDto> {
    state
        .services
        .run(parse_id("run", &run_id)?)
        .map(|run| run.as_ref().map(Into::into))
        .map_err(ipc_use_case_error)
}

#[tauri::command]
pub fn logs_page(
    run_id: String,
    offset: u64,
    limit: u32,
    state: State<'_, DesktopState>,
) -> Result<Vec<LogRecordDto>, IpcErrorDto> {
    state
        .services
        .logs(parse_id("run", &run_id)?, page_request(offset, limit)?)
        .map(|logs| logs.iter().map(Into::into).collect())
        .map_err(ipc_use_case_error)
}

#[tauri::command]
pub async fn export_run_logs(
    run_id: String,
    app: AppHandle,
    state: State<'_, DesktopState>,
) -> Result<Option<String>, IpcErrorDto> {
    let run_id = parse_id("run", &run_id)?;
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let Some(selected) = app
            .dialog()
            .file()
            .add_filter("JSON Lines", &["jsonl"])
            .set_file_name(format!("foldry-{run_id}.jsonl"))
            .blocking_save_file()
        else {
            return Ok(None);
        };
        let path = selected
            .into_path()
            .map_err(|error| ipc_error("invalid_path", error.to_string()))?;
        let file = fs::File::create(&path)
            .map_err(|error| ipc_error("filesystem_error", error.to_string()))?;
        let mut writer = BufWriter::new(file);
        let mut offset = 0;
        loop {
            let logs = state
                .services
                .logs(
                    run_id,
                    PageRequest {
                        offset,
                        limit: 1000,
                    },
                )
                .map_err(ipc_use_case_error)?;
            if logs.is_empty() {
                break;
            }
            for log in &logs {
                serde_json::to_writer(&mut writer, &LogRecordDto::from(log))
                    .map_err(|error| ipc_error("serialization_error", error.to_string()))?;
                writer
                    .write_all(b"\n")
                    .map_err(|error| ipc_error("filesystem_error", error.to_string()))?;
            }
            offset += u64::try_from(logs.len()).unwrap_or(u64::MAX);
            if logs.len() < 1000 {
                break;
            }
        }
        writer
            .flush()
            .map_err(|error| ipc_error("filesystem_error", error.to_string()))?;
        Ok(Some(path_text(&path)))
    })
    .await
    .map_err(|error| ipc_error("internal_error", error.to_string()))?
}

#[tauri::command]
pub async fn pick_folders(app: AppHandle) -> Result<Vec<String>, IpcErrorDto> {
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .blocking_pick_folders()
            .unwrap_or_default()
            .into_iter()
            .map(|path| {
                path.into_path()
                    .map_err(|error| ipc_error("invalid_path", error.to_string()))
                    .and_then(|path| canonical_directory(&path_text(&path)))
                    .map(|path| path_text(&path))
            })
            .collect()
    })
    .await
    .map_err(|error| ipc_error("internal_error", error.to_string()))?
}

#[tauri::command]
pub fn reveal_run_output(
    run_id: String,
    app: AppHandle,
    state: State<'_, DesktopState>,
) -> Result<(), IpcErrorDto> {
    let run = state
        .services
        .run(parse_id("run", &run_id)?)
        .map_err(ipc_use_case_error)?
        .ok_or_else(|| ipc_error("not_found", "run was not found"))?;
    let artifact = run
        .summary
        .and_then(|summary| summary.artifact)
        .ok_or_else(|| ipc_error("not_found", "run has no output artifact"))?;
    reveal_validated_artifact(&artifact.path, &app)
}

fn reveal_validated_artifact(artifact: &Path, app: &AppHandle) -> Result<(), IpcErrorDto> {
    let metadata = fs::symlink_metadata(artifact)
        .map_err(|error| ipc_error("filesystem_error", error.to_string()))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(ipc_error(
            "invalid_path",
            "run artifact is not a regular file",
        ));
    }
    app.opener()
        .reveal_item_in_dir(artifact)
        .map_err(|error| ipc_error("desktop_error", error.to_string()))
}

fn page_request(offset: u64, limit: u32) -> Result<PageRequest, IpcErrorDto> {
    if !(1..=1000).contains(&limit) {
        return Err(ipc_error(
            "invalid_request",
            "page limit must be between 1 and 1000",
        ));
    }
    Ok(PageRequest { offset, limit })
}

fn canonical_directory(path: &str) -> Result<PathBuf, IpcErrorDto> {
    if path.trim().is_empty() {
        return Err(ipc_error("invalid_path", "directory path cannot be empty"));
    }
    let path = Path::new(path);
    if !path.is_absolute() {
        return Err(ipc_error("invalid_path", "directory path must be absolute"));
    }
    let canonical =
        fs::canonicalize(path).map_err(|error| ipc_error("filesystem_error", error.to_string()))?;
    if !canonical.is_dir() {
        return Err(ipc_error("invalid_path", "path is not a directory"));
    }
    Ok(canonical)
}

fn parse_id<T>(kind: &str, value: &str) -> Result<T, IpcErrorDto>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    value
        .parse()
        .map_err(|error| ipc_error("invalid_id", format!("invalid {kind} ID: {error}")))
}

fn ipc_use_case_error(error: UseCaseError) -> IpcErrorDto {
    match error {
        UseCaseError::Repository(error) => ipc_error("storage_error", error.to_string()),
        UseCaseError::NotFound(message) => ipc_error("not_found", message),
        UseCaseError::Conflict(message) => ipc_error("conflict", message),
        UseCaseError::Invalid(message) => ipc_error("invalid_request", message),
        UseCaseError::InvalidProfile {
            profile_id,
            diagnostics,
        } => ipc_error_details(
            "invalid_profile",
            format!("profile {profile_id} is invalid"),
            json!(diagnostics),
        ),
    }
}

fn ipc_scheduler_error(error: foldry_application::SchedulerError) -> IpcErrorDto {
    ipc_error("scheduler_error", error.to_string())
}

fn cancel_queued_folder_runs(
    state: &DesktopState,
    folder_ids: &[FolderId],
    confirmed: bool,
) -> Result<(), IpcErrorDto> {
    let active = state
        .scheduler
        .records()
        .map_err(ipc_scheduler_error)?
        .into_iter()
        .filter(|run| {
            folder_ids.contains(&run.folder_id)
                && matches!(
                    run.state,
                    RunState::Queued
                        | RunState::Planning
                        | RunState::Running
                        | RunState::Paused
                        | RunState::Stopping
                )
        })
        .collect::<Vec<_>>();
    if active.is_empty() {
        return Ok(());
    }
    if active.iter().any(|run| run.state != RunState::Queued) {
        return Err(ipc_error_details(
            "active_runs",
            "stop running or paused actions before removing the folder",
            json!({"run_ids": active.iter().map(|run| run.run_id.to_string()).collect::<Vec<_>>()}),
        ));
    }
    if !confirmed {
        return Err(ipc_error_details(
            "queued_runs_require_confirmation",
            "queued actions must be cancelled before removing the folder",
            json!({"run_ids": active.iter().map(|run| run.run_id.to_string()).collect::<Vec<_>>()}),
        ));
    }
    for run in active {
        state
            .scheduler
            .stop(run.run_id)
            .map_err(ipc_scheduler_error)?;
    }
    Ok(())
}

fn enqueue_if_queued(state: &DesktopState, run: &RunRecord) -> Result<(), IpcErrorDto> {
    if run.state == RunState::Queued {
        state
            .scheduler
            .enqueue(run.clone())
            .map_err(ipc_scheduler_error)?;
    }
    Ok(())
}

fn invalidate_action_preview(
    state: &DesktopState,
    folder_id: FolderId,
    action_id: ActionId,
) -> Result<(), IpcErrorDto> {
    let preview_id = (folder_id, action_id);
    lock(&state.preview_requests, "preview requests")?.cancel(&preview_id);
    if let Some(preview) = lock(&state.previews, "previews")?.remove(&preview_id) {
        let _ = preview.handle.remove();
    }
    Ok(())
}

fn invalidate_folder_previews(
    state: &DesktopState,
    folder_id: FolderId,
) -> Result<(), IpcErrorDto> {
    lock(&state.preview_requests, "preview requests")?
        .cancel_where(|(candidate, _)| *candidate == folder_id);
    let action_ids = lock(&state.previews, "previews")?
        .keys()
        .filter_map(|(candidate, action_id)| (*candidate == folder_id).then_some(*action_id))
        .collect::<Vec<_>>();
    for action_id in action_ids {
        invalidate_action_preview(state, folder_id, action_id)?;
    }
    Ok(())
}

fn invalidate_all_previews(state: &DesktopState) -> Result<(), IpcErrorDto> {
    lock(&state.preview_requests, "preview requests")?.cancel_where(|_| true);
    let preview_ids = lock(&state.previews, "previews")?
        .keys()
        .copied()
        .collect::<Vec<_>>();
    for (folder_id, action_id) in preview_ids {
        invalidate_action_preview(state, folder_id, action_id)?;
    }
    Ok(())
}

fn ipc_error(code: impl Into<String>, message: impl Into<String>) -> IpcErrorDto {
    IpcErrorDto {
        code: code.into(),
        message: message.into(),
        details: None,
    }
}

fn ipc_error_details(
    code: impl Into<String>,
    message: impl Into<String>,
    details: serde_json::Value,
) -> IpcErrorDto {
    IpcErrorDto {
        code: code.into(),
        message: message.into(),
        details: Some(details),
    }
}

fn lock<'a, T>(mutex: &'a Mutex<T>, description: &str) -> Result<MutexGuard<'a, T>, IpcErrorDto> {
    mutex
        .lock()
        .map_err(|_| ipc_error("internal_error", format!("{description} lock is poisoned")))
}

fn resource_directory(app: &AppHandle) -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("FOLDRY_RESOURCE_DIR").map(PathBuf::from) {
        return validate_resource_directory(path);
    }
    let platform_resources = app.path().resource_dir().map_err(display_error)?;
    for candidate in [
        platform_resources.join("resources"),
        platform_resources,
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../resources"),
    ] {
        if candidate.join("profiles/default.packignore").is_file()
            && candidate.join("presets").is_dir()
        {
            return Ok(candidate);
        }
    }
    Err("Foldry resource profiles and presets are unavailable".into())
}

fn validate_resource_directory(path: PathBuf) -> Result<PathBuf, String> {
    if path.join("profiles/default.packignore").is_file() && path.join("presets").is_dir() {
        Ok(path)
    } else {
        Err(format!(
            "resource directory {} does not contain Foldry resources",
            path.display()
        ))
    }
}

fn output_directories(plan: &Plan) -> Vec<PathBuf> {
    plan.folders
        .iter()
        .flat_map(|folder| {
            folder
                .actions
                .iter()
                .filter_map(|action| match &action.spec {
                    ActionSpec::Archive(action) => action.output.directory.resolve(&folder.source),
                    ActionSpec::Unsupported(_) => None,
                })
        })
        .collect()
}

fn empty_plan() -> Plan {
    foldry_application::Plan {
        version: foldry_application::PlanVersion::CURRENT,
        name: "Active plan".into(),
        folders: Vec::new(),
        extensions: foldry_application::Extensions::new(),
    }
}

fn home_directory() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE").map(PathBuf::from)
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn display_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, sync::Arc};

    use foldry_application::NoopRunEventSink;
    use foldry_storage::AppDirectories;
    use tempfile::tempdir;

    use super::{COMMAND_NAMES, DesktopState, canonical_directory, page_request};

    #[test]
    fn command_surface_is_explicit_and_has_no_duplicates() {
        let mut names = COMMAND_NAMES.to_vec();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), COMMAND_NAMES.len());
        assert!(COMMAND_NAMES.contains(&"bootstrap_snapshot"));
        assert!(COMMAND_NAMES.contains(&"reveal_run_output"));
    }

    #[test]
    fn paging_is_bounded() {
        assert!(page_request(0, 1).is_ok());
        assert!(page_request(0, 1000).is_ok());
        assert!(page_request(0, 0).is_err());
        assert!(page_request(0, 1001).is_err());
    }

    #[test]
    fn filesystem_inputs_must_be_absolute_directories() {
        assert!(canonical_directory("relative").is_err());
    }

    #[test]
    fn runtime_bootstrap_survives_a_fresh_desktop_layout() {
        let temporary = tempdir().expect("temporary directory");
        let directories = AppDirectories {
            config: temporary.path().join("config"),
            data: temporary.path().join("data"),
            cache: temporary.path().join("cache"),
        };
        let resources = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../resources");
        let state = DesktopState::open_with(&directories, &resources, Arc::new(NoopRunEventSink))
            .expect("desktop runtime");

        let snapshot = state.bootstrap().expect("bootstrap snapshot");
        assert_eq!(snapshot.version, 1);
        assert!(!snapshot.profiles.is_empty());
        assert!(!snapshot.presets.is_empty());
        assert!(directories.database().is_file());
    }
}
