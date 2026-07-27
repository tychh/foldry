use std::{collections::BTreeMap, fs, io::Write};

use foldry_core::{
    ArchiveFormat, ArchiveOutputSpec, CompressionLevel, ConflictPolicy, PlanOutput, RunId,
    reserve_output,
};

fn spec(directory: &std::path::Path, policy: ConflictPolicy) -> ArchiveOutputSpec {
    ArchiveOutputSpec {
        directory: directory.to_path_buf(),
        filename: "backup".to_owned(),
        format: ArchiveFormat::Zip,
        compression: CompressionLevel::Balanced,
        conflict_policy: policy,
        extensions: BTreeMap::new(),
    }
}

#[test]
fn skip_never_starts_a_temp_file_when_target_exists() {
    let source = tempfile::tempdir().expect("source");
    let output = tempfile::tempdir().expect("output");
    fs::write(output.path().join("backup.zip"), "old").expect("old archive");

    let planned = reserve_output(
        source.path(),
        &spec(output.path(), ConflictPolicy::Skip),
        RunId::new(),
    )
    .expect("plan");

    assert!(matches!(planned, PlanOutput::Skipped { .. }));
    assert_eq!(fs::read_dir(output.path()).expect("output").count(), 1);
}

#[test]
fn increment_reserves_a_unique_name_without_a_race_window() {
    let source = tempfile::tempdir().expect("source");
    let output = tempfile::tempdir().expect("output");
    fs::write(output.path().join("backup.zip"), "first").expect("first archive");
    let first = reserve_output(
        source.path(),
        &spec(output.path(), ConflictPolicy::Increment),
        RunId::new(),
    )
    .expect("first reservation");
    let second = reserve_output(
        source.path(),
        &spec(output.path(), ConflictPolicy::Increment),
        RunId::new(),
    )
    .expect("second reservation");

    let PlanOutput::Reserved(first) = first else {
        panic!("first must reserve");
    };
    let PlanOutput::Reserved(second) = second else {
        panic!("second must reserve");
    };
    assert_eq!(
        first
            .final_path()
            .file_name()
            .and_then(|name| name.to_str()),
        Some("backup (1).zip")
    );
    assert_eq!(
        second
            .final_path()
            .file_name()
            .and_then(|name| name.to_str()),
        Some("backup (2).zip")
    );
}

#[test]
fn failed_overwrite_keeps_old_archive_and_cleans_owned_artifacts() {
    let source = tempfile::tempdir().expect("source");
    let output = tempfile::tempdir().expect("output");
    let target = output.path().join("backup.zip");
    fs::write(&target, "old").expect("old archive");
    let reservation = reserve_output(
        source.path(),
        &spec(output.path(), ConflictPolicy::Overwrite),
        RunId::new(),
    )
    .expect("reservation");
    let PlanOutput::Reserved(reservation) = reservation else {
        panic!("must reserve");
    };
    let temp = reservation.temp_path().to_path_buf();
    let lock = reservation.reservation_path().to_path_buf();

    drop(reservation);

    assert_eq!(fs::read_to_string(target).expect("old archive"), "old");
    assert!(!temp.exists());
    assert!(!lock.exists());
}

#[test]
fn finalized_overwrite_atomically_publishes_new_archive() {
    let source = tempfile::tempdir().expect("source");
    let output = tempfile::tempdir().expect("output");
    let target = output.path().join("backup.zip");
    fs::write(&target, "old").expect("old archive");
    let reservation = reserve_output(
        source.path(),
        &spec(output.path(), ConflictPolicy::Overwrite),
        RunId::new(),
    )
    .expect("reservation");
    let PlanOutput::Reserved(mut reservation) = reservation else {
        panic!("must reserve");
    };
    let mut temp = reservation.take_temp_file().expect("temp file");
    temp.write_all(b"new").expect("new archive");
    temp.sync_all().expect("sync");
    drop(temp);

    let published = reservation.publish().expect("publish");

    assert_eq!(published, target);
    assert_eq!(fs::read_to_string(target).expect("new archive"), "new");
}
