use std::{
    fs::File,
    io::{Cursor, Read},
    path::{Path, PathBuf},
};

use foldry_core::{
    ArchiveFormat, CompressionLevel, FileSystemObjectKind, ScanDisposition, ScannedEntry,
    codec_level, create_archive_writer,
};

fn entry(path: &str, kind: FileSystemObjectKind, size: u64) -> ScannedEntry {
    ScannedEntry {
        relative_path: path.to_owned(),
        native_path: PathBuf::from(path),
        kind,
        disposition: ScanDisposition::Included,
        size,
        modified_unix_nanos: Some(1_700_000_000_000_000_000),
        link_target: (kind == FileSystemObjectKind::Symlink).then(|| PathBuf::from("file.txt")),
        is_mount_point: false,
        is_network_mount: false,
        reason: None,
    }
}

fn write_fixture(format: ArchiveFormat, path: &Path) {
    let file = File::create(path).expect("archive file");
    let mut writer =
        create_archive_writer(format, CompressionLevel::Balanced, file).expect("writer");
    writer
        .add_directory(
            "root/empty",
            &entry("empty", FileSystemObjectKind::Directory, 0),
        )
        .expect("directory");
    writer
        .add_file(
            "root/file.txt",
            &entry("file.txt", FileSystemObjectKind::RegularFile, 7),
            &mut Cursor::new(b"content"),
        )
        .expect("file");
    writer
        .add_symlink(
            "root/link",
            &entry("link", FileSystemObjectKind::Symlink, 0),
        )
        .expect("symlink");
    let file = writer.finish().expect("finish");
    file.sync_all().expect("sync");
}

#[test]
fn semantic_levels_have_the_version_one_codec_mapping() {
    assert_eq!(codec_level(ArchiveFormat::Zip, CompressionLevel::Fast), 1);
    assert_eq!(
        codec_level(ArchiveFormat::TarGz, CompressionLevel::Balanced),
        6
    );
    assert_eq!(
        codec_level(ArchiveFormat::TarZst, CompressionLevel::Maximum),
        19
    );
}

#[test]
fn zip_is_readable_by_an_independent_reader() {
    let directory = tempfile::tempdir().expect("directory");
    let path = directory.path().join("fixture.zip");
    write_fixture(ArchiveFormat::Zip, &path);

    let mut archive = zip::ZipArchive::new(File::open(path).expect("open")).expect("ZIP reader");
    assert!(archive.by_name("root/empty/").expect("directory").is_dir());
    let mut contents = String::new();
    archive
        .by_name("root/file.txt")
        .expect("file")
        .read_to_string(&mut contents)
        .expect("read");
    assert_eq!(contents, "content");
    assert!(archive.by_name("root/link").expect("link").is_symlink());
}

#[test]
fn tar_gz_is_readable_by_an_independent_reader() {
    let directory = tempfile::tempdir().expect("directory");
    let path = directory.path().join("fixture.tar.gz");
    write_fixture(ArchiveFormat::TarGz, &path);
    let decoder = flate2::read::GzDecoder::new(File::open(path).expect("open"));

    assert_tar_fixture(tar::Archive::new(decoder));
}

#[test]
fn tar_zst_is_readable_by_an_independent_reader() {
    let directory = tempfile::tempdir().expect("directory");
    let path = directory.path().join("fixture.tar.zst");
    write_fixture(ArchiveFormat::TarZst, &path);
    let decoder = zstd::stream::read::Decoder::new(File::open(path).expect("open"))
        .expect("Zstandard reader");

    assert_tar_fixture(tar::Archive::new(decoder));
}

fn assert_tar_fixture<R: Read>(mut archive: tar::Archive<R>) {
    let mut saw_directory = false;
    let mut saw_file = false;
    let mut saw_symlink = false;
    for entry in archive.entries().expect("entries") {
        let mut entry = entry.expect("entry");
        let path = entry.path().expect("path").into_owned();
        match path.to_string_lossy().as_ref() {
            "root/empty" | "root/empty/" => {
                saw_directory = entry.header().entry_type().is_dir();
            }
            "root/file.txt" => {
                let mut contents = String::new();
                entry.read_to_string(&mut contents).expect("file contents");
                saw_file = contents == "content";
            }
            "root/link" => {
                saw_symlink = entry.header().entry_type().is_symlink()
                    && entry
                        .link_name()
                        .expect("link name")
                        .as_deref()
                        .is_some_and(|target| target == Path::new("file.txt"));
            }
            _ => {}
        }
    }
    assert!(saw_directory);
    assert!(saw_file);
    assert!(saw_symlink);
}
