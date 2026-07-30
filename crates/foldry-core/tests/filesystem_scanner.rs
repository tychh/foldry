use std::{fs, path::Path};

use foldry_core::{
    CancellationToken, CompiledProfile, FileSystemBrowser, FileSystemCaseSensitivity,
    FileSystemObjectKind, FileSystemScanner, ScanDisposition, ScanNotice, ScanSink, ScanSinkError,
    ScannedEntry, parse_profile,
};

#[derive(Default)]
struct CollectingSink {
    entries: Vec<ScannedEntry>,
    notices: Vec<ScanNotice>,
}

impl ScanSink for CollectingSink {
    fn write_entry(&mut self, entry: &ScannedEntry) -> Result<(), ScanSinkError> {
        self.entries.push(entry.clone());
        Ok(())
    }

    fn write_notice(&mut self, notice: &ScanNotice) -> Result<(), ScanSinkError> {
        self.notices.push(notice.clone());
        Ok(())
    }
}

fn matcher() -> CompiledProfile {
    let text = "\
# @profile-id 0190f5f0-7f8b-7d80-a120-4f4f9fe95c20
# @profile-version 1
# @profile-name Scanner test
ignored/
!ignored/
ignored/*
!ignored/keep.txt
";
    let result = parse_profile(text);
    CompiledProfile::new(
        result.profile.as_ref().expect("valid profile"),
        FileSystemCaseSensitivity::Sensitive,
    )
    .expect("compiled profile")
}

#[test]
fn scanner_streams_explainable_entries_and_prunes_excluded_children() {
    let directory = tempfile::tempdir().expect("temp directory");
    fs::create_dir(directory.path().join("ignored")).expect("ignored directory");
    fs::write(directory.path().join("ignored/keep.txt"), "keep").expect("kept file");
    fs::write(directory.path().join("ignored/drop.log"), "drop").expect("excluded file");
    fs::write(directory.path().join("обычный.txt"), "unicode").expect("unicode file");
    let mut sink = CollectingSink::default();

    let summary = FileSystemScanner::scan(
        directory.path(),
        &matcher(),
        &mut sink,
        &CancellationToken::default(),
    )
    .expect("scan");

    assert_eq!(summary.visited_entries, 4);
    assert_eq!(summary.included_files, 2);
    let excluded = sink
        .entries
        .iter()
        .find(|entry| entry.relative_path == "ignored/drop.log")
        .expect("excluded entry");
    assert_eq!(excluded.disposition, ScanDisposition::Excluded);
    assert_eq!(excluded.reason.as_ref().map(|reason| reason.line), Some(6));
    assert!(
        sink.entries
            .iter()
            .any(|entry| entry.relative_path == "обычный.txt")
    );
}

#[cfg(unix)]
#[test]
fn links_and_special_files_are_not_traversed() {
    use std::os::unix::{fs::symlink, net::UnixListener};

    let directory = tempfile::tempdir().expect("temp directory");
    fs::create_dir(directory.path().join("target")).expect("target directory");
    fs::write(directory.path().join("target/file.txt"), "target").expect("target file");
    symlink("target", directory.path().join("link")).expect("symlink");
    let _socket =
        UnixListener::bind(directory.path().join("socket")).expect("unix domain socket fixture");
    let mut sink = CollectingSink::default();

    let summary = FileSystemScanner::scan(
        directory.path(),
        &matcher(),
        &mut sink,
        &CancellationToken::default(),
    )
    .expect("scan");

    let link = sink
        .entries
        .iter()
        .find(|entry| entry.relative_path == "link")
        .expect("link entry");
    assert_eq!(link.kind, FileSystemObjectKind::Symlink);
    assert_eq!(link.link_target.as_deref(), Some(Path::new("target")));
    assert!(
        !sink
            .entries
            .iter()
            .any(|entry| entry.relative_path.starts_with("link/"))
    );
    assert_eq!(summary.skipped_entries, 1);
    assert_eq!(summary.notices, 1);
}

#[test]
fn browser_is_lazy_sorted_stable_and_cancellable() {
    let directory = tempfile::tempdir().expect("temp directory");
    fs::create_dir(directory.path().join("Zulu")).expect("zulu");
    fs::create_dir(directory.path().join("alpha")).expect("alpha");
    fs::write(directory.path().join("aardvark.txt"), "file").expect("file");
    fs::write(directory.path().join("Zulu/hidden-child"), "not loaded").expect("nested");
    let cancellation = CancellationToken::default();

    let first = FileSystemBrowser::direct_children(directory.path(), &cancellation)
        .expect("direct children");
    let second = FileSystemBrowser::direct_children(directory.path(), &cancellation)
        .expect("direct children");

    assert_eq!(
        first
            .iter()
            .map(|node| node.name.as_str())
            .collect::<Vec<_>>(),
        vec!["alpha", "Zulu", "aardvark.txt"]
    );
    assert_eq!(
        first.iter().map(|node| &node.id).collect::<Vec<_>>(),
        second.iter().map(|node| &node.id).collect::<Vec<_>>()
    );
    assert!(!first.iter().any(|node| node.name == "hidden-child"));

    cancellation.cancel();
    assert!(FileSystemBrowser::direct_children(directory.path(), &cancellation).is_err());
}

#[test]
fn browser_locations_include_existing_home_shortcuts() {
    let home = tempfile::tempdir().expect("home");
    fs::create_dir(home.path().join("Documents")).expect("documents");
    fs::create_dir(home.path().join("Downloads")).expect("downloads");

    let roots = FileSystemBrowser::roots(Some(home.path()));

    assert_eq!(roots[0].name, "Home");
    assert_eq!(roots[1].name, "Documents");
    assert_eq!(roots[2].name, "Downloads");
    #[cfg(unix)]
    assert!(roots.iter().any(|root| root.path == Path::new("/")));
    #[cfg(windows)]
    assert!(
        roots
            .iter()
            .any(|root| root.kind == foldry_core::BrowserRootKind::Drive)
    );
}

#[test]
fn browser_size_counts_regular_file_logical_bytes_and_can_be_cancelled() {
    let directory = tempfile::tempdir().expect("directory");
    fs::create_dir(directory.path().join("nested")).expect("nested");
    fs::write(directory.path().join("one.bin"), [0_u8; 7]).expect("first file");
    fs::write(directory.path().join("nested/two.bin"), [0_u8; 11]).expect("second file");

    let result = FileSystemBrowser::directory_size(directory.path(), &CancellationToken::default())
        .expect("size");
    assert_eq!(result.logical_bytes, 18);
    assert!(!result.partial);

    let cancelled = CancellationToken::default();
    cancelled.cancel();
    assert!(FileSystemBrowser::directory_size(directory.path(), &cancelled).is_err());
}

#[cfg(unix)]
#[test]
fn browser_size_does_not_follow_symlink_loops() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().expect("directory");
    fs::write(directory.path().join("data.bin"), [0_u8; 13]).expect("file");
    symlink(directory.path(), directory.path().join("loop")).expect("loop");

    let result = FileSystemBrowser::directory_size(directory.path(), &CancellationToken::default())
        .expect("size");

    assert_eq!(result.logical_bytes, 13);
    assert!(!result.partial);
}

#[cfg(unix)]
#[test]
fn browser_size_marks_unreadable_subtrees_as_partial() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir().expect("directory");
    let unreadable = directory.path().join("unreadable");
    fs::create_dir(&unreadable).expect("unreadable directory");
    fs::write(unreadable.join("secret.bin"), [0_u8; 17]).expect("file");
    fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o000))
        .expect("remove permissions");

    let result = FileSystemBrowser::directory_size(directory.path(), &CancellationToken::default())
        .expect("size");

    fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o700))
        .expect("restore permissions");
    assert!(result.partial);
    assert_eq!(result.warnings, 1);
}

#[test]
fn cancelled_scan_writes_nothing() {
    let directory = tempfile::tempdir().expect("temp directory");
    fs::write(directory.path().join("file"), "data").expect("fixture");
    let cancellation = CancellationToken::default();
    cancellation.cancel();
    let mut sink = CollectingSink::default();

    let error = FileSystemScanner::scan(directory.path(), &matcher(), &mut sink, &cancellation)
        .expect_err("cancelled");

    assert_eq!(error.to_string(), "scan was cancelled");
    assert!(sink.entries.is_empty());
}

#[cfg(target_os = "linux")]
#[test]
fn source_filesystem_case_behavior_is_probed() {
    use foldry_core::detect_case_sensitivity;

    let directory = tempfile::Builder::new()
        .prefix("foldry-case-probe-")
        .tempdir()
        .expect("temp directory");

    let detected = detect_case_sensitivity(directory.path()).expect("case detection");

    assert_eq!(detected.value, FileSystemCaseSensitivity::Sensitive);
}
