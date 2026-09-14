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
        created_unix_nanos: None,
        unix_mode: None,
        windows_attributes: None,
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
    assert_eq!(
        codec_level(ArchiveFormat::SevenZip, CompressionLevel::Balanced),
        6
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
fn zip_preserves_supported_creation_and_modification_times() {
    let directory = tempfile::tempdir().expect("directory");
    let path = directory.path().join("timestamps.zip");
    let mut timestamped = entry("file.txt", FileSystemObjectKind::RegularFile, 7);
    timestamped.created_unix_nanos = Some(1_600_000_000_000_000_000);
    let mut writer = create_archive_writer(
        ArchiveFormat::Zip,
        CompressionLevel::Balanced,
        File::create(&path).expect("archive file"),
    )
    .expect("writer");
    writer
        .add_file("file.txt", &timestamped, &mut Cursor::new(b"content"))
        .expect("file");
    writer.finish().expect("finish");

    let mut archive = zip::ZipArchive::new(File::open(path).expect("open")).expect("ZIP reader");
    let archived = archive.by_name("file.txt").expect("file");
    let timestamps = archived.extra_data_fields().find_map(|field| match field {
        zip::extra_fields::ExtraField::ExtendedTimestamp(value) => Some(value),
        _ => None,
    });
    let timestamps = timestamps.expect("extended timestamp field");
    assert_eq!(timestamps.mod_time(), Some(1_700_000_000));
    assert_eq!(timestamps.cr_time(), Some(1_600_000_000));
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

#[test]
fn seven_zip_is_readable_by_an_independent_reader() {
    let directory = tempfile::tempdir().expect("directory");
    let path = directory.path().join("fixture.7z");
    write_fixture(ArchiveFormat::SevenZip, &path);
    let mut archive = sevenz_rust2::ArchiveReader::open(path, sevenz_rust2::Password::empty())
        .expect("7z reader");
    let names = archive
        .archive()
        .files
        .iter()
        .map(|entry| entry.name.as_str())
        .collect::<Vec<_>>();
    assert!(names.contains(&"root/empty"));
    assert!(names.contains(&"root/file.txt"));
    assert!(names.contains(&"root/link"));
    let mut file_contents = None;
    archive
        .for_each_entries(
            &mut |entry: &sevenz_rust2::ArchiveEntry, reader: &mut dyn Read| {
                let mut contents = Vec::new();
                reader.read_to_end(&mut contents)?;
                if entry.name == "root/file.txt" {
                    file_contents = Some(contents);
                }
                Ok(true)
            },
        )
        .expect("read entries");
    assert_eq!(file_contents.as_deref(), Some(b"content".as_slice()));
}

#[test]
#[cfg(unix)]
fn supported_timestamps_and_unix_modes_are_written() {
    let directory = tempfile::tempdir().expect("directory");
    let mut metadata_entry = entry("file.txt", FileSystemObjectKind::RegularFile, 7);
    metadata_entry.unix_mode = Some(0o100640);

    let zip_path = directory.path().join("metadata.zip");
    let mut zip_writer = create_archive_writer(
        ArchiveFormat::Zip,
        CompressionLevel::Balanced,
        File::create(&zip_path).unwrap(),
    )
    .unwrap();
    zip_writer
        .add_file("file.txt", &metadata_entry, &mut Cursor::new(b"content"))
        .unwrap();
    zip_writer.finish().unwrap();
    let mut zip = zip::ZipArchive::new(File::open(zip_path).unwrap()).unwrap();
    assert_eq!(zip.by_name("file.txt").unwrap().unix_mode(), Some(0o100640));

    let tar_path = directory.path().join("metadata.tar.gz");
    let mut tar_writer = create_archive_writer(
        ArchiveFormat::TarGz,
        CompressionLevel::Balanced,
        File::create(&tar_path).unwrap(),
    )
    .unwrap();
    tar_writer
        .add_file("file.txt", &metadata_entry, &mut Cursor::new(b"content"))
        .unwrap();
    tar_writer.finish().unwrap();
    let decoder = flate2::read::GzDecoder::new(File::open(tar_path).unwrap());
    let mut tar = tar::Archive::new(decoder);
    let header = tar.entries().unwrap().next().unwrap().unwrap();
    assert_eq!(header.header().mode().unwrap(), 0o640);
    assert_eq!(header.header().mtime().unwrap(), 1_700_000_000);

    let seven_path = directory.path().join("metadata.7z");
    let mut seven_writer = create_archive_writer(
        ArchiveFormat::SevenZip,
        CompressionLevel::Balanced,
        File::create(&seven_path).unwrap(),
    )
    .unwrap();
    seven_writer
        .add_file("file.txt", &metadata_entry, &mut Cursor::new(b"content"))
        .unwrap();
    seven_writer.finish().unwrap();
    let seven =
        sevenz_rust2::ArchiveReader::open(seven_path, sevenz_rust2::Password::empty()).unwrap();
    let archived = &seven.archive().files[0];
    assert!(archived.has_last_modified_date);
    assert!(archived.has_windows_attributes);
    assert_eq!(archived.windows_attributes >> 16, 0o100640);
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
