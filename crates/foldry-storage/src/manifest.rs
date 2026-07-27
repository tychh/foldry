use std::{
    fs::{self, File, OpenOptions},
    io::{self, BufRead, BufReader, BufWriter, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use foldry_application::{
    CancellationToken, CompiledProfile, ExecutionEntrySource, FileSystemScanner, PreviewFilter,
    ScanDisposition, ScanError, ScanNotice, ScanSink, ScanSinkError, ScanSummary, ScannedEntry,
};
use serde::{Deserialize, Serialize};
use tempfile::TempDir;
use thiserror::Error;

const MANIFEST_EXTENSION: &str = "foldry-manifest";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ManifestRecord {
    Entry { entry: ScannedEntry },
    Notice { notice: ScanNotice },
}

/// Opaque byte offset into the private newline-delimited manifest.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ManifestCursor(u64);

impl ManifestCursor {
    #[must_use]
    pub fn token(self) -> String {
        self.0.to_string()
    }

    pub fn from_token(token: &str) -> Result<Self, ManifestError> {
        token
            .parse()
            .map(Self)
            .map_err(|_| ManifestError::InvalidCursor)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManifestPage {
    pub entries: Vec<ScannedEntry>,
    pub next_cursor: Option<ManifestCursor>,
}

#[derive(Debug, Error)]
pub enum ManifestError {
    #[error("cannot create manifest directory {path}: {source}")]
    CreateDirectory { path: PathBuf, source: io::Error },
    #[error("cannot create manifest {path}: {source}")]
    Create { path: PathBuf, source: io::Error },
    #[error("manifest ID must contain only ASCII letters, digits, '-' or '_'")]
    InvalidId,
    #[error("cannot access manifest {path}: {source}")]
    Io { path: PathBuf, source: io::Error },
    #[error("manifest {path} contains an invalid record: {message}")]
    Decode { path: PathBuf, message: String },
    #[error("manifest page size must be between 1 and 1000")]
    InvalidPageSize,
    #[error("manifest cursor is invalid")]
    InvalidCursor,
    #[error("manifest page request was cancelled")]
    Cancelled,
}

/// Owns an unfinished file and removes it unless `finish` succeeds.
pub struct ManifestWriter {
    path: PathBuf,
    writer: Option<BufWriter<File>>,
}

impl ManifestWriter {
    pub fn create(directory: &Path, id: &str) -> Result<Self, ManifestError> {
        if id.is_empty()
            || !id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return Err(ManifestError::InvalidId);
        }
        fs::create_dir_all(directory).map_err(|source| ManifestError::CreateDirectory {
            path: directory.to_path_buf(),
            source,
        })?;
        let path = directory.join(format!("{id}.{MANIFEST_EXTENSION}"));
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|source| ManifestError::Create {
                path: path.clone(),
                source,
            })?;
        Ok(Self {
            path,
            writer: Some(BufWriter::new(file)),
        })
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn finish(mut self) -> Result<ManifestHandle, ManifestError> {
        let mut writer = self.writer.take().expect("unfinished writer exists");
        writer.flush().map_err(|source| ManifestError::Io {
            path: self.path.clone(),
            source,
        })?;
        writer
            .get_ref()
            .sync_all()
            .map_err(|source| ManifestError::Io {
                path: self.path.clone(),
                source,
            })?;
        Ok(ManifestHandle {
            path: self.path.clone(),
        })
    }

    fn write_record(&mut self, record: &ManifestRecord) -> Result<(), ScanSinkError> {
        let writer = self
            .writer
            .as_mut()
            .ok_or_else(|| ScanSinkError("manifest writer is already finished".to_owned()))?;
        serde_json::to_writer(&mut *writer, record)
            .map_err(|error| ScanSinkError(error.to_string()))?;
        writer
            .write_all(b"\n")
            .map_err(|error| ScanSinkError(error.to_string()))
    }
}

impl ScanSink for ManifestWriter {
    fn write_entry(&mut self, entry: &ScannedEntry) -> Result<(), ScanSinkError> {
        self.write_record(&ManifestRecord::Entry {
            entry: entry.clone(),
        })
    }

    fn write_notice(&mut self, notice: &ScanNotice) -> Result<(), ScanSinkError> {
        self.write_record(&ManifestRecord::Notice {
            notice: notice.clone(),
        })
    }
}

impl Drop for ManifestWriter {
    fn drop(&mut self) {
        if self.writer.is_some() {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManifestHandle {
    path: PathBuf,
}

pub struct ManifestEntryReader {
    path: PathBuf,
    reader: BufReader<File>,
    line: String,
}

impl ManifestEntryReader {
    pub fn open(handle: &ManifestHandle) -> Result<Self, ManifestError> {
        let file = File::open(&handle.path).map_err(|source| ManifestError::Io {
            path: handle.path.clone(),
            source,
        })?;
        Ok(Self {
            path: handle.path.clone(),
            reader: BufReader::new(file),
            line: String::new(),
        })
    }
}

impl ExecutionEntrySource for ManifestEntryReader {
    fn next_entry(&mut self) -> Result<Option<ScannedEntry>, String> {
        loop {
            self.line.clear();
            let read = self
                .reader
                .read_line(&mut self.line)
                .map_err(|error| format!("{}: {error}", self.path.display()))?;
            if read == 0 {
                return Ok(None);
            }
            let record: ManifestRecord = serde_json::from_str(&self.line)
                .map_err(|error| format!("{}: {error}", self.path.display()))?;
            if let ManifestRecord::Entry { entry } = record {
                return Ok(Some(entry));
            }
        }
    }
}

impl ManifestHandle {
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn page(
        &self,
        cursor: ManifestCursor,
        page_size: usize,
        filter: PreviewFilter,
        cancellation: &CancellationToken,
    ) -> Result<ManifestPage, ManifestError> {
        if !(1..=1000).contains(&page_size) {
            return Err(ManifestError::InvalidPageSize);
        }
        let file = File::open(&self.path).map_err(|source| ManifestError::Io {
            path: self.path.clone(),
            source,
        })?;
        let file_length = file
            .metadata()
            .map_err(|source| ManifestError::Io {
                path: self.path.clone(),
                source,
            })?
            .len();
        let mut reader = BufReader::new(file);
        reader
            .seek(SeekFrom::Start(cursor.0))
            .map_err(|source| ManifestError::Io {
                path: self.path.clone(),
                source,
            })?;
        let mut entries = Vec::with_capacity(page_size);
        let mut line = String::new();

        while entries.len() < page_size {
            if cancellation.is_cancelled() {
                return Err(ManifestError::Cancelled);
            }
            line.clear();
            let bytes = reader
                .read_line(&mut line)
                .map_err(|source| ManifestError::Io {
                    path: self.path.clone(),
                    source,
                })?;
            if bytes == 0 {
                break;
            }
            let record: ManifestRecord =
                serde_json::from_str(&line).map_err(|error| ManifestError::Decode {
                    path: self.path.clone(),
                    message: error.to_string(),
                })?;
            if let ManifestRecord::Entry { entry } = record
                && filter_matches(filter, entry.disposition)
            {
                entries.push(entry);
            }
        }
        let position = reader
            .stream_position()
            .map_err(|source| ManifestError::Io {
                path: self.path.clone(),
                source,
            })?;
        Ok(ManifestPage {
            entries,
            next_cursor: (position < file_length).then_some(ManifestCursor(position)),
        })
    }

    pub fn remove(self) -> Result<(), ManifestError> {
        fs::remove_file(&self.path).map_err(|source| ManifestError::Io {
            path: self.path,
            source,
        })
    }
}

fn filter_matches(filter: PreviewFilter, disposition: ScanDisposition) -> bool {
    match filter {
        PreviewFilter::All => true,
        PreviewFilter::Included => disposition == ScanDisposition::Included,
        PreviewFilter::Excluded => disposition == ScanDisposition::Excluded,
        PreviewFilter::Skipped => disposition == ScanDisposition::Skipped,
    }
}

pub fn temporary_manifest_directory() -> Result<TempDir, ManifestError> {
    tempfile::Builder::new()
        .prefix("foldry-manifests-")
        .tempdir()
        .map_err(|source| ManifestError::CreateDirectory {
            path: std::env::temp_dir(),
            source,
        })
}

#[derive(Debug, Error)]
pub enum ScanManifestError {
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    #[error(transparent)]
    Scan(#[from] ScanError),
}

/// Runs a fresh scan into a uniquely owned manifest; failures leave no temp file.
pub fn scan_to_manifest(
    directory: &Path,
    id: &str,
    source: &Path,
    matcher: &CompiledProfile,
    cancellation: &CancellationToken,
) -> Result<(ManifestHandle, ScanSummary), ScanManifestError> {
    let mut writer = ManifestWriter::create(directory, id)?;
    let summary = FileSystemScanner::scan(source, matcher, &mut writer, cancellation)?;
    let handle = writer.finish()?;
    Ok((handle, summary))
}

#[cfg(test)]
mod tests {
    use std::{
        io::{self, BufWriter, Write},
        path::PathBuf,
    };

    use foldry_application::{FileSystemObjectKind, ScanDisposition, ScannedEntry};

    use super::ManifestRecord;

    #[test]
    fn one_million_synthetic_entries_use_a_fixed_serialization_buffer() {
        const BUFFER_SIZE: usize = 16 * 1024;
        let entry = ScannedEntry {
            relative_path: "synthetic/file.txt".to_owned(),
            native_path: PathBuf::from("synthetic/file.txt"),
            kind: FileSystemObjectKind::RegularFile,
            disposition: ScanDisposition::Included,
            size: 1,
            modified_unix_nanos: Some(1),
            link_target: None,
            is_mount_point: false,
            is_network_mount: false,
            reason: None,
        };
        let record = ManifestRecord::Entry { entry };
        let mut writer = BufWriter::with_capacity(BUFFER_SIZE, io::sink());

        for _ in 0..1_000_000 {
            serde_json::to_writer(&mut writer, &record).expect("serialize record");
            writer.write_all(b"\n").expect("record delimiter");
        }

        assert_eq!(writer.capacity(), BUFFER_SIZE);
    }
}
