use std::{
    fmt, fs,
    fs::{File, OpenOptions},
    io::{self, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::UNIX_EPOCH,
};

use sha2::{Digest, Sha256};

use crate::{
    ArchiveActionSpec, ArchiveFormat, ChecksumAlgorithm, FileSystemObjectKind, OutputReservation,
    ScanDisposition, ScanSummary, ScannedEntry, UnreadablePolicy, VerificationMode,
    create_archive_writer, normalize_relative_path,
};

pub trait ExecutionEntrySource {
    fn next_entry(&mut self) -> Result<Option<ScannedEntry>, String>;
}

#[derive(Clone, Debug)]
pub struct ExecutionPlan {
    pub source_root: PathBuf,
    pub action: ArchiveActionSpec,
    pub totals: ScanSummary,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExecutionWarning {
    JunctionSkipped(String),
    SpecialFileSkipped(String),
    UnreadableEntrySkipped(String),
    SourceEntryChanged(String),
    ZipSymlinkPortability(String),
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ExecutionProgress {
    pub processed_entries: u64,
    pub processed_files: u64,
    pub processed_bytes: u64,
    pub current_path: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionResult {
    pub output_path: PathBuf,
    pub output_size: u64,
    pub checksum_sha256: Option<String>,
    pub progress: ExecutionProgress,
    pub warnings: Vec<ExecutionWarning>,
}

#[derive(Clone, Debug, Default)]
pub struct ExecutionControl {
    inner: Arc<ControlInner>,
}

#[derive(Debug, Default)]
struct ControlInner {
    paused: Mutex<bool>,
    resumed: Condvar,
    stopped: AtomicBool,
}

impl ExecutionControl {
    pub fn pause(&self) {
        *self.inner.paused.lock().expect("pause state") = true;
    }

    pub fn resume(&self) {
        *self.inner.paused.lock().expect("pause state") = false;
        self.inner.resumed.notify_all();
    }

    pub fn stop(&self) {
        self.inner.stopped.store(true, Ordering::Release);
        self.inner.resumed.notify_all();
    }

    /// Waits at an entry boundary while paused and returns `false` after stop.
    #[must_use]
    pub fn checkpoint(&self) -> bool {
        self.before_entry().is_ok()
    }

    fn before_entry(&self) -> Result<(), ExecutionError> {
        let mut paused = self.inner.paused.lock().expect("pause state");
        while *paused && !self.is_stopped() {
            paused = self.inner.resumed.wait(paused).expect("pause state");
        }
        self.check_stop()
    }

    fn check_stop(&self) -> Result<(), ExecutionError> {
        if self.is_stopped() {
            Err(ExecutionError::Stopped)
        } else {
            Ok(())
        }
    }

    #[must_use]
    pub fn is_stopped(&self) -> bool {
        self.inner.stopped.load(Ordering::Acquire)
    }

    #[must_use]
    pub fn is_paused(&self) -> bool {
        *self.inner.paused.lock().expect("pause state")
    }
}

#[derive(Debug)]
pub enum ExecutionError {
    Stopped,
    Manifest(String),
    Source { path: PathBuf, message: String },
    Archive(String),
    Verification(String),
    Publish(String),
}

impl fmt::Display for ExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stopped => formatter.write_str("execution was stopped"),
            Self::Manifest(message) => write!(formatter, "manifest read failed: {message}"),
            Self::Source { path, message } => {
                write!(
                    formatter,
                    "source read failed at {}: {message}",
                    path.display()
                )
            }
            Self::Archive(message) => write!(formatter, "archive write failed: {message}"),
            Self::Verification(message) => {
                write!(formatter, "archive verification failed: {message}")
            }
            Self::Publish(message) => write!(formatter, "archive publish failed: {message}"),
        }
    }
}

impl std::error::Error for ExecutionError {}

pub fn execute_archive(
    plan: &ExecutionPlan,
    mut reservation: OutputReservation,
    entries: &mut dyn ExecutionEntrySource,
    control: &ExecutionControl,
    mut on_progress: impl FnMut(&ExecutionProgress),
) -> Result<ExecutionResult, ExecutionError> {
    let temp_file = reservation
        .take_temp_file()
        .map_err(|error| ExecutionError::Archive(error.to_string()))?;
    let mut writer = create_archive_writer(
        plan.action.output.format,
        plan.action.output.compression,
        temp_file,
    )
    .map_err(|error| ExecutionError::Archive(error.to_string()))?;
    let mut progress = ExecutionProgress::default();
    let mut warnings = Vec::new();
    let root_name = plan
        .source_root
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "root".to_owned());

    while let Some(entry) = entries.next_entry().map_err(ExecutionError::Manifest)? {
        validate_planned_entry(plan, &entry)?;
        if is_output_artifact(&entry.native_path, &reservation) {
            continue;
        }
        if entry.disposition == ScanDisposition::Skipped {
            match entry.kind {
                FileSystemObjectKind::SpecialFile => warnings.push(
                    ExecutionWarning::SpecialFileSkipped(entry.relative_path.clone()),
                ),
                FileSystemObjectKind::Unreadable => handle_unreadable(
                    &entry,
                    plan.action.unreadable_policy,
                    &mut warnings,
                    "entry is unreadable",
                )?,
                _ => {}
            }
            continue;
        }
        if entry.disposition != ScanDisposition::Included {
            continue;
        }
        control.before_entry()?;
        progress.current_path = Some(entry.relative_path.clone());
        let archive_path = if plan.action.include_root {
            format!("{root_name}/{}", entry.relative_path)
        } else {
            entry.relative_path.clone()
        };
        match entry.kind {
            FileSystemObjectKind::Directory => writer
                .add_directory(&archive_path, &entry)
                .map_err(|error| ExecutionError::Archive(error.to_string()))?,
            FileSystemObjectKind::RegularFile => {
                let Some(mut spool) = spool_source(
                    &entry,
                    reservation
                        .temp_path()
                        .parent()
                        .unwrap_or_else(|| Path::new(".")),
                    plan.action.unreadable_policy,
                    control,
                    &mut warnings,
                )?
                else {
                    continue;
                };
                let mut controlled = ControlledReader {
                    reader: &mut spool,
                    control,
                    bytes: &mut progress.processed_bytes,
                };
                writer
                    .add_file(&archive_path, &entry, &mut controlled)
                    .map_err(|error| ExecutionError::Archive(error.to_string()))?;
                progress.processed_files += 1;
            }
            FileSystemObjectKind::Symlink => {
                writer
                    .add_symlink(&archive_path, &entry)
                    .map_err(|error| ExecutionError::Archive(error.to_string()))?;
                if plan.action.output.format == ArchiveFormat::Zip {
                    warnings.push(ExecutionWarning::ZipSymlinkPortability(
                        entry.relative_path.clone(),
                    ));
                }
            }
            FileSystemObjectKind::JunctionOrReparsePoint => {
                warnings.push(ExecutionWarning::JunctionSkipped(
                    entry.relative_path.clone(),
                ));
                continue;
            }
            FileSystemObjectKind::SpecialFile => {
                warnings.push(ExecutionWarning::SpecialFileSkipped(
                    entry.relative_path.clone(),
                ));
                continue;
            }
            FileSystemObjectKind::Unreadable => {
                handle_unreadable(
                    &entry,
                    plan.action.unreadable_policy,
                    &mut warnings,
                    "entry is unreadable",
                )?;
                continue;
            }
        }
        progress.processed_entries += 1;
        on_progress(&progress);
    }

    let file = writer
        .finish()
        .map_err(|error| ExecutionError::Archive(error.to_string()))?;
    file.sync_all()
        .map_err(|error| ExecutionError::Archive(error.to_string()))?;
    drop(file);
    verify_archive(
        reservation.temp_path(),
        plan.action.output.format,
        plan.action.verification.mode,
    )?;
    let checksum_sha256 = if plan.action.verification.checksum == ChecksumAlgorithm::Sha256 {
        Some(file_sha256(reservation.temp_path())?)
    } else {
        None
    };
    let output_size = fs::metadata(reservation.temp_path())
        .map_err(|error| ExecutionError::Archive(error.to_string()))?
        .len();
    let output_path = reservation
        .publish()
        .map_err(|error| ExecutionError::Publish(error.to_string()))?;
    progress.current_path = None;
    Ok(ExecutionResult {
        output_path,
        output_size,
        checksum_sha256,
        progress,
        warnings,
    })
}

fn is_output_artifact(path: &Path, reservation: &OutputReservation) -> bool {
    [
        reservation.final_path(),
        reservation.temp_path(),
        reservation.reservation_path(),
    ]
    .iter()
    .any(|artifact| {
        path == *artifact
            || (path.exists()
                && fs::canonicalize(path)
                    .ok()
                    .is_some_and(|canonical| canonical == **artifact))
    })
}

fn spool_source(
    entry: &ScannedEntry,
    spool_directory: &Path,
    policy: UnreadablePolicy,
    control: &ExecutionControl,
    warnings: &mut Vec<ExecutionWarning>,
) -> Result<Option<File>, ExecutionError> {
    let mut source = match open_regular_file_no_follow(&entry.native_path) {
        Ok(file) => file,
        Err(error) => {
            return handle_unreadable(entry, policy, warnings, &error.to_string()).map(|()| None);
        }
    };
    let before = match source.metadata() {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => metadata,
        Ok(_) => {
            return handle_unreadable(entry, policy, warnings, "entry is no longer a regular file")
                .map(|()| None);
        }
        Err(error) => {
            return handle_unreadable(entry, policy, warnings, &error.to_string()).map(|()| None);
        }
    };
    if before.len() != entry.size || modified_nanos(&before) != entry.modified_unix_nanos {
        return handle_changed(entry, policy, warnings).map(|()| None);
    }
    let mut spool =
        tempfile::tempfile_in(spool_directory).map_err(|error| ExecutionError::Source {
            path: entry.native_path.clone(),
            message: error.to_string(),
        })?;
    let mut buffer = [0_u8; 128 * 1024];
    loop {
        control.check_stop()?;
        let read = match source.read(&mut buffer) {
            Ok(read) => read,
            Err(error) => {
                return handle_unreadable(entry, policy, warnings, &error.to_string())
                    .map(|()| None);
            }
        };
        if read == 0 {
            break;
        }
        io::Write::write_all(&mut spool, &buffer[..read])
            .map_err(|error| ExecutionError::Archive(error.to_string()))?;
    }
    let after =
        fs::symlink_metadata(&entry.native_path).map_err(|error| ExecutionError::Source {
            path: entry.native_path.clone(),
            message: error.to_string(),
        })?;
    if !after.is_file()
        || after.file_type().is_symlink()
        || !same_file_identity(&before, &after)
        || after.len() != before.len()
        || modified_nanos(&after) != modified_nanos(&before)
    {
        return handle_changed(entry, policy, warnings).map(|()| None);
    }
    spool
        .seek(SeekFrom::Start(0))
        .map_err(|error| ExecutionError::Archive(error.to_string()))?;
    Ok(Some(spool))
}

fn validate_planned_entry(
    plan: &ExecutionPlan,
    entry: &ScannedEntry,
) -> Result<(), ExecutionError> {
    let normalized = normalize_relative_path(&entry.relative_path)
        .map_err(|error| ExecutionError::Manifest(error.to_string()))?;
    if normalized != entry.relative_path {
        return Err(ExecutionError::Manifest(format!(
            "entry path is not normalized: {}",
            entry.relative_path
        )));
    }
    let native_relative = entry
        .native_path
        .strip_prefix(&plan.source_root)
        .map_err(|_| {
            ExecutionError::Manifest(format!(
                "entry path escapes the source root: {}",
                entry.native_path.display()
            ))
        })?;
    let normalized_native = normalize_relative_path(&native_relative.to_string_lossy())
        .map_err(|error| ExecutionError::Manifest(error.to_string()))?;
    if normalized_native != entry.relative_path {
        return Err(ExecutionError::Manifest(format!(
            "entry native path does not match its relative path: {}",
            entry.relative_path
        )));
    }
    Ok(())
}

fn open_regular_file_no_follow(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(io::Error::other("source entry is not a regular file"));
    }
    Ok(file)
}

#[cfg(unix)]
fn same_file_identity(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    left.dev() == right.dev() && left.ino() == right.ino()
}

#[cfg(windows)]
fn same_file_identity(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    left.creation_time() == right.creation_time()
        && left.file_attributes() == right.file_attributes()
}

#[cfg(not(any(unix, windows)))]
fn same_file_identity(_left: &fs::Metadata, _right: &fs::Metadata) -> bool {
    false
}

fn handle_unreadable(
    entry: &ScannedEntry,
    policy: UnreadablePolicy,
    warnings: &mut Vec<ExecutionWarning>,
    message: &str,
) -> Result<(), ExecutionError> {
    match policy {
        UnreadablePolicy::Fail => Err(ExecutionError::Source {
            path: entry.native_path.clone(),
            message: message.to_owned(),
        }),
        UnreadablePolicy::WarnAndSkip => {
            warnings.push(ExecutionWarning::UnreadableEntrySkipped(
                entry.relative_path.clone(),
            ));
            Ok(())
        }
    }
}

fn handle_changed(
    entry: &ScannedEntry,
    policy: UnreadablePolicy,
    warnings: &mut Vec<ExecutionWarning>,
) -> Result<(), ExecutionError> {
    match policy {
        UnreadablePolicy::Fail => Err(ExecutionError::Source {
            path: entry.native_path.clone(),
            message: "source entry changed after planning".to_owned(),
        }),
        UnreadablePolicy::WarnAndSkip => {
            warnings.push(ExecutionWarning::SourceEntryChanged(
                entry.relative_path.clone(),
            ));
            Ok(())
        }
    }
}

struct ControlledReader<'a, R> {
    reader: &'a mut R,
    control: &'a ExecutionControl,
    bytes: &'a mut u64,
}

impl<R: Read> Read for ControlledReader<'_, R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.control
            .check_stop()
            .map_err(|_| io::Error::new(io::ErrorKind::Interrupted, "execution was stopped"))?;
        let read = self.reader.read(buffer)?;
        *self.bytes = self.bytes.saturating_add(read as u64);
        Ok(read)
    }
}

fn modified_nanos(metadata: &fs::Metadata) -> Option<u64> {
    metadata
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_nanos()).ok())
}

fn verify_archive(
    path: &Path,
    format: ArchiveFormat,
    mode: VerificationMode,
) -> Result<(), ExecutionError> {
    match format {
        ArchiveFormat::Zip => {
            let mut archive = zip::ZipArchive::new(File::open(path).map_err(verification)?)
                .map_err(verification)?;
            if mode == VerificationMode::Full {
                for index in 0..archive.len() {
                    io::copy(
                        &mut archive.by_index(index).map_err(verification)?,
                        &mut io::sink(),
                    )
                    .map_err(verification)?;
                }
            }
        }
        ArchiveFormat::TarGz => verify_tar(
            flate2::read::GzDecoder::new(File::open(path).map_err(verification)?),
            mode,
        )?,
        ArchiveFormat::TarZst => verify_tar(
            zstd::stream::read::Decoder::new(File::open(path).map_err(verification)?)
                .map_err(verification)?,
            mode,
        )?,
    }
    Ok(())
}

fn verify_tar<R: Read>(reader: R, mode: VerificationMode) -> Result<(), ExecutionError> {
    let mut archive = tar::Archive::new(reader);
    for entry in archive.entries().map_err(verification)? {
        let mut entry = entry.map_err(verification)?;
        if mode == VerificationMode::Full {
            io::copy(&mut entry, &mut io::sink()).map_err(verification)?;
        }
    }
    Ok(())
}

fn file_sha256(path: &Path) -> Result<String, ExecutionError> {
    let mut file = File::open(path).map_err(verification)?;
    let mut hasher = Sha256::new();
    io::copy(&mut file, &mut HashWriter(&mut hasher)).map_err(verification)?;
    Ok(format!("{:x}", hasher.finalize()))
}

struct HashWriter<'a>(&'a mut Sha256);

impl io::Write for HashWriter<'_> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.0.update(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn verification(error: impl fmt::Display) -> ExecutionError {
    ExecutionError::Verification(error.to_string())
}

#[cfg(test)]
mod tests {
    use std::{sync::mpsc, thread, time::Duration};

    use super::ExecutionControl;

    #[test]
    fn pause_waits_without_starting_an_entry_and_resume_wakes_it() {
        let control = ExecutionControl::default();
        control.pause();
        let worker_control = control.clone();
        let (sender, receiver) = mpsc::channel();
        let worker = thread::spawn(move || {
            worker_control.before_entry().expect("resumed");
            sender.send(()).expect("signal");
        });

        assert!(receiver.recv_timeout(Duration::from_millis(50)).is_err());
        control.resume();
        receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("worker resumed");
        worker.join().expect("worker");
    }

    #[test]
    fn stop_wakes_a_paused_worker() {
        let control = ExecutionControl::default();
        control.pause();
        let worker_control = control.clone();
        let worker = thread::spawn(move || worker_control.before_entry());
        control.stop();

        assert!(worker.join().expect("worker").is_err());
    }
}
