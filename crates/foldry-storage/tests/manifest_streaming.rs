use std::path::PathBuf;

use foldry_application::{
    CancellationToken, CompiledProfile, FileSystemCaseSensitivity, FileSystemObjectKind,
    PreviewFilter, ScanDisposition, ScanSink, ScannedEntry, parse_profile,
};
use foldry_storage::{
    ManifestCursor, ManifestWriter, scan_to_manifest, temporary_manifest_directory,
};

fn entry(path: &str, disposition: ScanDisposition) -> ScannedEntry {
    ScannedEntry {
        relative_path: path.to_owned(),
        native_path: PathBuf::from(path),
        kind: FileSystemObjectKind::RegularFile,
        disposition,
        size: 10,
        modified_unix_nanos: Some(1),
        link_target: None,
        is_mount_point: false,
        is_network_mount: false,
        reason: None,
    }
}

#[test]
fn pages_are_bounded_filterable_and_cursor_based() {
    let directory = temporary_manifest_directory().expect("temp manifest directory");
    let mut writer = ManifestWriter::create(directory.path(), "paged").expect("writer");
    writer
        .write_entry(&entry("a", ScanDisposition::Included))
        .expect("a");
    writer
        .write_entry(&entry("b", ScanDisposition::Excluded))
        .expect("b");
    writer
        .write_entry(&entry("c", ScanDisposition::Included))
        .expect("c");
    let handle = writer.finish().expect("finish");
    let cancellation = CancellationToken::default();

    let first = handle
        .page(
            ManifestCursor::default(),
            1,
            PreviewFilter::Included,
            &cancellation,
        )
        .expect("first page");
    assert_eq!(first.entries[0].relative_path, "a");
    let second = handle
        .page(
            first.next_cursor.expect("next cursor"),
            1,
            PreviewFilter::Included,
            &cancellation,
        )
        .expect("second page");
    assert_eq!(second.entries[0].relative_path, "c");
    assert!(second.next_cursor.is_none());
}

#[test]
fn unfinished_and_explicitly_removed_manifests_leave_no_artifacts() {
    let directory = temporary_manifest_directory().expect("temp manifest directory");
    let unfinished_path = {
        let writer = ManifestWriter::create(directory.path(), "unfinished").expect("writer");
        writer.path().to_path_buf()
    };
    assert!(!unfinished_path.exists());

    let handle = ManifestWriter::create(directory.path(), "finished")
        .expect("writer")
        .finish()
        .expect("finish");
    let path = handle.path().to_path_buf();
    assert!(path.exists());
    handle.remove().expect("remove");
    assert!(!path.exists());
}

#[test]
fn manifest_id_cannot_escape_the_cache_directory() {
    let directory = temporary_manifest_directory().expect("temp manifest directory");

    assert!(ManifestWriter::create(directory.path(), "../outside").is_err());
    assert_eq!(
        std::fs::read_dir(directory.path())
            .expect("manifest directory")
            .count(),
        0
    );
}

#[test]
fn cancelled_page_returns_no_partial_result() {
    let directory = temporary_manifest_directory().expect("temp manifest directory");
    let handle = ManifestWriter::create(directory.path(), "cancelled")
        .expect("writer")
        .finish()
        .expect("finish");
    let cancellation = CancellationToken::default();
    cancellation.cancel();

    assert!(
        handle
            .page(
                ManifestCursor::default(),
                10,
                PreviewFilter::All,
                &cancellation,
            )
            .is_err()
    );
}

#[test]
fn scan_to_manifest_connects_matcher_summary_and_paged_preview() {
    let source = tempfile::tempdir().expect("source directory");
    std::fs::write(source.path().join("keep.txt"), "keep").expect("keep");
    std::fs::write(source.path().join("drop.log"), "drop").expect("drop");
    let profile = parse_profile(
        "\
# @profile-id 0190f5f0-7f8b-7d80-a120-4f4f9fe95c20
# @profile-version 1
# @profile-name Manifest integration
*.log
",
    )
    .profile
    .expect("profile");
    let matcher =
        CompiledProfile::new(&profile, FileSystemCaseSensitivity::Sensitive).expect("matcher");
    let directory = temporary_manifest_directory().expect("manifest directory");

    let (handle, summary) = scan_to_manifest(
        directory.path(),
        "integrated",
        source.path(),
        &matcher,
        &CancellationToken::default(),
    )
    .expect("scan manifest");
    let page = handle
        .page(
            ManifestCursor::default(),
            10,
            PreviewFilter::All,
            &CancellationToken::default(),
        )
        .expect("page");

    assert_eq!(summary.included_entries, 1);
    assert_eq!(summary.excluded_entries, 1);
    assert_eq!(page.entries.len(), 2);
    assert_eq!(
        page.entries
            .iter()
            .find(|entry| entry.relative_path == "drop.log")
            .and_then(|entry| entry.reason.as_ref())
            .map(|reason| reason.line),
        Some(4)
    );
}

#[test]
fn cancelled_scan_to_manifest_removes_its_unfinished_file() {
    let source = tempfile::tempdir().expect("source directory");
    let profile = parse_profile(
        "\
# @profile-id 0190f5f0-7f8b-7d80-a120-4f4f9fe95c20
# @profile-version 1
# @profile-name Cancelled manifest
",
    )
    .profile
    .expect("profile");
    let matcher =
        CompiledProfile::new(&profile, FileSystemCaseSensitivity::Sensitive).expect("matcher");
    let directory = temporary_manifest_directory().expect("manifest directory");
    let cancellation = CancellationToken::default();
    cancellation.cancel();

    assert!(
        scan_to_manifest(
            directory.path(),
            "must-disappear",
            source.path(),
            &matcher,
            &cancellation,
        )
        .is_err()
    );
    assert_eq!(
        std::fs::read_dir(directory.path())
            .expect("manifest directory")
            .count(),
        0
    );
}
