use std::{
    collections::HashSet,
    ffi::OsStr,
    fmt, fs, io,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::UNIX_EPOCH,
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    CompiledProfile, FileSystemCaseSensitivity, MatchDecision, MatchPathError, MatchReason,
};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CaseSensitivityConfidence {
    Probed,
    PlatformAssumption,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DetectedCaseSensitivity {
    pub value: FileSystemCaseSensitivity,
    pub confidence: CaseSensitivityConfidence,
}

/// Kind of filesystem object observed without following links.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileSystemObjectKind {
    Directory,
    RegularFile,
    Symlink,
    JunctionOrReparsePoint,
    SpecialFile,
    Unreadable,
}

/// Why an entry was retained, filtered, or skipped by the scanner.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanDisposition {
    Included,
    Excluded,
    Skipped,
}

/// Stable machine-readable scanner warning.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanNoticeCode {
    SpecialFile,
    UnreadableEntry,
    EntryDisappeared,
    DirectoryCycle,
    InvalidRelativePath,
}

/// One non-fatal observation made while scanning.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ScanNotice {
    pub code: ScanNoticeCode,
    pub relative_path: String,
    pub message: String,
}

/// Metadata captured for later preview and immutable planning.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ScannedEntry {
    pub relative_path: String,
    pub native_path: PathBuf,
    pub kind: FileSystemObjectKind,
    pub disposition: ScanDisposition,
    pub size: u64,
    pub modified_unix_nanos: Option<u64>,
    pub link_target: Option<PathBuf>,
    pub is_mount_point: bool,
    pub is_network_mount: bool,
    pub reason: Option<MatchReason>,
}

/// Totals kept in memory while individual entries are streamed to a sink.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ScanSummary {
    pub visited_entries: u64,
    pub included_entries: u64,
    pub excluded_entries: u64,
    pub skipped_entries: u64,
    pub included_files: u64,
    pub included_directories: u64,
    pub included_links: u64,
    pub included_bytes: u64,
    pub notices: u64,
}

/// Cooperative cancellation shared by browser, scan, and preview requests.
#[derive(Clone, Debug, Default)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    fn check(&self) -> Result<(), ScanError> {
        if self.is_cancelled() {
            Err(ScanError::Cancelled)
        } else {
            Ok(())
        }
    }
}

/// Streaming destination. Implementations must not retain every entry in memory.
pub trait ScanSink {
    fn write_entry(&mut self, entry: &ScannedEntry) -> Result<(), ScanSinkError>;
    fn write_notice(&mut self, notice: &ScanNotice) -> Result<(), ScanSinkError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScanSinkError(pub String);

impl fmt::Display for ScanSinkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for ScanSinkError {}

#[derive(Debug)]
pub enum ScanError {
    Cancelled,
    InvalidSource(PathBuf),
    ReadSource { path: PathBuf, source: io::Error },
    Match(MatchPathError),
    Sink(ScanSinkError),
}

impl fmt::Display for ScanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("scan was cancelled"),
            Self::InvalidSource(path) => {
                write!(
                    formatter,
                    "scan source is not a directory: {}",
                    path.display()
                )
            }
            Self::ReadSource { path, source } => {
                write!(
                    formatter,
                    "cannot read scan source {}: {source}",
                    path.display()
                )
            }
            Self::Match(error) => write!(formatter, "cannot match scanned path: {error}"),
            Self::Sink(error) => write!(formatter, "cannot write scan manifest: {error}"),
        }
    }
}

impl std::error::Error for ScanError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ReadSource { source, .. } => Some(source),
            Self::Match(error) => Some(error),
            Self::Sink(error) => Some(error),
            Self::Cancelled | Self::InvalidSource(_) => None,
        }
    }
}

impl From<MatchPathError> for ScanError {
    fn from(error: MatchPathError) -> Self {
        Self::Match(error)
    }
}

impl From<ScanSinkError> for ScanError {
    fn from(error: ScanSinkError) -> Self {
        Self::Sink(error)
    }
}

struct DirectoryFrame {
    relative_path: PathBuf,
    entries: fs::ReadDir,
    identity: Option<FileIdentity>,
}

/// Synchronous, dependency-free scanner intended to run on a worker thread.
pub struct FileSystemScanner;

impl FileSystemScanner {
    /// Walks `source` without following links and streams every result to `sink`.
    pub fn scan(
        source: &Path,
        matcher: &CompiledProfile,
        sink: &mut dyn ScanSink,
        cancellation: &CancellationToken,
    ) -> Result<ScanSummary, ScanError> {
        cancellation.check()?;
        let source_metadata =
            fs::symlink_metadata(source).map_err(|source_error| ScanError::ReadSource {
                path: source.to_path_buf(),
                source: source_error,
            })?;
        if !source_metadata.is_dir() || source_metadata.file_type().is_symlink() {
            return Err(ScanError::InvalidSource(source.to_path_buf()));
        }

        let root_entries = fs::read_dir(source).map_err(|source_error| ScanError::ReadSource {
            path: source.to_path_buf(),
            source: source_error,
        })?;
        let root_identity = file_identity(&source_metadata);
        let mut ancestors = HashSet::new();
        if let Some(identity) = root_identity {
            ancestors.insert(identity);
        }
        let mut frames = vec![DirectoryFrame {
            relative_path: PathBuf::new(),
            entries: root_entries,
            identity: root_identity,
        }];
        let mounts = MountTable::load();
        let mut summary = ScanSummary::default();

        while !frames.is_empty() {
            cancellation.check()?;
            let next = {
                let frame = frames.last_mut().expect("frame exists");
                frame
                    .entries
                    .next()
                    .map(|entry| (frame.relative_path.clone(), entry))
            };

            let Some((parent_relative, entry_result)) = next else {
                let frame = frames.pop().expect("frame exists");
                if let Some(identity) = frame.identity {
                    ancestors.remove(&identity);
                }
                continue;
            };

            let directory_entry = match entry_result {
                Ok(entry) => entry,
                Err(error) => {
                    write_notice(
                        sink,
                        &mut summary,
                        ScanNoticeCode::UnreadableEntry,
                        path_for_display(&parent_relative),
                        format!("cannot read directory entry: {error}"),
                    )?;
                    continue;
                }
            };
            let relative_path = parent_relative.join(directory_entry.file_name());
            let relative_match_path = match relative_path_to_match_path(&relative_path) {
                Ok(path) => path,
                Err(error) => {
                    write_notice(
                        sink,
                        &mut summary,
                        ScanNoticeCode::InvalidRelativePath,
                        path_for_display(&relative_path),
                        error.to_string(),
                    )?;
                    continue;
                }
            };
            let native_path = source.join(&relative_path);
            let metadata = match fs::symlink_metadata(&native_path) {
                Ok(metadata) => metadata,
                Err(error) => {
                    let code = if error.kind() == io::ErrorKind::NotFound {
                        ScanNoticeCode::EntryDisappeared
                    } else {
                        ScanNoticeCode::UnreadableEntry
                    };
                    let entry = ScannedEntry {
                        relative_path: relative_match_path.clone(),
                        native_path: native_path.clone(),
                        kind: FileSystemObjectKind::Unreadable,
                        disposition: ScanDisposition::Skipped,
                        size: 0,
                        modified_unix_nanos: None,
                        link_target: None,
                        is_mount_point: false,
                        is_network_mount: false,
                        reason: None,
                    };
                    write_entry(sink, &mut summary, &entry)?;
                    write_notice(
                        sink,
                        &mut summary,
                        code,
                        relative_match_path,
                        format!("cannot read entry metadata: {error}"),
                    )?;
                    continue;
                }
            };

            let kind = classify_metadata(&native_path, &metadata);
            let is_directory = kind == FileSystemObjectKind::Directory;
            let matched = matcher.matched(&relative_match_path, is_directory)?;
            let mut disposition = match matched.decision {
                MatchDecision::Include => ScanDisposition::Included,
                MatchDecision::Exclude => ScanDisposition::Excluded,
            };
            if matches!(
                kind,
                FileSystemObjectKind::SpecialFile | FileSystemObjectKind::Unreadable
            ) {
                disposition = ScanDisposition::Skipped;
            }
            let is_mount_point = is_directory
                && (mounts.is_mount(&native_path) || is_mount(&native_path, source, &metadata));
            let mut entry = ScannedEntry {
                relative_path: relative_match_path.clone(),
                native_path: native_path.clone(),
                kind,
                disposition,
                size: if kind == FileSystemObjectKind::RegularFile {
                    metadata.len()
                } else {
                    0
                },
                modified_unix_nanos: modified_unix_nanos(&metadata),
                link_target: if matches!(
                    kind,
                    FileSystemObjectKind::Symlink | FileSystemObjectKind::JunctionOrReparsePoint
                ) {
                    fs::read_link(&native_path).ok()
                } else {
                    None
                },
                is_mount_point,
                is_network_mount: is_mount_point && mounts.is_network_mount(&native_path),
                reason: matched.reason,
            };
            let mut next_directory = None;
            let mut deferred_notice = None;
            if kind == FileSystemObjectKind::SpecialFile {
                deferred_notice = Some((
                    ScanNoticeCode::SpecialFile,
                    relative_match_path.clone(),
                    "special files are not archived".to_owned(),
                ));
            } else if is_directory && disposition == ScanDisposition::Included {
                let identity = file_identity(&metadata);
                if identity.is_some_and(|identity| ancestors.contains(&identity)) {
                    entry.disposition = ScanDisposition::Skipped;
                    deferred_notice = Some((
                        ScanNoticeCode::DirectoryCycle,
                        relative_match_path.clone(),
                        "directory identity is already present in the active traversal path"
                            .to_owned(),
                    ));
                } else {
                    match fs::read_dir(&native_path) {
                        Ok(entries) => {
                            next_directory = Some(DirectoryFrame {
                                relative_path,
                                entries,
                                identity,
                            });
                        }
                        Err(error) => {
                            entry.disposition = ScanDisposition::Skipped;
                            deferred_notice = Some((
                                ScanNoticeCode::UnreadableEntry,
                                relative_match_path.clone(),
                                format!("cannot read directory: {error}"),
                            ));
                        }
                    }
                }
            }
            write_entry(sink, &mut summary, &entry)?;
            if let Some((code, path, message)) = deferred_notice {
                write_notice(sink, &mut summary, code, path, message)?;
            }
            if let Some(frame) = next_directory {
                if let Some(identity) = frame.identity {
                    ancestors.insert(identity);
                }
                frames.push(frame);
            }
        }

        Ok(summary)
    }
}

/// Detects source-filesystem case behavior without creating probe files.
pub fn detect_case_sensitivity(source: &Path) -> io::Result<DetectedCaseSensitivity> {
    let candidate = case_probe_candidate(source)?;
    let Some(candidate) = candidate else {
        return Ok(assumed_case_sensitivity());
    };
    let Some(file_name) = candidate.file_name() else {
        return Ok(assumed_case_sensitivity());
    };
    let Some(alternate_name) = toggle_ascii_case(file_name) else {
        return Ok(assumed_case_sensitivity());
    };
    let alternate = candidate.with_file_name(alternate_name);
    let original_metadata = fs::symlink_metadata(&candidate)?;
    let alternate_metadata = match fs::symlink_metadata(alternate) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(DetectedCaseSensitivity {
                value: FileSystemCaseSensitivity::Sensitive,
                confidence: CaseSensitivityConfidence::Probed,
            });
        }
        Err(error) => return Err(error),
    };
    Ok(DetectedCaseSensitivity {
        value: if file_identity(&original_metadata) == file_identity(&alternate_metadata) {
            FileSystemCaseSensitivity::Insensitive
        } else {
            FileSystemCaseSensitivity::Sensitive
        },
        confidence: CaseSensitivityConfidence::Probed,
    })
}

fn case_probe_candidate(source: &Path) -> io::Result<Option<PathBuf>> {
    if source.file_name().and_then(toggle_ascii_case).is_some() {
        return Ok(Some(source.to_path_buf()));
    }
    if !source.is_dir() {
        return Ok(None);
    }
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        if toggle_ascii_case(&entry.file_name()).is_some() {
            return Ok(Some(entry.path()));
        }
    }
    Ok(None)
}

fn toggle_ascii_case(value: &OsStr) -> Option<std::ffi::OsString> {
    let text = value.to_str()?;
    let mut changed = false;
    let alternate = text
        .chars()
        .map(|character| {
            if !changed && character.is_ascii_alphabetic() {
                changed = true;
                if character.is_ascii_lowercase() {
                    character.to_ascii_uppercase()
                } else {
                    character.to_ascii_lowercase()
                }
            } else {
                character
            }
        })
        .collect::<String>();
    changed.then(|| alternate.into())
}

const fn assumed_case_sensitivity() -> DetectedCaseSensitivity {
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    let value = FileSystemCaseSensitivity::Insensitive;
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let value = FileSystemCaseSensitivity::Sensitive;

    DetectedCaseSensitivity {
        value,
        confidence: CaseSensitivityConfidence::PlatformAssumption,
    }
}

fn write_entry(
    sink: &mut dyn ScanSink,
    summary: &mut ScanSummary,
    entry: &ScannedEntry,
) -> Result<(), ScanSinkError> {
    summary.visited_entries += 1;
    match entry.disposition {
        ScanDisposition::Included => {
            summary.included_entries += 1;
            match entry.kind {
                FileSystemObjectKind::RegularFile => {
                    summary.included_files += 1;
                    summary.included_bytes = summary.included_bytes.saturating_add(entry.size);
                }
                FileSystemObjectKind::Directory => summary.included_directories += 1,
                FileSystemObjectKind::Symlink | FileSystemObjectKind::JunctionOrReparsePoint => {
                    summary.included_links += 1
                }
                FileSystemObjectKind::SpecialFile | FileSystemObjectKind::Unreadable => {}
            }
        }
        ScanDisposition::Excluded => summary.excluded_entries += 1,
        ScanDisposition::Skipped => summary.skipped_entries += 1,
    }
    sink.write_entry(entry)
}

fn write_notice(
    sink: &mut dyn ScanSink,
    summary: &mut ScanSummary,
    code: ScanNoticeCode,
    relative_path: String,
    message: String,
) -> Result<(), ScanSinkError> {
    summary.notices += 1;
    sink.write_notice(&ScanNotice {
        code,
        relative_path,
        message,
    })
}

fn modified_unix_nanos(metadata: &fs::Metadata) -> Option<u64> {
    metadata
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_nanos()).ok())
}

fn relative_path_to_match_path(path: &Path) -> Result<String, MatchPathError> {
    crate::normalize_relative_path(&path.to_string_lossy())
}

fn path_for_display(path: &Path) -> String {
    if path.as_os_str().is_empty() {
        ".".to_owned()
    } else {
        path.to_string_lossy().replace('\\', "/")
    }
}

#[cfg(unix)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct FileIdentity {
    device: u64,
    inode: u64,
}

#[cfg(unix)]
fn file_identity(metadata: &fs::Metadata) -> Option<FileIdentity> {
    use std::os::unix::fs::MetadataExt;
    Some(FileIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
    })
}

#[cfg(not(unix))]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct FileIdentity;

/// Windows links, junctions, and mount-point reparse entries are never traversed,
/// so an ordinary directory tree cannot cycle. Avoid approximate metadata keys:
/// equal timestamps and lengths are common for distinct directories.
#[cfg(not(unix))]
fn file_identity(_metadata: &fs::Metadata) -> Option<FileIdentity> {
    None
}

#[cfg(unix)]
fn is_mount(path: &Path, source: &Path, metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    if path == source {
        return false;
    }
    path.parent()
        .and_then(|parent| fs::metadata(parent).ok())
        .is_some_and(|parent| parent.dev() != metadata.dev())
}

#[cfg(not(unix))]
fn is_mount(_path: &Path, _source: &Path, _metadata: &fs::Metadata) -> bool {
    false
}

#[cfg(windows)]
fn classify_metadata(_path: &Path, metadata: &fs::Metadata) -> FileSystemObjectKind {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;

    if metadata.file_type().is_symlink() {
        FileSystemObjectKind::Symlink
    } else if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        FileSystemObjectKind::JunctionOrReparsePoint
    } else if metadata.is_dir() {
        FileSystemObjectKind::Directory
    } else if metadata.is_file() {
        FileSystemObjectKind::RegularFile
    } else {
        FileSystemObjectKind::SpecialFile
    }
}

#[cfg(not(windows))]
fn classify_metadata(_path: &Path, metadata: &fs::Metadata) -> FileSystemObjectKind {
    if metadata.file_type().is_symlink() {
        FileSystemObjectKind::Symlink
    } else if metadata.is_dir() {
        FileSystemObjectKind::Directory
    } else if metadata.is_file() {
        FileSystemObjectKind::RegularFile
    } else {
        FileSystemObjectKind::SpecialFile
    }
}

#[derive(Default)]
pub(crate) struct MountTable {
    mount_points: HashSet<PathBuf>,
    network_mounts: HashSet<PathBuf>,
}

impl MountTable {
    pub(crate) fn load() -> Self {
        #[cfg(target_os = "linux")]
        {
            let Ok(contents) = fs::read_to_string("/proc/self/mountinfo") else {
                return Self::default();
            };
            let mut mount_points = HashSet::new();
            let mut network_mounts = HashSet::new();
            for line in contents.lines() {
                let Some((before_separator, after_separator)) = line.split_once(" - ") else {
                    continue;
                };
                let Some(mount_point) = before_separator.split_whitespace().nth(4) else {
                    continue;
                };
                let Some(file_system) = after_separator.split_whitespace().next() else {
                    continue;
                };
                let mount_point = PathBuf::from(unescape_mount_path(mount_point));
                mount_points.insert(mount_point.clone());
                if is_network_file_system(file_system) {
                    network_mounts.insert(mount_point);
                }
            }
            Self {
                mount_points,
                network_mounts,
            }
        }
        #[cfg(not(target_os = "linux"))]
        Self::default()
    }

    pub(crate) fn is_mount(&self, path: &Path) -> bool {
        self.mount_points.contains(path)
    }

    pub(crate) fn is_network_mount(&self, path: &Path) -> bool {
        self.network_mounts.contains(path)
    }
}

#[cfg(target_os = "linux")]
fn is_network_file_system(file_system: &str) -> bool {
    matches!(
        file_system,
        "9p" | "afs" | "cifs" | "ceph" | "fuse.sshfs" | "nfs" | "nfs4" | "smb3"
    )
}

#[cfg(target_os = "linux")]
fn unescape_mount_path(path: &str) -> String {
    path.replace("\\040", " ")
        .replace("\\011", "\t")
        .replace("\\012", "\n")
        .replace("\\134", "\\")
}

/// Stable opaque ID for one native browser path.
#[must_use]
pub fn stable_node_id(path: &Path) -> String {
    let mut hasher = Sha256::new();
    update_hasher_with_os_str(&mut hasher, path.as_os_str());
    format!("{:x}", hasher.finalize())
}

#[cfg(unix)]
fn update_hasher_with_os_str(hasher: &mut Sha256, value: &OsStr) {
    use std::os::unix::ffi::OsStrExt;
    hasher.update(value.as_bytes());
}

#[cfg(windows)]
fn update_hasher_with_os_str(hasher: &mut Sha256, value: &OsStr) {
    use std::os::windows::ffi::OsStrExt;
    for unit in value.encode_wide() {
        hasher.update(unit.to_le_bytes());
    }
}

#[cfg(not(any(unix, windows)))]
fn update_hasher_with_os_str(hasher: &mut Sha256, value: &OsStr) {
    hasher.update(value.to_string_lossy().as_bytes());
}
