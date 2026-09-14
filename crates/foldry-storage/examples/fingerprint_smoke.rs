use std::{path::PathBuf, time::Instant};

use foldry_application::{
    ExecutionControl, FileSystemObjectKind, ScanDisposition, ScanSink, ScannedEntry,
};
use foldry_storage::{ManifestWriter, fingerprint_manifest};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    const ENTRIES: u64 = 100_000;
    let directory = tempfile::tempdir()?;
    let mut writer = ManifestWriter::create(directory.path(), "fingerprint-smoke")?;
    for index in (0..ENTRIES).rev() {
        writer.write_entry(&ScannedEntry {
            relative_path: format!("tree/{index:08}/file.bin"),
            native_path: PathBuf::from(format!("tree/{index:08}/file.bin")),
            kind: FileSystemObjectKind::RegularFile,
            disposition: ScanDisposition::Included,
            size: index % 65_536,
            modified_unix_nanos: Some(index * 1_000_000),
            created_unix_nanos: None,
            unix_mode: Some(0o100_644),
            windows_attributes: None,
            link_target: None,
            is_mount_point: false,
            is_network_mount: false,
            reason: None,
        })?;
    }
    let manifest = writer.finish()?;
    let started = Instant::now();
    let fingerprint = fingerprint_manifest(&manifest, &ExecutionControl::default())?;
    let elapsed = started.elapsed();
    println!(
        "fingerprint: {ENTRIES} entries in {} ms ({:.0} entries/s), {}",
        elapsed.as_millis(),
        ENTRIES as f64 / elapsed.as_secs_f64(),
        fingerprint.digest
    );
    Ok(())
}
