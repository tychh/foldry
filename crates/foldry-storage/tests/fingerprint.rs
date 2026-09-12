use std::path::PathBuf;

use foldry_application::{ExecutionControl, FileSystemObjectKind, ScanDisposition, ScannedEntry};
use foldry_storage::{ManifestWriter, fingerprint_manifest};

fn entry(path: &str, disposition: ScanDisposition, size: u64, modified: u64) -> ScannedEntry {
    ScannedEntry {
        relative_path: path.into(),
        native_path: PathBuf::from(path),
        kind: FileSystemObjectKind::RegularFile,
        disposition,
        size,
        modified_unix_nanos: Some(modified),
        created_unix_nanos: None,
        unix_mode: None,
        windows_attributes: None,
        link_target: None,
        is_mount_point: false,
        is_network_mount: false,
        reason: None,
    }
}

fn fingerprint(entries: &[ScannedEntry]) -> Result<String, String> {
    let directory = tempfile::tempdir().unwrap();
    let mut writer = ManifestWriter::create(directory.path(), "fingerprint").unwrap();
    for entry in entries {
        foldry_application::ScanSink::write_entry(&mut writer, entry).unwrap();
    }
    let handle = writer.finish().unwrap();
    fingerprint_manifest(&handle, &ExecutionControl::default()).map(|value| value.digest)
}

#[test]
fn fingerprint_is_order_independent_and_ignores_excluded_entries() {
    let first = entry("a", ScanDisposition::Included, 1, 10);
    let second = entry("b", ScanDisposition::Included, 2, 20);
    let ignored = entry("ignored", ScanDisposition::Excluded, 4, 30);
    assert_eq!(
        fingerprint(&[first.clone(), second.clone(), ignored]).unwrap(),
        fingerprint(&[second, first]).unwrap()
    );
}

#[test]
fn included_metadata_changes_fingerprint_and_duplicates_are_rejected() {
    let base = entry("file", ScanDisposition::Included, 1, 10);
    let changed = entry("file", ScanDisposition::Included, 2, 10);
    assert_ne!(
        fingerprint(std::slice::from_ref(&base)).unwrap(),
        fingerprint(std::slice::from_ref(&changed)).unwrap()
    );
    assert!(
        fingerprint(&[base.clone(), base])
            .unwrap_err()
            .contains("duplicate")
    );
}

#[test]
fn directory_mtime_is_not_part_of_the_fingerprint() {
    let mut first = entry("directory", ScanDisposition::Included, 0, 10);
    first.kind = FileSystemObjectKind::Directory;
    let mut second = first.clone();
    second.modified_unix_nanos = Some(20);
    assert_eq!(
        fingerprint(&[first]).unwrap(),
        fingerprint(&[second]).unwrap()
    );
}
