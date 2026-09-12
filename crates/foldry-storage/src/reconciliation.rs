use std::{
    collections::BTreeSet,
    fs, io,
    path::{Path, PathBuf},
};

use foldry_application::{
    LEGACY_RESERVATION_METADATA_VERSION, RESERVATION_METADATA_VERSION, RepositoryError,
    ReservationMetadata, ReservationOwnerKind, RunHistoryRepository, RunId,
};
use jiff::Timestamp;
use sysinfo::{Pid, System};

const RESERVATION_SUFFIX: &str = ".foldry-reserve";
const MANIFEST_SUFFIX: &str = ".foldry-manifest";

pub trait ProcessProbe {
    fn matches(&self, process_id: u32, started_unix_seconds: Option<u64>) -> bool;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemProcessProbe;

impl ProcessProbe for SystemProcessProbe {
    fn matches(&self, process_id: u32, started_unix_seconds: Option<u64>) -> bool {
        if process_id == 0 {
            return false;
        }
        System::new_all()
            .process(Pid::from_u32(process_id))
            .is_some_and(|process| {
                started_unix_seconds.is_none_or(|started| process.start_time() == started)
            })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryContext {
    pub owner_kind: ReservationOwnerKind,
    pub instance_id: String,
    pub directory_scope_id: String,
    pub exclusive_scope_guard: bool,
}

#[must_use]
pub fn current_process_started_unix_seconds() -> u64 {
    System::new_all()
        .process(Pid::from_u32(std::process::id()))
        .map_or(0, sysinfo::Process::start_time)
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ArtifactCleanupReport {
    pub removed_reservations: u64,
    pub removed_temp_files: u64,
    pub removed_manifests: u64,
    pub retained_active: u64,
    pub retained_recent: u64,
    pub retained_unverified: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct StartupReconciliationReport {
    pub interrupted_runs: u64,
    pub artifacts: ArtifactCleanupReport,
}

pub fn reconcile_startup(
    history: &dyn RunHistoryRepository,
    at: Timestamp,
    output_directories: &[PathBuf],
    manifest_directory: &Path,
    minimum_age_seconds: u64,
    processes: &dyn ProcessProbe,
    context: &RecoveryContext,
) -> Result<StartupReconciliationReport, RepositoryError> {
    let mut artifacts = clean_stale_output_artifacts(
        output_directories,
        at.as_second(),
        minimum_age_seconds,
        processes,
        context,
    )?;
    artifacts.removed_manifests =
        clean_stale_manifests(manifest_directory, at.as_second(), minimum_age_seconds)?;
    let interrupted_runs = history.mark_unfinished_interrupted(at)?;
    Ok(StartupReconciliationReport {
        interrupted_runs,
        artifacts,
    })
}

pub fn clean_stale_output_artifacts(
    output_directories: &[PathBuf],
    now_unix_seconds: i64,
    minimum_age_seconds: u64,
    processes: &dyn ProcessProbe,
    context: &RecoveryContext,
) -> Result<ArtifactCleanupReport, RepositoryError> {
    let mut report = ArtifactCleanupReport::default();
    let directories = output_directories
        .iter()
        .map(|path| fs::canonicalize(path).unwrap_or_else(|_| path.clone()))
        .collect::<BTreeSet<_>>();
    for directory in directories {
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(repository_error(error)),
        };
        for entry in entries {
            let entry = entry.map_err(repository_error)?;
            let path = entry.path();
            let file_type = entry.file_type().map_err(repository_error)?;
            if !file_type.is_file() || file_type.is_symlink() || !is_reservation_name(&path) {
                continue;
            }
            let Some((metadata, expected_temp)) = verified_metadata(&path)? else {
                report.retained_unverified += 1;
                continue;
            };
            let legacy = metadata.version == LEGACY_RESERVATION_METADATA_VERSION;
            if !legacy && !metadata_matches_scope(&metadata, context) {
                report.retained_unverified += 1;
                continue;
            }
            if !is_old_enough(
                metadata.created_unix_seconds,
                now_unix_seconds,
                if legacy { minimum_age_seconds } else { 0 },
            ) {
                report.retained_recent += 1;
                continue;
            }
            if owner_is_active(&metadata, processes, context) {
                report.retained_active += 1;
                continue;
            }
            match fs::symlink_metadata(&expected_temp) {
                Ok(temp_metadata)
                    if temp_metadata.file_type().is_file()
                        && !temp_metadata.file_type().is_symlink() =>
                {
                    fs::remove_file(&expected_temp).map_err(repository_error)?;
                    report.removed_temp_files += 1;
                }
                Ok(_) => {
                    report.retained_unverified += 1;
                    continue;
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(repository_error(error)),
            }
            fs::remove_file(&path).map_err(repository_error)?;
            report.removed_reservations += 1;
        }
    }
    Ok(report)
}

pub fn clean_stale_manifests(
    directory: &Path,
    now_unix_seconds: i64,
    minimum_age_seconds: u64,
) -> Result<u64, RepositoryError> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(repository_error(error)),
    };
    let mut removed = 0;
    for entry in entries {
        let entry = entry.map_err(repository_error)?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(repository_error)?;
        let valid_run_id = path
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.strip_suffix(MANIFEST_SUFFIX))
            .and_then(|id| id.parse::<RunId>().ok())
            .is_some();
        if !file_type.is_file() || file_type.is_symlink() || !valid_run_id {
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .and_then(|duration| i64::try_from(duration.as_secs()).ok());
        if modified
            .is_some_and(|created| is_old_enough(created, now_unix_seconds, minimum_age_seconds))
        {
            fs::remove_file(path).map_err(repository_error)?;
            removed += 1;
        }
    }
    Ok(removed)
}

fn verified_metadata(
    reservation_path: &Path,
) -> Result<Option<(ReservationMetadata, PathBuf)>, RepositoryError> {
    let Some(name) = reservation_path.file_name().and_then(|name| name.to_str()) else {
        return Ok(None);
    };
    let Some(final_name) = name
        .strip_prefix('.')
        .and_then(|name| name.strip_suffix(RESERVATION_SUFFIX))
        .filter(|name| !name.is_empty())
    else {
        return Ok(None);
    };
    let contents = fs::read(reservation_path).map_err(repository_error)?;
    let Ok(metadata) = serde_json::from_slice::<ReservationMetadata>(&contents) else {
        return Ok(None);
    };
    if !matches!(
        metadata.version,
        LEGACY_RESERVATION_METADATA_VERSION | RESERVATION_METADATA_VERSION
    ) || metadata.process_id == 0
    {
        return Ok(None);
    }
    let expected_name = format!(".{final_name}.{}.part", metadata.run_id);
    if metadata.temp_file_name != expected_name
        || Path::new(&metadata.temp_file_name).components().count() != 1
    {
        return Ok(None);
    }
    if metadata.version == RESERVATION_METADATA_VERSION
        && (metadata.final_file_name.as_deref() != Some(final_name)
            || metadata.owner_kind.is_none()
            || metadata.instance_id.as_deref().is_none_or(str::is_empty)
            || metadata
                .directory_scope_id
                .as_deref()
                .is_none_or(str::is_empty)
            || metadata.process_started_unix_seconds.is_none())
    {
        return Ok(None);
    }
    Ok(Some((
        metadata.clone(),
        reservation_path.with_file_name(metadata.temp_file_name),
    )))
}

fn metadata_matches_scope(metadata: &ReservationMetadata, context: &RecoveryContext) -> bool {
    metadata.owner_kind == Some(context.owner_kind)
        && metadata.directory_scope_id.as_deref() == Some(context.directory_scope_id.as_str())
}

fn owner_is_active(
    metadata: &ReservationMetadata,
    processes: &dyn ProcessProbe,
    context: &RecoveryContext,
) -> bool {
    if metadata.version == LEGACY_RESERVATION_METADATA_VERSION {
        return processes.matches(metadata.process_id, None);
    }
    if metadata.instance_id.as_deref() == Some(context.instance_id.as_str()) {
        return true;
    }
    if context.owner_kind == ReservationOwnerKind::Desktop && context.exclusive_scope_guard {
        return false;
    }
    processes.matches(metadata.process_id, metadata.process_started_unix_seconds)
}

fn is_reservation_name(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with('.') && name.ends_with(RESERVATION_SUFFIX))
}

fn is_old_enough(created: i64, now: i64, minimum_age_seconds: u64) -> bool {
    now.checked_sub(created)
        .and_then(|age| u64::try_from(age).ok())
        .is_some_and(|age| age >= minimum_age_seconds)
}

fn repository_error(error: impl std::fmt::Display) -> RepositoryError {
    RepositoryError::new(error.to_string())
}
