use std::{fs, path::PathBuf};

use foldry_application::{
    ActionCheckpoint, ArchiveArtifact, ErrorCode, Extensions, FoldryError, LogLevel, LogRecord,
    LogRepository, OutputDirectoryRegistry, PageRequest, ResultSummary, RunHistoryRepository,
    RunOutcome, RunRecord, RunSnapshot, RunState, Settings, SourceFingerprintSummary,
    TerminalRunCommit,
};
use foldry_storage::{SqliteRepository, decode_plan};
use jiff::Timestamp;
use rusqlite::Connection;

#[test]
fn runs_and_logs_survive_reopen_and_update_does_not_delete_logs() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("data/app.db");
    let mut run = sample_run("2026-01-01T00:00:00Z", RunState::Running);
    {
        let repository = SqliteRepository::open(&path).unwrap();
        repository.insert(&run).unwrap();
        repository.append(&sample_log(&run, 1)).unwrap();
        run.state = RunState::Failed;
        run.finished_at = Some(timestamp("2026-01-01T00:01:00Z"));
        run.summary = Some(failed_summary());
        repository.update(&run).unwrap();
    }

    let repository = SqliteRepository::open(&path).unwrap();
    assert_eq!(repository.get(run.run_id).unwrap(), Some(run.clone()));
    assert_eq!(
        RunHistoryRepository::page(
            &repository,
            PageRequest {
                offset: 0,
                limit: 10
            }
        )
        .unwrap(),
        vec![run.clone()]
    );
    assert_eq!(
        LogRepository::page(
            &repository,
            run.run_id,
            PageRequest {
                offset: 0,
                limit: 10
            }
        )
        .unwrap(),
        vec![sample_log(&run, 1)]
    );
}

#[test]
fn startup_marks_only_unfinished_runs_as_interrupted() {
    let repository = SqliteRepository::open_in_memory().unwrap();
    let running = sample_run("2026-01-01T00:00:00Z", RunState::Running);
    let succeeded = sample_run("2026-01-02T00:00:00Z", RunState::Succeeded);
    repository.insert(&running).unwrap();
    repository.insert(&succeeded).unwrap();

    let changed = repository
        .mark_unfinished_interrupted(timestamp("2026-01-03T00:00:00Z"))
        .unwrap();

    assert_eq!(changed, 1);
    let interrupted = repository.get(running.run_id).unwrap().unwrap();
    assert_eq!(interrupted.state, RunState::Interrupted);
    assert_eq!(
        interrupted.finished_at,
        Some(timestamp("2026-01-03T00:00:00Z"))
    );
    assert_eq!(
        repository.get(succeeded.run_id).unwrap().unwrap().state,
        RunState::Succeeded
    );
}

#[test]
fn run_and_log_retention_apply_both_age_and_count_limits() {
    let repository = SqliteRepository::open_in_memory().unwrap();
    let old = sample_run("2024-01-01T00:00:00Z", RunState::Succeeded);
    let middle = sample_run("2026-01-01T00:00:00Z", RunState::Succeeded);
    let latest = sample_run("2026-02-01T00:00:00Z", RunState::Succeeded);
    for run in [&old, &middle, &latest] {
        repository.insert(run).unwrap();
        repository.append(&sample_log(run, 1)).unwrap();
    }

    let deleted_runs = RunHistoryRepository::apply_retention(
        &repository,
        timestamp("2026-03-01T00:00:00Z"),
        365,
        1,
        false,
    )
    .unwrap();

    assert_eq!(deleted_runs, 2);
    assert!(repository.get(old.run_id).unwrap().is_none());
    assert!(repository.get(middle.run_id).unwrap().is_none());
    assert!(repository.get(latest.run_id).unwrap().is_some());

    let another = sample_run("2026-02-02T00:00:00Z", RunState::Succeeded);
    repository.insert(&another).unwrap();
    repository.append(&sample_log(&another, 1)).unwrap();
    let deleted_logs = LogRepository::apply_retention(
        &repository,
        timestamp("2026-03-01T00:00:00Z"),
        90,
        1,
        false,
    )
    .unwrap();
    assert_eq!(deleted_logs, 1);
    assert!(
        LogRepository::page(
            &repository,
            latest.run_id,
            PageRequest {
                offset: 0,
                limit: 10
            }
        )
        .unwrap()
        .is_empty()
    );
}

#[test]
fn unlimited_retention_does_nothing() {
    let repository = SqliteRepository::open_in_memory().unwrap();
    let run = sample_run("2020-01-01T00:00:00Z", RunState::Succeeded);
    repository.insert(&run).unwrap();

    assert_eq!(
        RunHistoryRepository::apply_retention(
            &repository,
            timestamp("2026-01-01T00:00:00Z"),
            1,
            1,
            true,
        )
        .unwrap(),
        0
    );
    assert!(repository.get(run.run_id).unwrap().is_some());
}

#[test]
fn migration_is_transactional_and_future_schemas_are_rejected() {
    let root = tempfile::tempdir().unwrap();
    let broken = root.path().join("broken.db");
    {
        let connection = Connection::open(&broken).unwrap();
        connection
            .execute_batch("CREATE TABLE runs (unexpected TEXT);")
            .unwrap();
    }
    assert!(SqliteRepository::open(&broken).is_err());
    let connection = Connection::open(&broken).unwrap();
    assert_eq!(
        connection
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert!(
        connection
            .prepare("SELECT name FROM sqlite_master WHERE name = 'logs'")
            .unwrap()
            .query([])
            .unwrap()
            .next()
            .unwrap()
            .is_none()
    );

    let future = root.path().join("future.db");
    Connection::open(&future)
        .unwrap()
        .execute_batch("PRAGMA user_version = 99;")
        .unwrap();
    let error = SqliteRepository::open(&future).err().unwrap();
    assert!(error.message.contains("newer than supported"));
}

#[test]
fn v1_migration_preserves_runs_and_logs_and_adds_output_registry() {
    let root = tempfile::tempdir().unwrap();
    let database = root.path().join("app.db");
    let output = root.path().join("output");
    fs::create_dir(&output).unwrap();
    let run = sample_run("2026-01-01T00:00:00Z", RunState::Succeeded);
    {
        let repository = SqliteRepository::open(&database).unwrap();
        repository.insert(&run).unwrap();
        repository.append(&sample_log(&run, 1)).unwrap();
    }
    Connection::open(&database)
        .unwrap()
        .execute_batch(
            "DROP TABLE output_directories;
             DROP TABLE action_checkpoints;
             DROP TABLE action_operational_state;
             PRAGMA user_version = 1;",
        )
        .unwrap();

    let repository = SqliteRepository::open(&database).unwrap();
    assert_eq!(repository.get(run.run_id).unwrap(), Some(run.clone()));
    assert_eq!(
        LogRepository::page(
            &repository,
            run.run_id,
            PageRequest {
                offset: 0,
                limit: 10,
            },
        )
        .unwrap(),
        vec![sample_log(&run, 1)]
    );
    repository.register(&output).unwrap();
    repository.register(&output).unwrap();
    assert_eq!(
        repository.known_directories().unwrap(),
        vec![output.canonicalize().unwrap()]
    );
}

#[test]
fn output_registry_prunes_directories_not_in_the_active_plan() {
    let root = tempfile::tempdir().unwrap();
    let keep = root.path().join("keep");
    let stale = root.path().join("stale");
    fs::create_dir(&keep).unwrap();
    fs::create_dir(&stale).unwrap();
    let repository = SqliteRepository::open_in_memory().unwrap();
    repository.register(&keep).unwrap();
    repository.register(&stale).unwrap();

    assert_eq!(
        repository
            .prune_except(std::slice::from_ref(&keep))
            .unwrap(),
        1
    );
    assert_eq!(
        repository.known_directories().unwrap(),
        vec![fs::canonicalize(keep).unwrap()]
    );
}

#[test]
fn terminal_commit_and_operational_records_survive_history_retention() {
    let root = tempfile::tempdir().unwrap();
    let artifact_path = root.path().join("archive.zip");
    fs::write(&artifact_path, "archive").unwrap();
    let repository = SqliteRepository::open_in_memory().unwrap();
    let mut run = sample_run("2026-01-01T00:00:00Z", RunState::Running);
    repository.insert(&run).unwrap();
    run.state = RunState::Succeeded;
    run.finished_at = Some(timestamp("2026-01-01T00:01:00Z"));
    run.summary = Some(ResultSummary {
        outcome: RunOutcome::Succeeded,
        included_entries: 2,
        skipped_entries: 0,
        source_bytes: 7,
        duration_ms: 100,
        artifact: Some(ArchiveArtifact {
            path: artifact_path.clone(),
            size_bytes: 7,
            checksum_sha256: None,
        }),
        warnings: Vec::new(),
        error: None,
        skip_reason: None,
    });
    let checkpoint = ActionCheckpoint {
        folder_id: run.folder_id,
        action_id: run.action_id,
        algorithm_version: 1,
        source_fingerprint: "sha256:source".into(),
        effective_profile_hash: "sha256:profile".into(),
        source_summary: SourceFingerprintSummary {
            included_entries: 2,
            included_bytes: 7,
        },
        run_id: run.run_id,
        completed_at: run.finished_at.unwrap(),
    };
    repository
        .commit_terminal_run(&TerminalRunCommit {
            run: run.clone(),
            checkpoint: Some(checkpoint.clone()),
        })
        .unwrap();

    RunHistoryRepository::apply_retention(
        &repository,
        timestamp("2027-01-01T00:00:00Z"),
        1,
        1,
        false,
    )
    .unwrap();
    assert!(repository.get(run.run_id).unwrap().is_none());
    assert_eq!(
        repository.checkpoints(&[run.folder_id]).unwrap(),
        vec![checkpoint]
    );
    let states = repository.operational_states(&[run.folder_id]).unwrap();
    assert_eq!(states.len(), 1);
    assert_eq!(states[0].latest_outcome, RunOutcome::Succeeded);
    assert_eq!(
        states[0]
            .last_successful_artifact
            .as_ref()
            .map(|artifact| &artifact.path),
        Some(&artifact_path)
    );
}

#[test]
fn terminal_commit_rolls_back_all_parts_on_checkpoint_failure() {
    let root = tempfile::tempdir().unwrap();
    let database = root.path().join("atomic.db");
    let repository = SqliteRepository::open(&database).unwrap();
    let mut run = sample_run("2026-01-01T00:00:00Z", RunState::Running);
    repository.insert(&run).unwrap();
    Connection::open(&database)
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER reject_checkpoint BEFORE INSERT ON action_checkpoints
             BEGIN SELECT RAISE(ABORT, 'fault injection'); END;",
        )
        .unwrap();
    run.state = RunState::Succeeded;
    run.finished_at = Some(timestamp("2026-01-01T00:01:00Z"));
    run.summary = Some(ResultSummary {
        outcome: RunOutcome::Succeeded,
        included_entries: 1,
        skipped_entries: 0,
        source_bytes: 1,
        duration_ms: 1,
        artifact: None,
        warnings: Vec::new(),
        error: None,
        skip_reason: None,
    });
    let checkpoint = ActionCheckpoint {
        folder_id: run.folder_id,
        action_id: run.action_id,
        algorithm_version: 1,
        source_fingerprint: "fingerprint".into(),
        effective_profile_hash: "profile".into(),
        source_summary: SourceFingerprintSummary {
            included_entries: 1,
            included_bytes: 1,
        },
        run_id: run.run_id,
        completed_at: run.finished_at.unwrap(),
    };

    assert!(
        repository
            .commit_terminal_run(&TerminalRunCommit {
                run: run.clone(),
                checkpoint: Some(checkpoint),
            })
            .is_err()
    );
    assert_eq!(
        repository.get(run.run_id).unwrap().unwrap().state,
        RunState::Running
    );
    assert!(repository.checkpoints(&[run.folder_id]).unwrap().is_empty());
    assert!(
        repository
            .operational_states(&[run.folder_id])
            .unwrap()
            .is_empty()
    );
}

fn sample_run(started_at: &str, state: RunState) -> RunRecord {
    let plan = fixture_plan();
    let mut folder = plan.folders[0].clone();
    let action = folder.actions.remove(0);
    RunRecord {
        run_id: foldry_application::RunId::new(),
        folder_id: folder.id,
        action_id: action.id,
        state,
        started_at: timestamp(started_at),
        finished_at: None,
        snapshot: RunSnapshot {
            folder: foldry_application::FolderSnapshot {
                id: folder.id,
                source: folder.source,
            },
            action,
            effective_profile_id: folder.default_profile_id,
            settings: Settings::default(),
            profile_text: "# profile snapshot".into(),
            profile_hash: "sha256:test".into(),
            effective_profile: None,
        },
        summary: None,
    }
}

fn sample_log(run: &RunRecord, sequence: u64) -> LogRecord {
    LogRecord {
        run_id: run.run_id,
        sequence,
        occurred_at: run.started_at,
        level: LogLevel::Info,
        message: "message".into(),
        path: Some("source/file.txt".into()),
    }
}

fn failed_summary() -> ResultSummary {
    ResultSummary {
        outcome: RunOutcome::Failed,
        included_entries: 1,
        skipped_entries: 0,
        source_bytes: 5,
        duration_ms: 10,
        artifact: None,
        warnings: Vec::new(),
        error: Some(FoldryError {
            code: ErrorCode::ReadFailed,
            message: "failed".into(),
            retryable: true,
            path: Some("source/file.txt".into()),
            extensions: Extensions::new(),
        }),
        skip_reason: None,
    }
}

fn fixture_plan() -> foldry_application::Plan {
    let source = fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/formats/v2/plan.packplan.yaml"),
    )
    .unwrap();
    decode_plan(&source).unwrap()
}

fn timestamp(value: &str) -> Timestamp {
    value.parse().unwrap()
}
