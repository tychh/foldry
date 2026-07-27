use std::{fs, sync::Arc, thread};

use foldry_application::{
    ActivePlanRepository, Appearance, ExecutionSettings, Extensions, Locale, PresetRepository,
    ProfileRepository, SettingsRepository,
};
use foldry_storage::{
    AppDirectories, DirectoryOverrides, FileActivePlanRepository, FilePresetRepository,
    FileProfileRepository, FileSettingsRepository, decode_plan, initialize_resource_copies,
    install_missing_resources,
};

#[test]
fn directory_overrides_create_the_expected_layout() {
    let root = tempfile::tempdir().unwrap();
    let directories = AppDirectories::resolve(&DirectoryOverrides {
        config: Some(root.path().join("config")),
        data: Some(root.path().join("data")),
        cache: Some(root.path().join("cache")),
    })
    .unwrap();

    directories.ensure_layout().unwrap();

    assert!(directories.profiles().is_dir());
    assert!(directories.presets().is_dir());
    assert!(directories.crash_reports().is_dir());
    assert!(directories.manifests().is_dir());
    assert_eq!(
        directories.settings(),
        root.path().join("config/settings.yaml")
    );
    assert_eq!(directories.database(), root.path().join("data/app.db"));
}

#[test]
fn settings_and_active_plan_survive_repository_recreation() {
    let root = tempfile::tempdir().unwrap();
    let settings_path = root.path().join("config/settings.yaml");
    let plan_path = root.path().join("config/active.packplan.yaml");
    let settings = foldry_application::Settings {
        locale: Locale::Ru,
        appearance: Appearance::Dark,
        ..Default::default()
    };
    let plan = fixture_plan();

    FileSettingsRepository::new(settings_path.clone())
        .save(&settings)
        .unwrap();
    FileActivePlanRepository::new(plan_path.clone())
        .save(&plan)
        .unwrap();

    assert_eq!(
        FileSettingsRepository::new(settings_path).load().unwrap(),
        Some(settings)
    );
    assert_eq!(
        FileActivePlanRepository::new(plan_path).load().unwrap(),
        Some(plan)
    );
}

#[test]
fn corrupt_settings_are_reported_and_never_replaced_during_load() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("settings.yaml");
    let corrupt = "version: 1\nlocale: [broken\n";
    fs::write(&path, corrupt).unwrap();

    let error = FileSettingsRepository::new(path.clone())
        .load()
        .unwrap_err();

    assert!(!error.message.is_empty());
    assert_eq!(fs::read_to_string(path).unwrap(), corrupt);
}

#[test]
fn invalid_profile_is_saved_while_previous_good_version_is_preserved() {
    let root = tempfile::tempdir().unwrap();
    let profiles = root.path().join("profiles");
    let resource = resource_profile();
    let repository = FileProfileRepository::new(profiles.clone(), resource.clone());
    let valid = fs::read_to_string(resource).unwrap();
    let stored = repository.save_text("custom.packignore", &valid).unwrap();
    assert!(stored.valid);

    let invalid = "# deliberately invalid: missing metadata\n*.tmp\n";
    let stored = repository.save_text("custom.packignore", invalid).unwrap();

    assert!(!stored.valid);
    assert_eq!(stored.text, invalid);
    assert!(!stored.diagnostics.is_empty());
    assert_eq!(
        fs::read_to_string(profiles.join("custom.packignore.previous-good")).unwrap(),
        valid
    );
    assert_eq!(repository.list().unwrap().len(), 1);
}

#[test]
fn installing_resources_never_overwrites_a_working_copy_and_reset_is_explicit() {
    let root = tempfile::tempdir().unwrap();
    let resources = root.path().join("resources");
    let working = root.path().join("working");
    fs::create_dir_all(&resources).unwrap();
    let original = "# @preset-id rust\n# @preset-version 1\n# @preset-name Rust\n\ntarget/\n";
    fs::write(resources.join("rust.packignore"), original).unwrap();

    assert_eq!(install_missing_resources(&resources, &working).unwrap(), 1);
    fs::write(
        working.join("rust.packignore"),
        format!("{original}custom/\n"),
    )
    .unwrap();
    assert_eq!(install_missing_resources(&resources, &working).unwrap(), 0);
    assert!(
        fs::read_to_string(working.join("rust.packignore"))
            .unwrap()
            .contains("custom/")
    );

    let repository = FilePresetRepository::new(working.clone(), resources);
    let restored = repository
        .reset_from_resources(&"rust".parse().unwrap())
        .unwrap();
    assert_eq!(restored.text, original);
}

#[test]
fn concurrent_atomic_settings_saves_leave_one_complete_document() {
    let root = tempfile::tempdir().unwrap();
    let path = Arc::new(root.path().join("settings.yaml"));
    let threads = (0..8)
        .map(|index| {
            let path = Arc::clone(&path);
            thread::spawn(move || {
                let settings = foldry_application::Settings {
                    locale: if index % 2 == 0 {
                        Locale::En
                    } else {
                        Locale::Ru
                    },
                    execution: ExecutionSettings {
                        max_parallel_runs: index + 1,
                        extensions: Extensions::new(),
                    },
                    ..Default::default()
                };
                FileSettingsRepository::new((*path).clone())
                    .save(&settings)
                    .unwrap();
            })
        })
        .collect::<Vec<_>>();
    for handle in threads {
        handle.join().unwrap();
    }

    let loaded = FileSettingsRepository::new((*path).clone())
        .load()
        .unwrap()
        .unwrap();
    assert!((1..=8).contains(&loaded.execution.max_parallel_runs));
}

#[test]
fn one_time_resource_initialization_preserves_later_deletions() {
    let root = tempfile::tempdir().unwrap();
    let config = root.path().join("config");
    let resources = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../resources");

    assert!(initialize_resource_copies(&resources, &config).unwrap());
    let preset = config.join("presets/rust.packignore");
    assert!(preset.exists());
    fs::remove_file(&preset).unwrap();

    assert!(!initialize_resource_copies(&resources, &config).unwrap());
    assert!(!preset.exists());
}

fn fixture_plan() -> foldry_application::Plan {
    let source = fs::read_to_string(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/formats/v1/plan.packplan.yaml"),
    )
    .unwrap();
    decode_plan(&source).unwrap()
}

fn resource_profile() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../resources/profiles/default.packignore")
}
