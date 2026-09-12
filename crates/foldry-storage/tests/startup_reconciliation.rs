use std::{
    fs,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, SystemTime},
};

use foldry_application::{
    LEGACY_RESERVATION_METADATA_VERSION, RESERVATION_METADATA_VERSION, ReservationMetadata,
    ReservationOwnerKind, RunHistoryRepository, RunId, RunState,
};
use foldry_storage::{
    ProcessProbe, RecoveryContext, SqliteRepository, clean_stale_output_artifacts,
    reconcile_startup,
};

struct FixedProcessProbe {
    running: AtomicBool,
}

impl ProcessProbe for FixedProcessProbe {
    fn matches(&self, _process_id: u32, _started_unix_seconds: Option<u64>) -> bool {
        self.running.load(Ordering::Relaxed)
    }
}

fn desktop_recovery(instance_id: &str) -> RecoveryContext {
    RecoveryContext {
        owner_kind: ReservationOwnerKind::Desktop,
        instance_id: instance_id.into(),
        directory_scope_id: "desktop-scope".into(),
        exclusive_scope_guard: true,
    }
}

#[test]
fn cleanup_removes_only_old_verified_artifacts_from_dead_processes() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("output");
    fs::create_dir(&output).unwrap();
    let run_id = RunId::new();
    write_reservation(&output, "archive.zip", run_id, 100, 100);
    fs::write(output.join("unrelated.part"), "keep").unwrap();
    fs::write(
        output.join(".unknown-format.zip.foldry-reserve"),
        run_id.to_string(),
    )
    .unwrap();

    let report = clean_stale_output_artifacts(
        std::slice::from_ref(&output),
        10_000,
        60,
        &FixedProcessProbe {
            running: AtomicBool::new(false),
        },
        &desktop_recovery("current"),
    )
    .unwrap();

    assert_eq!(report.removed_reservations, 1);
    assert_eq!(report.removed_temp_files, 1);
    assert_eq!(report.retained_unverified, 1);
    assert!(output.join("unrelated.part").exists());
    assert!(output.join(".unknown-format.zip.foldry-reserve").exists());
}

#[test]
fn cleanup_retains_recent_or_still_owned_reservations() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path();
    write_reservation(output, "active.zip", RunId::new(), 100, 100);
    let active = FixedProcessProbe {
        running: AtomicBool::new(true),
    };
    let report = clean_stale_output_artifacts(
        &[output.into()],
        10_000,
        60,
        &active,
        &desktop_recovery("current"),
    )
    .unwrap();
    assert_eq!(report.retained_active, 1);

    active.running.store(false, Ordering::Relaxed);
    write_reservation(output, "recent.zip", RunId::new(), 100, 9_990);
    let report = clean_stale_output_artifacts(
        &[output.into()],
        10_000,
        60,
        &active,
        &desktop_recovery("current"),
    )
    .unwrap();
    assert_eq!(report.removed_reservations, 1);
    assert_eq!(report.retained_recent, 1);
}

#[test]
fn v2_dead_instance_is_removed_immediately_but_current_instance_is_retained() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path();
    write_owned_reservation(output, "dead.zip", RunId::new(), "previous", 9_999);
    write_owned_reservation(output, "active.zip", RunId::new(), "current", 9_999);
    let report = clean_stale_output_artifacts(
        &[output.into()],
        10_000,
        24 * 60 * 60,
        &FixedProcessProbe {
            running: AtomicBool::new(true),
        },
        &desktop_recovery("current"),
    )
    .unwrap();

    assert_eq!(report.removed_reservations, 1);
    assert_eq!(report.retained_active, 1);
    assert!(output.join(".active.zip.foldry-reserve").exists());
}

#[test]
#[cfg(unix)]
fn invalid_owned_filename_and_symlink_are_never_removed() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path();
    let run_id = RunId::new();
    write_owned_reservation(output, "unsafe.zip", run_id, "previous", 100);
    let reservation = output.join(".unsafe.zip.foldry-reserve");
    let mut metadata: ReservationMetadata =
        serde_json::from_slice(&fs::read(&reservation).unwrap()).unwrap();
    metadata.temp_file_name = "../outside.part".into();
    fs::write(&reservation, serde_json::to_vec(&metadata).unwrap()).unwrap();

    let linked_run = RunId::new();
    write_owned_reservation(output, "linked.zip", linked_run, "previous", 100);
    let linked_temp = output.join(format!(".linked.zip.{linked_run}.part"));
    fs::remove_file(&linked_temp).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(output.join("target"), &linked_temp).unwrap();

    let report = clean_stale_output_artifacts(
        &[output.into()],
        10_000,
        60,
        &FixedProcessProbe {
            running: AtomicBool::new(false),
        },
        &desktop_recovery("current"),
    )
    .unwrap();
    assert_eq!(report.removed_reservations, 0);
    assert_eq!(report.retained_unverified, 2);
    assert!(reservation.exists());
    assert!(output.join(".linked.zip.foldry-reserve").exists());
}

#[test]
fn startup_reconciles_history_and_only_owned_manifest_files() {
    let root = tempfile::tempdir().unwrap();
    let manifests = root.path().join("manifests");
    fs::create_dir(&manifests).unwrap();
    let old_manifest = manifests.join(format!("{}.foldry-manifest", RunId::new()));
    fs::write(&old_manifest, "{}\n").unwrap();
    let old = SystemTime::now() - Duration::from_secs(7_200);
    set_mtime(&old_manifest, old);
    fs::write(manifests.join("notes.txt"), "keep").unwrap();

    let repository = SqliteRepository::open_in_memory().unwrap();
    let run = sqlite_sample::sample_run(RunState::Running);
    repository.insert(&run).unwrap();
    let at = jiff::Timestamp::now();

    let report = reconcile_startup(
        &repository,
        at,
        &[],
        &manifests,
        60,
        &FixedProcessProbe {
            running: AtomicBool::new(false),
        },
        &desktop_recovery("current"),
    )
    .unwrap();

    assert_eq!(report.interrupted_runs, 1);
    assert_eq!(report.artifacts.removed_manifests, 1);
    assert_eq!(
        repository.get(run.run_id).unwrap().unwrap().state,
        RunState::Interrupted
    );
    assert!(manifests.join("notes.txt").exists());
}

fn write_reservation(
    directory: &Path,
    final_name: &str,
    run_id: RunId,
    process_id: u32,
    created_unix_seconds: i64,
) {
    let temp_file_name = format!(".{final_name}.{run_id}.part");
    let metadata = ReservationMetadata {
        version: LEGACY_RESERVATION_METADATA_VERSION,
        run_id,
        process_id,
        created_unix_seconds,
        temp_file_name: temp_file_name.clone(),
        final_file_name: None,
        owner_kind: None,
        instance_id: None,
        directory_scope_id: None,
        process_started_unix_seconds: None,
    };
    fs::write(
        directory.join(format!(".{final_name}.foldry-reserve")),
        serde_json::to_vec(&metadata).unwrap(),
    )
    .unwrap();
    fs::write(directory.join(temp_file_name), "partial").unwrap();
}

fn write_owned_reservation(
    directory: &Path,
    final_name: &str,
    run_id: RunId,
    instance_id: &str,
    created_unix_seconds: i64,
) {
    let temp_file_name = format!(".{final_name}.{run_id}.part");
    let metadata = ReservationMetadata {
        version: RESERVATION_METADATA_VERSION,
        run_id,
        process_id: 100,
        created_unix_seconds,
        temp_file_name: temp_file_name.clone(),
        final_file_name: Some(final_name.into()),
        owner_kind: Some(ReservationOwnerKind::Desktop),
        instance_id: Some(instance_id.into()),
        directory_scope_id: Some("desktop-scope".into()),
        process_started_unix_seconds: Some(50),
    };
    fs::write(
        directory.join(format!(".{final_name}.foldry-reserve")),
        serde_json::to_vec(&metadata).unwrap(),
    )
    .unwrap();
    fs::write(directory.join(temp_file_name), "partial").unwrap();
}

fn set_mtime(path: &Path, time: SystemTime) {
    fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(time))
        .unwrap();
}

mod sqlite_sample {
    use std::{fs, path::PathBuf};

    use foldry_application::{FolderSnapshot, RunRecord, RunSnapshot, RunState, Settings};
    use foldry_storage::decode_plan;

    pub fn sample_run(state: RunState) -> RunRecord {
        let source = fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/formats/v2/plan.packplan.yaml"),
        )
        .unwrap();
        let mut folder = decode_plan(&source).unwrap().folders.remove(0);
        let action = folder.actions.remove(0);
        RunRecord {
            run_id: foldry_application::RunId::new(),
            folder_id: folder.id,
            action_id: action.id,
            state,
            started_at: "2026-07-27T10:00:00Z".parse().unwrap(),
            finished_at: None,
            snapshot: RunSnapshot {
                folder: FolderSnapshot {
                    id: folder.id,
                    source: folder.source,
                },
                action,
                effective_profile_id: folder.default_profile_id,
                settings: Settings::default(),
                profile_text: "# snapshot".into(),
                profile_hash: "hash".into(),
                effective_profile: None,
            },
            summary: None,
        }
    }
}
