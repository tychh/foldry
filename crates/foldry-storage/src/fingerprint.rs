use std::{
    fs::File,
    io::{BufRead, BufReader, BufWriter, Write},
};

use foldry_application::{
    CancellationToken, ExecutionControl, ExecutionEntrySource, FileSystemObjectKind,
    ScanDisposition, ScannedEntry, SourceFingerprintSummary,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{ManifestEntryReader, ManifestHandle};

pub const SOURCE_FINGERPRINT_ALGORITHM_VERSION: u16 = 1;
const SORT_CHUNK_ENTRIES: usize = 4_096;
const DOMAIN: &[u8] = b"foldry.source-fingerprint\0";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceFingerprint {
    pub algorithm_version: u16,
    pub digest: String,
    pub summary: SourceFingerprintSummary,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct CanonicalRecord {
    path: String,
    kind: u8,
    size: Option<u64>,
    modified_unix_nanos: Option<u64>,
    link_target: Option<String>,
}

pub fn fingerprint_manifest(
    manifest: &ManifestHandle,
    control: &ExecutionControl,
) -> Result<SourceFingerprint, String> {
    fingerprint_manifest_cancellable(manifest, control, None)
}

pub fn fingerprint_manifest_cancellable(
    manifest: &ManifestHandle,
    control: &ExecutionControl,
    cancellation: Option<&CancellationToken>,
) -> Result<SourceFingerprint, String> {
    let mut entries = ManifestEntryReader::open(manifest).map_err(|error| error.to_string())?;
    let sort_directory = manifest
        .path()
        .parent()
        .ok_or_else(|| "manifest has no parent directory".to_owned())?;
    let mut pending = Vec::with_capacity(SORT_CHUNK_ENTRIES);
    let mut chunks = Vec::new();
    let mut summary = SourceFingerprintSummary {
        included_entries: 0,
        included_bytes: 0,
    };
    while let Some(entry) = entries.next_entry()? {
        if !control.checkpoint() || cancellation.is_some_and(CancellationToken::is_cancelled) {
            return Err("fingerprint cancelled".into());
        }
        if entry.disposition != ScanDisposition::Included {
            continue;
        }
        summary.included_entries = summary.included_entries.saturating_add(1);
        if entry.kind == FileSystemObjectKind::RegularFile {
            summary.included_bytes = summary.included_bytes.saturating_add(entry.size);
        }
        pending.push(canonical_record(entry)?);
        if pending.len() == SORT_CHUNK_ENTRIES {
            chunks.push(write_chunk(sort_directory, &mut pending)?);
        }
    }
    if !pending.is_empty() {
        chunks.push(write_chunk(sort_directory, &mut pending)?);
    }

    let mut readers = chunks
        .iter()
        .map(|chunk| {
            chunk
                .reopen()
                .map(BufReader::new)
                .map_err(|error| error.to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut heads = readers
        .iter_mut()
        .map(read_record)
        .collect::<Result<Vec<_>, _>>()?;
    let mut hasher = Sha256::new();
    hasher.update(DOMAIN);
    hasher.update(SOURCE_FINGERPRINT_ALGORITHM_VERSION.to_be_bytes());
    let mut previous_path: Option<String> = None;
    loop {
        if !control.checkpoint() || cancellation.is_some_and(CancellationToken::is_cancelled) {
            return Err("fingerprint cancelled".into());
        }
        let Some(index) = heads
            .iter()
            .enumerate()
            .filter_map(|(index, record)| record.as_ref().map(|record| (index, record)))
            .min_by(|(_, left), (_, right)| {
                left.path.cmp(&right.path).then(left.kind.cmp(&right.kind))
            })
            .map(|(index, _)| index)
        else {
            break;
        };
        let record = heads[index].take().expect("selected record exists");
        if previous_path.as_deref() == Some(record.path.as_str()) {
            return Err(format!(
                "duplicate normalized manifest path: {}",
                record.path
            ));
        }
        hash_record(&mut hasher, &record);
        previous_path = Some(record.path.clone());
        heads[index] = read_record(&mut readers[index])?;
    }
    Ok(SourceFingerprint {
        algorithm_version: SOURCE_FINGERPRINT_ALGORITHM_VERSION,
        digest: format!("sha256:{:x}", hasher.finalize()),
        summary,
    })
}

fn canonical_record(entry: ScannedEntry) -> Result<CanonicalRecord, String> {
    let kind = match entry.kind {
        FileSystemObjectKind::Directory => 1,
        FileSystemObjectKind::RegularFile => 2,
        FileSystemObjectKind::Symlink => 3,
        other => return Err(format!("unsupported included object kind: {other:?}")),
    };
    Ok(CanonicalRecord {
        path: entry.relative_path,
        kind,
        size: (entry.kind == FileSystemObjectKind::RegularFile).then_some(entry.size),
        modified_unix_nanos: (entry.kind != FileSystemObjectKind::Directory)
            .then_some(entry.modified_unix_nanos)
            .flatten(),
        link_target: (entry.kind == FileSystemObjectKind::Symlink)
            .then(|| {
                entry
                    .link_target
                    .map(|target| target.to_string_lossy().into_owned())
            })
            .flatten(),
    })
}

fn write_chunk(
    directory: &std::path::Path,
    records: &mut Vec<CanonicalRecord>,
) -> Result<tempfile::NamedTempFile, String> {
    records.sort_by(|left, right| left.path.cmp(&right.path).then(left.kind.cmp(&right.kind)));
    let mut chunk =
        tempfile::NamedTempFile::new_in(directory).map_err(|error| error.to_string())?;
    {
        let mut writer = BufWriter::new(chunk.as_file_mut());
        for record in records.drain(..) {
            serde_json::to_writer(&mut writer, &record).map_err(|error| error.to_string())?;
            writer.write_all(b"\n").map_err(|error| error.to_string())?;
        }
        writer.flush().map_err(|error| error.to_string())?;
    }
    Ok(chunk)
}

fn read_record(reader: &mut BufReader<File>) -> Result<Option<CanonicalRecord>, String> {
    let mut line = String::new();
    if reader
        .read_line(&mut line)
        .map_err(|error| error.to_string())?
        == 0
    {
        return Ok(None);
    }
    serde_json::from_str(&line)
        .map(Some)
        .map_err(|error| error.to_string())
}

fn hash_record(hasher: &mut Sha256, record: &CanonicalRecord) {
    hash_bytes(hasher, record.path.as_bytes());
    hasher.update([record.kind]);
    hash_optional_u64(hasher, record.size);
    hash_optional_u64(hasher, record.modified_unix_nanos);
    match &record.link_target {
        Some(target) => {
            hasher.update([1]);
            hash_bytes(hasher, target.as_bytes());
        }
        None => hasher.update([0]),
    }
}

fn hash_bytes(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update(u64::try_from(bytes.len()).unwrap_or(u64::MAX).to_be_bytes());
    hasher.update(bytes);
}

fn hash_optional_u64(hasher: &mut Sha256, value: Option<u64>) {
    match value {
        Some(value) => {
            hasher.update([1]);
            hasher.update(value.to_be_bytes());
        }
        None => hasher.update([0]),
    }
}
