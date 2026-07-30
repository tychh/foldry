use std::{fs, path::PathBuf};

use foldry_application::{
    ActionId, ApplicationPorts, ApplicationServices, BrowserView, Clock, FolderId, IdGenerator,
    Locale, PageRequest, ProfileId, RunId, RunState, Settings, UseCaseError,
};
use foldry_storage::{
    AppDirectories, DirectoryOverrides, FileActivePlanRepository, FilePresetRepository,
    FileProfileRepository, FileSettingsRepository, SqliteRepository, install_missing_resources,
};
use jiff::Timestamp;

#[derive(Clone, Copy)]
struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> Timestamp {
        "2026-07-27T12:00:00Z".parse().unwrap()
    }
}

#[derive(Clone, Copy)]
struct TestIds;

impl IdGenerator for TestIds {
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

#[test]
fn folders_and_actions_survive_a_full_service_restart() {
    let root = tempfile::tempdir().unwrap();
    let directories = directories(root.path());
    let service = build_service(&directories);
    let profile = default_profile(&service);
    let source = create_source(root.path(), "source");
    let folder = service.add_folder(source, profile.id).unwrap();
    let settings = Settings {
        locale: Locale::Ru,
        default_profile_id: profile.id,
        ..Default::default()
    };
    service.save_settings(settings.clone()).unwrap();
    drop(service);

    let restored = build_service(&directories);
    let state = restored.state().unwrap();

    assert_eq!(state.settings, settings);
    assert_eq!(state.active_plan.folders, vec![folder]);
    assert!(state.active_plan.folders[0].listed);
    assert_eq!(state.active_plan.folders[0].actions.len(), 1);
    assert!(!state.active_plan.folders[0].actions[0].enabled);
}

#[test]
fn relist_reuses_folder_and_deleted_archive_is_not_recreated() {
    let root = tempfile::tempdir().unwrap();
    let directories = directories(root.path());
    let service = build_service(&directories);
    let source = create_source(root.path(), "source");
    let folder = service.add_folder(source.clone(), None).unwrap();
    let archive_id = folder.actions[0].id;

    assert!(service.remove_action(folder.id, archive_id).unwrap());
    assert!(service.unlist_folder(folder.id).unwrap());
    let relisted = service.add_folder(source, None).unwrap();

    assert_eq!(relisted.id, folder.id);
    assert!(relisted.listed);
    assert!(relisted.actions.is_empty());

    drop(service);
    let restored = build_service(&directories);
    assert!(
        restored.state().unwrap().active_plan.folders[0]
            .actions
            .is_empty()
    );
}

#[test]
fn action_crud_allows_multiple_types_and_preserves_explicit_order() {
    let root = tempfile::tempdir().unwrap();
    let service = build_service(&directories(root.path()));
    let folder = service
        .add_folder(create_source(root.path(), "source"), None)
        .unwrap();
    let first = folder.actions[0].clone();
    let second = service
        .add_action(folder.id, true, None, first.spec.clone())
        .unwrap();

    service
        .reorder_actions(folder.id, &[second.id, first.id])
        .unwrap();
    let actions = service.state().unwrap().active_plan.folders[0]
        .actions
        .clone();
    assert_eq!(
        actions.iter().map(|action| action.id).collect::<Vec<_>>(),
        vec![second.id, first.id]
    );

    assert!(service.remove_action(folder.id, first.id).unwrap());
    let third = service.add_archive_action(folder.id, false, None).unwrap();
    assert_ne!(third.id, first.id);
    assert_eq!(third.spec, first.spec);
}

#[test]
fn hidden_folders_can_be_forgotten_but_listed_folders_cannot() {
    let root = tempfile::tempdir().unwrap();
    let service = build_service(&directories(root.path()));
    let first = service
        .add_folder(create_source(root.path(), "first"), None)
        .unwrap();
    let second = service
        .add_folder(create_source(root.path(), "second"), None)
        .unwrap();

    assert!(matches!(
        service.forget_folders(&[first.id]),
        Err(UseCaseError::Conflict(_))
    ));
    service.unlist_folder(first.id).unwrap();
    assert_eq!(service.unlisted_folders().unwrap().len(), 1);
    assert_eq!(service.forget_folders(&[first.id]).unwrap(), 1);
    assert_eq!(
        service.state().unwrap().active_plan.folders[0].id,
        second.id
    );
}

#[test]
fn non_terminal_runs_block_action_removal_unlist_and_forget() {
    let root = tempfile::tempdir().unwrap();
    let service = build_service(&directories(root.path()));
    let folder = service
        .add_folder(create_source(root.path(), "source"), None)
        .unwrap();
    let action_id = folder.actions[0].id;
    let mut run = service.prepare_run_current(folder.id, action_id).unwrap();

    assert!(matches!(
        service.remove_action(folder.id, action_id),
        Err(UseCaseError::Conflict(_))
    ));
    assert!(matches!(
        service.unlist_folder(folder.id),
        Err(UseCaseError::Conflict(_))
    ));

    run.state = RunState::Stopped;
    run.finished_at = Some("2026-07-27T12:00:01Z".parse().unwrap());
    service.update_run(&run).unwrap();
    assert!(service.unlist_folder(folder.id).unwrap());

    let second = service.prepare_run_current(folder.id, action_id).unwrap();
    assert!(matches!(
        service.forget_folders(&[folder.id]),
        Err(UseCaseError::Conflict(_))
    ));
    let mut stopped = second;
    stopped.state = RunState::Stopped;
    stopped.finished_at = Some("2026-07-27T12:00:02Z".parse().unwrap());
    service.update_run(&stopped).unwrap();
    assert_eq!(service.forget_folders(&[folder.id]).unwrap(), 1);
}

#[test]
fn repeated_action_start_reuses_the_non_terminal_run() {
    let root = tempfile::tempdir().unwrap();
    let service = build_service(&directories(root.path()));
    let folder = service
        .add_folder(create_source(root.path(), "source"), None)
        .unwrap();
    let action_id = folder.actions[0].id;

    let queued = service.prepare_run_current(folder.id, action_id).unwrap();
    assert_eq!(
        service
            .prepare_run_current(folder.id, action_id)
            .unwrap()
            .run_id,
        queued.run_id
    );

    let mut running = queued.clone();
    running.state = RunState::Running;
    service.update_run(&running).unwrap();
    assert_eq!(
        service
            .prepare_run_current(folder.id, action_id)
            .unwrap()
            .run_id,
        queued.run_id
    );

    running.state = RunState::Paused;
    service.update_run(&running).unwrap();
    assert_eq!(
        service
            .prepare_run_current(folder.id, action_id)
            .unwrap()
            .run_id,
        queued.run_id
    );
    assert_eq!(
        service
            .history(PageRequest {
                offset: 0,
                limit: 10
            })
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn favorites_and_recent_are_persisted_deduplicated_and_bounded() {
    let root = tempfile::tempdir().unwrap();
    let directories = directories(root.path());
    let service = build_service(&directories);
    let favorite = create_source(root.path(), "favorite");
    let canonical_favorite = fs::canonicalize(&favorite).unwrap();
    service.set_favorite(favorite.clone(), true).unwrap();
    service.set_favorite(favorite.clone(), true).unwrap();
    service.set_browser_view(BrowserView::List).unwrap();

    let mut parents = Vec::new();
    for index in 0..12 {
        let parent = root.path().join(format!("parent-{index}"));
        let source = parent.join("source");
        fs::create_dir_all(&source).unwrap();
        service.add_folder(source, None).unwrap();
        parents.push(parent);
    }

    assert_eq!(
        service.browser_favorites().unwrap(),
        vec![canonical_favorite.clone()]
    );
    let recent = service.browser_recent().unwrap();
    assert_eq!(recent.len(), 10);
    assert_eq!(recent[0], fs::canonicalize(&parents[11]).unwrap());
    assert_eq!(recent[9], fs::canonicalize(&parents[2]).unwrap());
    drop(service);

    fs::remove_dir_all(&parents[11]).unwrap();
    let restored = build_service(&directories);
    assert_eq!(
        restored.browser_favorites().unwrap(),
        vec![canonical_favorite]
    );
    assert_eq!(
        restored.state().unwrap().settings.browser.view,
        BrowserView::List
    );
    assert_eq!(restored.state().unwrap().settings.browser.recent.len(), 10);
    assert_eq!(restored.browser_recent().unwrap().len(), 9);
}

#[test]
fn default_profile_is_editable_but_cannot_be_deleted() {
    let root = tempfile::tempdir().unwrap();
    let directories = directories(root.path());
    let service = build_service(&directories);
    let profile = default_profile(&service);
    let profile_id = profile.id.unwrap();
    let edited = format!("{}\ncustom-system-junk\n", profile.text);

    let saved = service
        .save_profile_text("default.packignore", &edited)
        .unwrap();

    assert!(saved.valid);
    assert_eq!(saved.text, edited);
    assert!(matches!(
        service.delete_profile(profile_id),
        Err(UseCaseError::Conflict(_))
    ));
}

#[test]
fn deleting_a_used_profile_repairs_folder_and_action_references() {
    let root = tempfile::tempdir().unwrap();
    let service = build_service(&directories(root.path()));
    let default_id = default_profile(&service).id.unwrap();
    let custom = service.create_profile("Temporary").unwrap();
    let custom_id = custom.id.unwrap();
    let mut folder = service
        .add_folder(create_source(root.path(), "source"), Some(custom_id))
        .unwrap();
    folder.actions[0].profile_id_override = Some(custom_id);
    service.update_folder(folder).unwrap();

    assert!(service.delete_profile(custom_id).unwrap());
    let repaired = &service.state().unwrap().active_plan.folders[0];
    assert_eq!(repaired.default_profile_id, default_id);
    assert_eq!(repaired.actions[0].profile_id_override, None);
}

#[test]
fn bootstrap_restores_default_and_repairs_missing_profile_references() {
    let root = tempfile::tempdir().unwrap();
    let directories = directories(root.path());
    let service = build_service(&directories);
    let default = default_profile(&service);
    let custom = service.create_profile("Temporary").unwrap();
    let source = create_source(root.path(), "source");
    service.add_folder(source, custom.id).unwrap();
    service
        .save_settings(Settings {
            default_profile_id: custom.id,
            ..Default::default()
        })
        .unwrap();
    drop(service);
    fs::remove_file(custom.path).unwrap();
    fs::remove_file(default.path).unwrap();

    let restored = build_service(&directories);
    let restored_default = default_profile(&restored);
    let default_id = restored_default.id.unwrap();
    let state = restored.state().unwrap();

    assert_eq!(state.settings.default_profile_id, Some(default_id));
    assert_eq!(state.active_plan.folders[0].default_profile_id, default_id);
}

#[test]
fn invalid_profile_blocks_preview_and_run_but_disabled_action_runs_manually() {
    let root = tempfile::tempdir().unwrap();
    let directories = directories(root.path());
    let service = build_service(&directories);
    let profile = default_profile(&service);
    let profile_id = profile.id.unwrap();
    let mut folder = service
        .add_folder(create_source(root.path(), "source"), Some(profile_id))
        .unwrap();
    folder.enabled = false;
    service.update_folder(folder.clone()).unwrap();
    let action_id = folder.actions[0].id;

    let run = service
        .prepare_run_current(folder.id, action_id)
        .expect("manual run ignores disabled flags");
    assert_eq!(run.action_id, action_id);

    let invalid = format!(
        "# @profile-id {profile_id}\n# @profile-version 1\n# @profile-name Broken\n\
         # @preset-begin id=rust version=1\ntarget/\n"
    );
    service
        .save_profile_text("default.packignore", &invalid)
        .unwrap();
    assert!(matches!(
        service.prepare_preview(folder.id, action_id),
        Err(UseCaseError::InvalidProfile { .. })
    ));
    assert!(matches!(
        service.prepare_run_current(folder.id, action_id),
        Err(UseCaseError::InvalidProfile { .. })
    ));
}

#[test]
fn repeat_uses_the_historical_snapshot_and_group_run_filters_all_flags() {
    let root = tempfile::tempdir().unwrap();
    let directories = directories(root.path());
    let service = build_service(&directories);
    let profile = default_profile(&service);
    let mut included = service
        .add_folder(create_source(root.path(), "included"), profile.id)
        .unwrap();
    included.actions[0].enabled = true;
    service.update_folder(included.clone()).unwrap();

    let mut hidden = service
        .add_folder(create_source(root.path(), "hidden"), profile.id)
        .unwrap();
    hidden.actions[0].enabled = true;
    service.update_folder(hidden.clone()).unwrap();
    service.unlist_folder(hidden.id).unwrap();

    let disabled = service
        .add_folder(create_source(root.path(), "disabled"), profile.id)
        .unwrap();
    let mut first = service
        .prepare_run_current(included.id, included.actions[0].id)
        .unwrap();
    first.state = RunState::Succeeded;
    first.finished_at = Some("2026-07-27T12:00:01Z".parse().unwrap());
    service.update_run(&first).unwrap();
    service
        .save_settings(Settings {
            locale: Locale::Ru,
            ..Default::default()
        })
        .unwrap();

    let repeated = service.repeat_run(first.run_id).unwrap();
    let runs = service.prepare_all_enabled().unwrap();

    assert_eq!(repeated.snapshot, first.snapshot);
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].folder_id, included.id);
    assert_ne!(runs[0].folder_id, disabled.id);
    assert_eq!(
        service
            .history(PageRequest {
                offset: 0,
                limit: 10
            })
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn group_run_preserves_folder_and_action_order() {
    let root = tempfile::tempdir().unwrap();
    let service = build_service(&directories(root.path()));
    let mut first = service
        .add_folder(create_source(root.path(), "first"), None)
        .unwrap();
    first.actions[0].enabled = true;
    service.update_folder(first.clone()).unwrap();
    let first_extra = service
        .add_action(first.id, true, None, first.actions[0].spec.clone())
        .unwrap();

    let mut second = service
        .add_folder(create_source(root.path(), "second"), None)
        .unwrap();
    second.actions[0].enabled = true;
    service.update_folder(second.clone()).unwrap();

    let runs = service.prepare_all_enabled().unwrap();
    assert_eq!(
        runs.iter()
            .map(|run| (run.folder_id, run.action_id))
            .collect::<Vec<_>>(),
        vec![
            (first.id, first.actions[0].id),
            (first.id, first_extra.id),
            (second.id, second.actions[0].id),
        ]
    );
}

#[test]
fn history_and_repeat_survive_action_removal_and_forgotten_folder() {
    let root = tempfile::tempdir().unwrap();
    let service = build_service(&directories(root.path()));
    let folder = service
        .add_folder(create_source(root.path(), "source"), None)
        .unwrap();
    let action_id = folder.actions[0].id;
    let mut original = service.prepare_run_current(folder.id, action_id).unwrap();
    original.state = RunState::Succeeded;
    original.finished_at = Some("2026-07-27T12:00:01Z".parse().unwrap());
    service.update_run(&original).unwrap();

    assert!(service.remove_action(folder.id, action_id).unwrap());
    assert!(service.unlist_folder(folder.id).unwrap());
    assert_eq!(service.forget_folders(&[folder.id]).unwrap(), 1);
    assert_eq!(
        service.run(original.run_id).unwrap().unwrap().snapshot,
        original.snapshot
    );
    let filtered = service
        .history_filtered(
            PageRequest {
                offset: 0,
                limit: 20,
            },
            Some(folder.id),
            Some(action_id),
        )
        .unwrap();
    assert_eq!(
        filtered.iter().map(|run| run.run_id).collect::<Vec<_>>(),
        vec![original.run_id]
    );

    let repeated = service.repeat_run(original.run_id).unwrap();
    assert_eq!(repeated.snapshot, original.snapshot);
    assert_ne!(repeated.run_id, original.run_id);
}

#[test]
fn profile_create_and_rename_keep_a_stable_uuid_and_filename() {
    let root = tempfile::tempdir().unwrap();
    let service = build_service(&directories(root.path()));
    let created = service.create_profile("Frontend cache").unwrap();
    let profile_id = created.id.unwrap();
    let renamed = service
        .rename_profile(profile_id, "Frontend artifacts")
        .unwrap();

    assert_eq!(renamed.id, Some(profile_id));
    assert_eq!(renamed.path, created.path);
    assert_eq!(renamed.name, "Frontend artifacts");
}

#[test]
fn profile_usage_counts_listed_hidden_defaults_and_action_overrides() {
    let root = tempfile::tempdir().unwrap();
    let service = build_service(&directories(root.path()));
    let first = service
        .add_folder(create_source(root.path(), "first"), None)
        .unwrap();
    let mut second = service
        .add_folder(create_source(root.path(), "second"), None)
        .unwrap();
    second.actions[0].profile_id_override = Some(first.default_profile_id);
    service
        .update_action(second.id, second.actions[0].clone())
        .unwrap();
    assert!(service.unlist_folder(second.id).unwrap());

    assert_eq!(service.profile_usage(first.default_profile_id).unwrap(), 3);
}

fn build_service(directories: &AppDirectories) -> ApplicationServices {
    directories.ensure_layout().unwrap();
    let resources = resource_root();
    install_missing_resources(&resources.join("presets"), &directories.presets()).unwrap();
    ApplicationServices::bootstrap(ApplicationPorts {
        settings: Box::new(FileSettingsRepository::new(directories.settings())),
        active_plan: Box::new(FileActivePlanRepository::new(directories.active_plan())),
        profiles: Box::new(FileProfileRepository::new(
            directories.profiles(),
            resources.join("profiles/default.packignore"),
        )),
        presets: Box::new(FilePresetRepository::new(
            directories.presets(),
            resources.join("presets"),
        )),
        history: Box::new(SqliteRepository::open(&directories.database()).unwrap()),
        logs: Box::new(SqliteRepository::open(&directories.database()).unwrap()),
        clock: Box::new(FixedClock),
        ids: Box::new(TestIds),
    })
    .unwrap()
}

fn default_profile(service: &ApplicationServices) -> foldry_application::StoredProfile {
    service
        .profiles()
        .unwrap()
        .into_iter()
        .find(|profile| profile.path.file_name().unwrap() == "default.packignore")
        .unwrap()
}

fn create_source(root: &std::path::Path, name: &str) -> PathBuf {
    let source = root.join(name);
    fs::create_dir(&source).unwrap();
    source
}

fn directories(root: &std::path::Path) -> AppDirectories {
    AppDirectories::resolve(&DirectoryOverrides {
        config: Some(root.join("config")),
        data: Some(root.join("data")),
        cache: Some(root.join("cache")),
    })
    .unwrap()
}

fn resource_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../resources")
}
