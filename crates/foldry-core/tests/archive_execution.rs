use std::{collections::BTreeMap, fs, path::PathBuf, time::UNIX_EPOCH};

use foldry_core::{
    ActionVersion, ArchiveActionSpec, ArchiveFormat, ArchiveOutputSpec, ChecksumAlgorithm,
    CompressionLevel, ConflictPolicy, ExecutionControl, ExecutionEntrySource, ExecutionPlan,
    ExecutionWarning, FileSystemObjectKind, PlanOutput, RunId, ScanDisposition, ScanSummary,
    ScannedEntry, UnreadablePolicy, VerificationMode, VerificationSpec, execute_archive,
    reserve_output,
};

struct Entries(std::vec::IntoIter<ScannedEntry>);

impl ExecutionEntrySource for Entries {
    fn next_entry(&mut self) -> Result<Option<ScannedEntry>, String> {
        Ok(self.0.next())
    }
}

fn action(
    output: &std::path::Path,
    format: ArchiveFormat,
    unreadable_policy: UnreadablePolicy,
) -> ArchiveActionSpec {
    ArchiveActionSpec {
        version: ActionVersion::V1,
        output: ArchiveOutputSpec {
            directory: output.to_path_buf(),
            filename: "backup".to_owned(),
            format,
            compression: CompressionLevel::Fast,
            conflict_policy: ConflictPolicy::Overwrite,
            extensions: BTreeMap::new(),
        },
        include_root: true,
        unreadable_policy,
        verification: VerificationSpec {
            mode: VerificationMode::Full,
            checksum: ChecksumAlgorithm::Sha256,
            extensions: BTreeMap::new(),
        },
        extensions: BTreeMap::new(),
    }
}

fn scanned_file(path: PathBuf, relative_path: &str) -> ScannedEntry {
    let metadata = fs::metadata(&path).expect("metadata");
    ScannedEntry {
        relative_path: relative_path.to_owned(),
        native_path: path,
        kind: FileSystemObjectKind::RegularFile,
        disposition: ScanDisposition::Included,
        size: metadata.len(),
        modified_unix_nanos: metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .and_then(|duration| u64::try_from(duration.as_nanos()).ok()),
        link_target: None,
        is_mount_point: false,
        is_network_mount: false,
        reason: None,
    }
}

#[test]
fn execution_publishes_all_formats_after_full_verification() {
    for format in [
        ArchiveFormat::Zip,
        ArchiveFormat::TarGz,
        ArchiveFormat::TarZst,
    ] {
        let source = tempfile::tempdir().expect("source");
        let output = tempfile::tempdir().expect("output");
        let file_path = source.path().join("данные.txt");
        fs::write(&file_path, "payload").expect("source file");
        let action = action(output.path(), format, UnreadablePolicy::Fail);
        let PlanOutput::Reserved(reservation) =
            reserve_output(source.path(), &action.output, RunId::new()).expect("reservation")
        else {
            panic!("must reserve");
        };
        let mut entries = Entries(vec![scanned_file(file_path, "данные.txt")].into_iter());
        let plan = ExecutionPlan {
            source_root: source.path().to_path_buf(),
            action,
            totals: ScanSummary::default(),
        };

        let result = execute_archive(
            &plan,
            reservation,
            &mut entries,
            &ExecutionControl::default(),
            |_| {},
        )
        .expect("execution");

        assert!(result.output_path.exists());
        assert!(result.output_size > 0);
        assert_eq!(result.checksum_sha256.as_deref().map(str::len), Some(64));
        assert_eq!(result.progress.processed_files, 1);
        assert_eq!(result.progress.processed_bytes, 7);
    }
}

#[test]
fn stop_removes_temp_and_reservation_without_publishing() {
    let source = tempfile::tempdir().expect("source");
    let output = tempfile::tempdir().expect("output");
    let file_path = source.path().join("file");
    fs::write(&file_path, "payload").expect("source file");
    let action = action(output.path(), ArchiveFormat::Zip, UnreadablePolicy::Fail);
    let expected_target = output.path().join("backup.zip");
    fs::write(&expected_target, "old archive").expect("old archive");
    let PlanOutput::Reserved(reservation) =
        reserve_output(source.path(), &action.output, RunId::new()).expect("reservation")
    else {
        panic!("must reserve");
    };
    let final_path = reservation.final_path().to_path_buf();
    let temp_path = reservation.temp_path().to_path_buf();
    let lock_path = reservation.reservation_path().to_path_buf();
    let mut entries = Entries(vec![scanned_file(file_path, "file")].into_iter());
    let plan = ExecutionPlan {
        source_root: source.path().to_path_buf(),
        action,
        totals: ScanSummary::default(),
    };
    let control = ExecutionControl::default();
    control.stop();

    assert!(execute_archive(&plan, reservation, &mut entries, &control, |_| {}).is_err());
    assert_eq!(final_path, expected_target);
    assert_eq!(
        fs::read_to_string(final_path).expect("old archive remains"),
        "old archive"
    );
    assert!(!temp_path.exists());
    assert!(!lock_path.exists());
}

#[test]
fn warn_and_skip_publishes_remaining_files_with_a_typed_warning() {
    let source = tempfile::tempdir().expect("source");
    let output = tempfile::tempdir().expect("output");
    let good_path = source.path().join("good");
    fs::write(&good_path, "good").expect("good file");
    let missing_path = source.path().join("missing");
    let action = action(
        output.path(),
        ArchiveFormat::Zip,
        UnreadablePolicy::WarnAndSkip,
    );
    let PlanOutput::Reserved(reservation) =
        reserve_output(source.path(), &action.output, RunId::new()).expect("reservation")
    else {
        panic!("must reserve");
    };
    let missing = ScannedEntry {
        relative_path: "missing".to_owned(),
        native_path: missing_path,
        kind: FileSystemObjectKind::Unreadable,
        disposition: ScanDisposition::Skipped,
        size: 0,
        modified_unix_nanos: None,
        link_target: None,
        is_mount_point: false,
        is_network_mount: false,
        reason: None,
    };
    let mut entries = Entries(vec![missing, scanned_file(good_path, "good")].into_iter());
    let plan = ExecutionPlan {
        source_root: source.path().to_path_buf(),
        action,
        totals: ScanSummary::default(),
    };

    let result = execute_archive(
        &plan,
        reservation,
        &mut entries,
        &ExecutionControl::default(),
        |_| {},
    )
    .expect("execution with warning");

    assert!(result.output_path.exists());
    assert_eq!(
        result.warnings,
        vec![ExecutionWarning::UnreadableEntrySkipped(
            "missing".to_owned()
        )]
    );
}

#[test]
fn output_inside_source_is_excluded_and_include_root_false_flattens_layout() {
    let source = tempfile::tempdir().expect("source");
    let good_path = source.path().join("good.txt");
    let old_output = source.path().join("backup.zip");
    fs::write(&good_path, "good").expect("good file");
    fs::write(&old_output, "old archive").expect("old output");
    let mut action = action(source.path(), ArchiveFormat::Zip, UnreadablePolicy::Fail);
    action.include_root = false;
    let PlanOutput::Reserved(reservation) =
        reserve_output(source.path(), &action.output, RunId::new()).expect("reservation")
    else {
        panic!("must reserve");
    };
    let mut entries = Entries(
        vec![
            scanned_file(good_path, "good.txt"),
            scanned_file(old_output, "backup.zip"),
        ]
        .into_iter(),
    );
    let plan = ExecutionPlan {
        source_root: source.path().to_path_buf(),
        action,
        totals: ScanSummary::default(),
    };

    let result = execute_archive(
        &plan,
        reservation,
        &mut entries,
        &ExecutionControl::default(),
        |_| {},
    )
    .expect("execution");
    let mut archive =
        zip::ZipArchive::new(fs::File::open(result.output_path).expect("archive")).expect("ZIP");

    assert!(archive.by_name("good.txt").is_ok());
    assert!(archive.by_name("backup.zip").is_err());
}

#[test]
fn execution_rejects_manifest_paths_outside_the_source_root() {
    let source = tempfile::tempdir().expect("source");
    let outside = tempfile::tempdir().expect("outside");
    let output = tempfile::tempdir().expect("output");
    let outside_file = outside.path().join("secret.txt");
    fs::write(&outside_file, "secret").expect("outside file");
    let action = action(output.path(), ArchiveFormat::Zip, UnreadablePolicy::Fail);
    let PlanOutput::Reserved(reservation) =
        reserve_output(source.path(), &action.output, RunId::new()).expect("reservation")
    else {
        panic!("must reserve");
    };
    let final_path = reservation.final_path().to_path_buf();
    let mut entries = Entries(vec![scanned_file(outside_file, "secret.txt")].into_iter());
    let plan = ExecutionPlan {
        source_root: source.path().to_path_buf(),
        action,
        totals: ScanSummary::default(),
    };

    let error = execute_archive(
        &plan,
        reservation,
        &mut entries,
        &ExecutionControl::default(),
        |_| {},
    )
    .expect_err("untrusted manifest path must fail");

    assert!(error.to_string().contains("escapes the source root"));
    assert!(!final_path.exists());
}

#[cfg(unix)]
#[test]
fn execution_does_not_follow_a_file_replaced_by_a_symlink() {
    use std::os::unix::fs::symlink;

    let source = tempfile::tempdir().expect("source");
    let outside = tempfile::tempdir().expect("outside");
    let output = tempfile::tempdir().expect("output");
    let source_file = source.path().join("document.txt");
    let outside_file = outside.path().join("secret.txt");
    fs::write(&source_file, "public").expect("source file");
    fs::write(&outside_file, "secret").expect("outside file");
    let planned_entry = scanned_file(source_file.clone(), "document.txt");
    fs::remove_file(&source_file).expect("replace source");
    symlink(&outside_file, &source_file).expect("replacement symlink");
    let action = action(output.path(), ArchiveFormat::Zip, UnreadablePolicy::Fail);
    let PlanOutput::Reserved(reservation) =
        reserve_output(source.path(), &action.output, RunId::new()).expect("reservation")
    else {
        panic!("must reserve");
    };
    let final_path = reservation.final_path().to_path_buf();
    let mut entries = Entries(vec![planned_entry].into_iter());
    let plan = ExecutionPlan {
        source_root: source.path().to_path_buf(),
        action,
        totals: ScanSummary::default(),
    };

    assert!(
        execute_archive(
            &plan,
            reservation,
            &mut entries,
            &ExecutionControl::default(),
            |_| {},
        )
        .is_err()
    );
    assert!(!final_path.exists());
}
