use std::{fs, path::PathBuf};

use foldry_application::{
    ApplicationPorts, ApplicationServices, Clock, IdGenerator, Locale, PageRequest, ProfileId,
    RunId, Settings, TaskId, UseCaseError,
};
use foldry_storage::{
    AppDirectories, DirectoryOverrides, FileActivePlanRepository, FilePresetRepository,
    FileProfileRepository, FileSettingsRepository, SqliteRepository, decode_plan,
    install_missing_resources,
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

    fn task_id(&self) -> TaskId {
        TaskId::new()
    }

    fn profile_id(&self) -> ProfileId {
        ProfileId::new()
    }
}

#[test]
fn application_state_and_tasks_survive_a_full_service_restart() {
    let root = tempfile::tempdir().unwrap();
    let directories = directories(root.path());
    let service = build_service(&directories);
    let profile = service.restore_default_profile().unwrap();
    let source = root.path().join("source");
    fs::create_dir(&source).unwrap();
    let task = service
        .add_task(
            source.canonicalize().unwrap(),
            true,
            profile.id.unwrap(),
            fixture_action(),
        )
        .unwrap();
    let settings = Settings {
        locale: Locale::Ru,
        ..Default::default()
    };
    service.save_settings(settings.clone()).unwrap();
    drop(service);

    let restored = build_service(&directories);
    let state = restored.state().unwrap();

    assert_eq!(state.settings, settings);
    assert_eq!(state.active_plan.tasks, vec![task]);
    assert!(directories.settings().is_file());
    assert!(directories.active_plan().is_file());
}

#[test]
fn task_use_cases_reject_duplicate_sources_and_profiles_in_use() {
    let root = tempfile::tempdir().unwrap();
    let directories = directories(root.path());
    let service = build_service(&directories);
    let profile = service.restore_default_profile().unwrap();
    let profile_id = profile.id.unwrap();
    let source = root.path().join("source");
    fs::create_dir(&source).unwrap();
    let task = service
        .add_task(source.clone(), true, profile_id, fixture_action())
        .unwrap();

    assert!(matches!(
        service.add_task(source, true, profile_id, fixture_action()),
        Err(UseCaseError::Conflict(_))
    ));
    assert!(matches!(
        service.delete_profile(profile_id),
        Err(UseCaseError::Conflict(_))
    ));
    assert!(service.remove_task(task.id).unwrap());
    assert!(service.delete_profile(profile_id).unwrap());
}

#[test]
fn profile_create_and_rename_keep_a_stable_uuid_and_filename() {
    let root = tempfile::tempdir().unwrap();
    let directories = directories(root.path());
    let service = build_service(&directories);

    let created = service.create_profile("Frontend cache").unwrap();
    let profile_id = created.id.unwrap();
    assert_eq!(
        created.path.file_name().unwrap(),
        "frontend-cache.packignore"
    );
    assert!(created.valid);
    assert!(created.text.contains(&profile_id.to_string()));

    let renamed = service
        .rename_profile(profile_id, "Frontend artifacts")
        .unwrap();
    assert_eq!(renamed.id, Some(profile_id));
    assert_eq!(renamed.path, created.path);
    assert_eq!(renamed.name, "Frontend artifacts");
    assert!(renamed.text.contains("# @profile-name Frontend artifacts"));

    assert!(matches!(
        service.create_profile(" \n "),
        Err(UseCaseError::Invalid(_))
    ));
}

#[test]
fn invalid_profile_remains_addressable_but_blocks_preview_and_new_runs() {
    let root = tempfile::tempdir().unwrap();
    let directories = directories(root.path());
    let service = build_service(&directories);
    let profile = service.restore_default_profile().unwrap();
    let profile_id = profile.id.unwrap();
    let source = root.path().join("source");
    fs::create_dir(&source).unwrap();
    let task = service
        .add_task(source, true, profile_id, fixture_action())
        .unwrap();
    let invalid = format!(
        "# @profile-id {profile_id}\n# @profile-version 1\n# @profile-name Broken\n\
         # @preset-begin id=rust version=1\ntarget/\n"
    );
    let stored = service
        .save_profile_text("default.packignore", &invalid)
        .unwrap();

    assert_eq!(stored.id, Some(profile_id));
    assert!(!stored.valid);
    assert!(matches!(
        service.prepare_preview(task.id),
        Err(UseCaseError::InvalidProfile { .. })
    ));
    assert!(matches!(
        service.prepare_run_current(task.id),
        Err(UseCaseError::InvalidProfile { .. })
    ));
}

#[test]
fn repeat_uses_the_historical_snapshot_while_current_run_uses_current_state() {
    let root = tempfile::tempdir().unwrap();
    let directories = directories(root.path());
    let service = build_service(&directories);
    let profile = service.restore_default_profile().unwrap();
    let source = root.path().join("source");
    fs::create_dir(&source).unwrap();
    let task = service
        .add_task(source, true, profile.id.unwrap(), fixture_action())
        .unwrap();
    let first = service.prepare_run_current(task.id).unwrap();

    let changed_profile = format!("{}\nchanged/\n", profile.text);
    service
        .save_profile_text("default.packignore", &changed_profile)
        .unwrap();
    let changed_settings = Settings {
        locale: Locale::Ru,
        ..Default::default()
    };
    service.save_settings(changed_settings).unwrap();

    let repeated = service.repeat_run(first.run_id).unwrap();
    let current = service.prepare_run_current(task.id).unwrap();

    assert_eq!(repeated.snapshot, first.snapshot);
    assert_ne!(current.snapshot.profile_hash, first.snapshot.profile_hash);
    assert_ne!(current.snapshot.settings, first.snapshot.settings);
    assert_eq!(
        service
            .history(PageRequest {
                offset: 0,
                limit: 10
            })
            .unwrap()
            .len(),
        3
    );
}

#[test]
fn prepare_all_queues_only_enabled_tasks() {
    let root = tempfile::tempdir().unwrap();
    let directories = directories(root.path());
    let service = build_service(&directories);
    let profile = service.restore_default_profile().unwrap();
    let enabled_source = root.path().join("enabled");
    let disabled_source = root.path().join("disabled");
    fs::create_dir(&enabled_source).unwrap();
    fs::create_dir(&disabled_source).unwrap();
    let enabled = service
        .add_task(enabled_source, true, profile.id.unwrap(), fixture_action())
        .unwrap();
    service
        .add_task(
            disabled_source,
            false,
            profile.id.unwrap(),
            fixture_action(),
        )
        .unwrap();

    let runs = service.prepare_all_enabled().unwrap();

    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].task_id, enabled.id);
    assert_eq!(runs[0].state, foldry_application::RunState::Queued);
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

fn directories(root: &std::path::Path) -> AppDirectories {
    AppDirectories::resolve(&DirectoryOverrides {
        config: Some(root.join("config")),
        data: Some(root.join("data")),
        cache: Some(root.join("cache")),
    })
    .unwrap()
}

fn fixture_action() -> Vec<foldry_application::ActionSpec> {
    let source = fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/formats/v1/plan.packplan.yaml"),
    )
    .unwrap();
    decode_plan(&source).unwrap().tasks.remove(0).steps
}

fn resource_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../resources")
}
