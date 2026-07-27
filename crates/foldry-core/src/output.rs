use std::{
    fmt, fs,
    fs::{File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::{ArchiveFormat, ArchiveOutputSpec, ConflictPolicy, RunId};

pub const RESERVATION_METADATA_VERSION: u16 = 1;

/// Durable ownership proof used by startup recovery after an unclean shutdown.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReservationMetadata {
    pub version: u16,
    pub run_id: RunId,
    pub process_id: u32,
    pub created_unix_seconds: i64,
    pub temp_file_name: String,
}

#[derive(Debug)]
pub enum PlanOutput {
    Skipped { path: PathBuf },
    Reserved(OutputReservation),
}

#[derive(Debug)]
pub enum OutputPlanError {
    InvalidSource(PathBuf),
    InvalidOutputDirectory(PathBuf),
    InvalidFilename(String),
    SourceEqualsOutput(PathBuf),
    Conflict(PathBuf),
    NoIncrementAvailable(PathBuf),
    Io { path: PathBuf, source: io::Error },
}

impl fmt::Display for OutputPlanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSource(path) => {
                write!(
                    formatter,
                    "source is not a readable directory: {}",
                    path.display()
                )
            }
            Self::InvalidOutputDirectory(path) => {
                write!(
                    formatter,
                    "output directory is unavailable: {}",
                    path.display()
                )
            }
            Self::InvalidFilename(name) => write!(formatter, "invalid archive filename: {name}"),
            Self::SourceEqualsOutput(path) => {
                write!(
                    formatter,
                    "archive output cannot equal source: {}",
                    path.display()
                )
            }
            Self::Conflict(path) => {
                write!(formatter, "output is already reserved: {}", path.display())
            }
            Self::NoIncrementAvailable(path) => {
                write!(
                    formatter,
                    "cannot find a free incremented name near {}",
                    path.display()
                )
            }
            Self::Io { path, source } => write!(
                formatter,
                "output I/O failed at {}: {source}",
                path.display()
            ),
        }
    }
}

impl std::error::Error for OutputPlanError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// Resolves and atomically reserves an output name before any archive bytes are written.
pub fn reserve_output(
    source: &Path,
    spec: &ArchiveOutputSpec,
    run_id: RunId,
) -> Result<PlanOutput, OutputPlanError> {
    let canonical_source = fs::canonicalize(source)
        .map_err(|_| OutputPlanError::InvalidSource(source.to_path_buf()))?;
    if !canonical_source.is_dir() {
        return Err(OutputPlanError::InvalidSource(source.to_path_buf()));
    }
    let canonical_directory = fs::canonicalize(&spec.directory)
        .map_err(|_| OutputPlanError::InvalidOutputDirectory(spec.directory.clone()))?;
    if !canonical_directory.is_dir() {
        return Err(OutputPlanError::InvalidOutputDirectory(
            spec.directory.clone(),
        ));
    }
    let base_name = archive_filename(&spec.filename, spec.format)?;
    let base_path = canonical_directory.join(&base_name);
    if base_path == canonical_source {
        return Err(OutputPlanError::SourceEqualsOutput(base_path));
    }

    match spec.conflict_policy {
        ConflictPolicy::Skip => {
            if base_path.exists() {
                return Ok(PlanOutput::Skipped { path: base_path });
            }
            match try_reserve(base_path.clone(), spec.conflict_policy, run_id) {
                Ok(reservation) => Ok(PlanOutput::Reserved(reservation)),
                Err(OutputPlanError::Conflict(_)) => Ok(PlanOutput::Skipped { path: base_path }),
                Err(error) => Err(error),
            }
        }
        ConflictPolicy::Overwrite => {
            try_reserve(base_path, spec.conflict_policy, run_id).map(PlanOutput::Reserved)
        }
        ConflictPolicy::Increment => {
            for index in 0..10_000_u32 {
                let candidate = if index == 0 {
                    base_path.clone()
                } else {
                    incremented_path(&base_path, index)
                };
                if candidate.exists() {
                    continue;
                }
                match try_reserve(candidate, spec.conflict_policy, run_id) {
                    Ok(reservation) => return Ok(PlanOutput::Reserved(reservation)),
                    Err(OutputPlanError::Conflict(_)) => {}
                    Err(error) => return Err(error),
                }
            }
            Err(OutputPlanError::NoIncrementAvailable(base_path))
        }
    }
}

fn try_reserve(
    final_path: PathBuf,
    policy: ConflictPolicy,
    run_id: RunId,
) -> Result<OutputReservation, OutputPlanError> {
    let file_name = final_path
        .file_name()
        .expect("validated output filename")
        .to_string_lossy();
    let reservation_path = final_path.with_file_name(format!(".{file_name}.foldry-reserve"));
    let temp_file_name = format!(".{file_name}.{run_id}.part");
    let temp_path = final_path.with_file_name(&temp_file_name);
    let mut reservation_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&reservation_path)
        .map_err(|source| {
            if source.kind() == io::ErrorKind::AlreadyExists {
                OutputPlanError::Conflict(final_path.clone())
            } else {
                OutputPlanError::Io {
                    path: reservation_path.clone(),
                    source,
                }
            }
        })?;
    let metadata = ReservationMetadata {
        version: RESERVATION_METADATA_VERSION,
        run_id,
        process_id: std::process::id(),
        created_unix_seconds: unix_seconds(SystemTime::now()),
        temp_file_name,
    };
    let encoded = serde_json::to_vec(&metadata).map_err(|source| OutputPlanError::Io {
        path: reservation_path.clone(),
        source: io::Error::other(source),
    })?;
    if let Err(source) = reservation_file
        .write_all(&encoded)
        .and_then(|()| reservation_file.write_all(b"\n"))
        .and_then(|()| reservation_file.sync_all())
    {
        let _ = fs::remove_file(&reservation_path);
        return Err(OutputPlanError::Io {
            path: reservation_path,
            source,
        });
    }
    let temp_file = match OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&temp_path)
    {
        Ok(file) => file,
        Err(source) => {
            let _ = fs::remove_file(&reservation_path);
            return Err(OutputPlanError::Io {
                path: temp_path,
                source,
            });
        }
    };

    Ok(OutputReservation {
        final_path,
        temp_path,
        reservation_path,
        policy,
        temp_file: Some(temp_file),
        published: false,
    })
}

#[derive(Debug)]
pub struct OutputReservation {
    final_path: PathBuf,
    temp_path: PathBuf,
    reservation_path: PathBuf,
    policy: ConflictPolicy,
    temp_file: Option<File>,
    published: bool,
}

impl OutputReservation {
    #[must_use]
    pub fn final_path(&self) -> &Path {
        &self.final_path
    }

    #[must_use]
    pub fn temp_path(&self) -> &Path {
        &self.temp_path
    }

    #[must_use]
    pub fn reservation_path(&self) -> &Path {
        &self.reservation_path
    }

    pub fn take_temp_file(&mut self) -> Result<File, OutputPlanError> {
        self.temp_file.take().ok_or_else(|| OutputPlanError::Io {
            path: self.temp_path.clone(),
            source: io::Error::other("temporary file is already in use"),
        })
    }

    /// Publishes only a fully closed and synced temp archive.
    pub fn publish(mut self) -> Result<PathBuf, OutputPlanError> {
        if self.temp_file.is_some() {
            return Err(OutputPlanError::Io {
                path: self.temp_path.clone(),
                source: io::Error::other("temporary archive was not finalized"),
            });
        }
        let result = match self.policy {
            ConflictPolicy::Overwrite => {
                atomicwrites::replace_atomic(&self.temp_path, &self.final_path)
            }
            ConflictPolicy::Skip | ConflictPolicy::Increment => {
                atomicwrites::move_atomic(&self.temp_path, &self.final_path)
            }
        };
        result.map_err(|source| OutputPlanError::Io {
            path: self.final_path.clone(),
            source,
        })?;
        self.published = true;
        let _ = fs::remove_file(&self.reservation_path);
        Ok(self.final_path.clone())
    }
}

impl Drop for OutputReservation {
    fn drop(&mut self) {
        self.temp_file.take();
        if !self.published {
            let _ = fs::remove_file(&self.temp_path);
            let _ = fs::remove_file(&self.reservation_path);
        }
    }
}

fn archive_filename(name: &str, format: ArchiveFormat) -> Result<String, OutputPlanError> {
    if name.trim().is_empty()
        || name.contains(['/', '\\'])
        || Path::new(name).components().count() != 1
        || matches!(name, "." | "..")
    {
        return Err(OutputPlanError::InvalidFilename(name.to_owned()));
    }
    let extension = match format {
        ArchiveFormat::Zip => ".zip",
        ArchiveFormat::TarGz => ".tar.gz",
        ArchiveFormat::TarZst => ".tar.zst",
    };
    Ok(if name.to_ascii_lowercase().ends_with(extension) {
        name.to_owned()
    } else {
        format!("{name}{extension}")
    })
}

fn unix_seconds(time: SystemTime) -> i64 {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_secs()).ok())
        .unwrap_or(0)
}

fn incremented_path(base: &Path, index: u32) -> PathBuf {
    let name = base
        .file_name()
        .expect("base has filename")
        .to_string_lossy();
    let (stem, extension) = if let Some(stem) = name.strip_suffix(".tar.gz") {
        (stem, ".tar.gz")
    } else if let Some(stem) = name.strip_suffix(".tar.zst") {
        (stem, ".tar.zst")
    } else if let Some(stem) = name.strip_suffix(".zip") {
        (stem, ".zip")
    } else {
        (name.as_ref(), "")
    };
    base.with_file_name(format!("{stem} ({index}){extension}"))
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(2_048))]

        #[test]
        fn accepted_archive_names_remain_single_safe_components(
            name in prop::collection::vec(any::<char>(), 0..256)
                .prop_map(|characters| characters.into_iter().collect::<String>()),
            format in prop_oneof![
                Just(ArchiveFormat::Zip),
                Just(ArchiveFormat::TarGz),
                Just(ArchiveFormat::TarZst),
            ],
        ) {
            if let Ok(filename) = archive_filename(&name, format) {
                prop_assert!(!filename.contains(['/', '\\']));
                prop_assert_eq!(Path::new(&filename).components().count(), 1);
                prop_assert!(!Path::new(&filename).is_absolute());
                let base = Path::new("/safe-output").join(&filename);
                let incremented = incremented_path(&base, 9);
                prop_assert_eq!(incremented.parent(), base.parent());
                prop_assert_eq!(incremented.components().count(), base.components().count());
            }
        }
    }
}
