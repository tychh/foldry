use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

use foldry_application::{
    ActionSpec, ArchiveArtifact, CompiledProfile, ErrorCode, ExecutionControl, ExecutionError,
    ExecutionPlan, ExecutionWarning, Extensions, FileSystemScanner, FoldryError, FoldryWarning,
    LogLevel, PlanOutput, ProgressPhase, ProgressSnapshot, ResultSummary, RunExecutor, RunOutcome,
    RunRecord, RunReporter, ScanSink, ScanSinkError, ScannedEntry, WarningCode,
    detect_case_sensitivity, execute_archive, parse_profile, reserve_output,
};

use crate::{ManifestEntryReader, ManifestWriter};

pub struct ArchiveRunExecutor {
    manifest_directory: PathBuf,
}

impl ArchiveRunExecutor {
    #[must_use]
    pub fn new(manifest_directory: PathBuf) -> Self {
        Self { manifest_directory }
    }
}

impl RunExecutor for ArchiveRunExecutor {
    fn execute(
        &self,
        run: &RunRecord,
        control: &ExecutionControl,
        reporter: &dyn RunReporter,
    ) -> ResultSummary {
        let started = Instant::now();
        let result = self.execute_inner(run, control, reporter, started);
        match result {
            Ok(summary) => summary,
            Err(_error) if control.is_stopped() => stopped_summary(started.elapsed()),
            Err(error) => {
                reporter.error(error.clone());
                failed_summary(error, started.elapsed())
            }
        }
    }
}

impl ArchiveRunExecutor {
    fn execute_inner(
        &self,
        run: &RunRecord,
        control: &ExecutionControl,
        reporter: &dyn RunReporter,
        started: Instant,
    ) -> Result<ResultSummary, FoldryError> {
        reporter.log(LogLevel::Info, "planning archive".into(), None);
        let mut action = match &run.snapshot.action.spec {
            ActionSpec::Archive(action) => action.clone(),
            ActionSpec::Unsupported(action) => {
                return Err(foldry_error(
                    ErrorCode::UnsupportedAction,
                    format!("action type `{}` is unsupported", action.action_type),
                    None,
                ));
            }
        };
        action.output.filename =
            resolve_filename_template(&action.output.filename, &run.snapshot.folder.source)?;
        let parsed = parse_profile(&run.snapshot.profile_text);
        let profile = parsed.profile.ok_or_else(|| {
            foldry_error(
                ErrorCode::InvalidProfile,
                parsed
                    .diagnostics
                    .iter()
                    .map(|diagnostic| diagnostic.message.as_str())
                    .collect::<Vec<_>>()
                    .join("; "),
                None,
            )
        })?;
        if profile.id != run.snapshot.effective_profile_id {
            return Err(foldry_error(
                ErrorCode::InvalidProfile,
                "snapshot profile ID does not match the effective action profile ID".into(),
                None,
            ));
        }
        let case = detect_case_sensitivity(&run.snapshot.folder.source).map_err(|error| {
            foldry_error(
                ErrorCode::SourceUnavailable,
                error.to_string(),
                Some(run.snapshot.folder.source.to_string_lossy().into_owned()),
            )
        })?;
        let matcher = CompiledProfile::new(&profile, case.value)
            .map_err(|message| foldry_error(ErrorCode::InvalidProfile, message, None))?;
        if !control.checkpoint() {
            return Err(cancelled_error());
        }
        let reservation = reserve_output(&run.snapshot.folder.source, &action.output, run.run_id)
            .map_err(|error| {
            foldry_error(
                ErrorCode::OutputUnavailable,
                error.to_string(),
                action
                    .output
                    .directory
                    .resolve(&run.snapshot.folder.source)
                    .map(|path| path.to_string_lossy().into_owned()),
            )
        })?;
        let PlanOutput::Reserved(reservation) = reservation else {
            let PlanOutput::Skipped { path } = reservation else {
                unreachable!()
            };
            let size_bytes = path.metadata().map_or(0, |metadata| metadata.len());
            reporter.log(
                LogLevel::Info,
                "output conflict policy skipped archive creation".into(),
                Some(path.to_string_lossy().into_owned()),
            );
            return Ok(ResultSummary {
                outcome: RunOutcome::Succeeded,
                included_entries: 0,
                skipped_entries: 0,
                source_bytes: 0,
                duration_ms: duration_ms(started.elapsed()),
                artifact: Some(ArchiveArtifact {
                    path,
                    size_bytes,
                    checksum_sha256: None,
                }),
                warnings: Vec::new(),
                error: None,
            });
        };
        reporter.log(
            LogLevel::Info,
            "archive output planned".into(),
            Some(reservation.final_path().to_string_lossy().into_owned()),
        );
        let manifest_id = run.run_id.to_string();
        let writer =
            ManifestWriter::create(&self.manifest_directory, &manifest_id).map_err(|error| {
                foldry_error(
                    ErrorCode::WriteFailed,
                    error.to_string(),
                    Some(self.manifest_directory.to_string_lossy().into_owned()),
                )
            })?;
        let mut sink = ControlledManifestSink { writer, control };
        reporter.progress(ProgressSnapshot {
            phase: ProgressPhase::Planning,
            completed_entries: 0,
            total_entries: None,
            completed_bytes: 0,
            total_bytes: None,
            current_path: None,
        });
        let totals = FileSystemScanner::scan(
            &run.snapshot.folder.source,
            &matcher,
            &mut sink,
            &Default::default(),
        )
        .map_err(|error| {
            if control.is_stopped() {
                cancelled_error()
            } else {
                foldry_error(
                    ErrorCode::ReadFailed,
                    error.to_string(),
                    Some(run.snapshot.folder.source.to_string_lossy().into_owned()),
                )
            }
        })?;
        if !control.checkpoint() {
            return Err(cancelled_error());
        }
        let handle = sink
            .writer
            .finish()
            .map_err(|error| foldry_error(ErrorCode::WriteFailed, error.to_string(), None))?;
        let mut entries = match ManifestEntryReader::open(&handle) {
            Ok(entries) => entries,
            Err(error) => {
                let _ = handle.remove();
                return Err(foldry_error(ErrorCode::ReadFailed, error.to_string(), None));
            }
        };
        let plan = ExecutionPlan {
            source_root: run.snapshot.folder.source.clone(),
            action,
            totals: totals.clone(),
        };
        let execution = execute_archive(&plan, reservation, &mut entries, control, |progress| {
            reporter.progress(ProgressSnapshot {
                phase: ProgressPhase::Archiving,
                completed_entries: progress.processed_entries,
                total_entries: Some(totals.included_entries),
                completed_bytes: progress.processed_bytes,
                total_bytes: Some(totals.included_bytes),
                current_path: progress.current_path.clone(),
            });
        });
        drop(entries);
        let cleanup = handle.remove();
        let execution =
            execution.map_err(|error| execution_error(error, &run.snapshot.folder.source))?;
        cleanup.map_err(|error| foldry_error(ErrorCode::WriteFailed, error.to_string(), None))?;
        let warnings = execution
            .warnings
            .into_iter()
            .map(execution_warning)
            .collect::<Vec<_>>();
        for warning in &warnings {
            reporter.warning(warning.clone());
        }
        reporter.log(
            LogLevel::Info,
            "archive published".into(),
            Some(execution.output_path.to_string_lossy().into_owned()),
        );
        Ok(ResultSummary {
            outcome: if warnings.is_empty() {
                RunOutcome::Succeeded
            } else {
                RunOutcome::SucceededWithWarnings
            },
            included_entries: totals.included_entries,
            skipped_entries: totals.skipped_entries,
            source_bytes: totals.included_bytes,
            duration_ms: duration_ms(started.elapsed()),
            artifact: Some(ArchiveArtifact {
                path: execution.output_path,
                size_bytes: execution.output_size,
                checksum_sha256: execution.checksum_sha256,
            }),
            warnings,
            error: None,
        })
    }
}

struct ControlledManifestSink<'a> {
    writer: ManifestWriter,
    control: &'a ExecutionControl,
}

impl ScanSink for ControlledManifestSink<'_> {
    fn write_entry(&mut self, entry: &ScannedEntry) -> Result<(), ScanSinkError> {
        if !self.control.checkpoint() {
            return Err(ScanSinkError("execution stopped".into()));
        }
        self.writer.write_entry(entry)
    }

    fn write_notice(
        &mut self,
        notice: &foldry_application::ScanNotice,
    ) -> Result<(), ScanSinkError> {
        if !self.control.checkpoint() {
            return Err(ScanSinkError("execution stopped".into()));
        }
        self.writer.write_notice(notice)
    }
}

fn execution_warning(warning: ExecutionWarning) -> FoldryWarning {
    let (code, message, path) = match warning {
        ExecutionWarning::JunctionSkipped(path) => (
            WarningCode::JunctionSkipped,
            "junction was skipped".into(),
            path,
        ),
        ExecutionWarning::SpecialFileSkipped(path) => (
            WarningCode::SpecialFileSkipped,
            "special file was skipped".into(),
            path,
        ),
        ExecutionWarning::UnreadableEntrySkipped(path) => (
            WarningCode::UnreadableEntrySkipped,
            "unreadable entry was skipped".into(),
            path,
        ),
        ExecutionWarning::SourceEntryChanged(path) => (
            WarningCode::SourceEntryChanged,
            "source entry changed after planning".into(),
            path,
        ),
        ExecutionWarning::ZipSymlinkPortability(path) => (
            WarningCode::ZipSymlinkPortability,
            "ZIP symlink restoration depends on the extractor".into(),
            path,
        ),
    };
    FoldryWarning {
        code,
        message,
        path: Some(path),
        extensions: Extensions::new(),
    }
}

fn execution_error(error: ExecutionError, source: &std::path::Path) -> FoldryError {
    let code = match error {
        ExecutionError::Stopped => ErrorCode::Cancelled,
        ExecutionError::Manifest(_) | ExecutionError::Source { .. } => ErrorCode::ReadFailed,
        ExecutionError::Archive(_) | ExecutionError::Publish(_) => ErrorCode::WriteFailed,
        ExecutionError::Verification(_) => ErrorCode::VerificationFailed,
    };
    foldry_error(
        code,
        error.to_string(),
        Some(source.to_string_lossy().into_owned()),
    )
}

fn resolve_filename_template(
    template: &str,
    source: &std::path::Path,
) -> Result<String, FoldryError> {
    let folder = source
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            foldry_error(
                ErrorCode::InvalidConfiguration,
                format!("source {} has no usable folder name", source.display()),
                Some(source.to_string_lossy().into_owned()),
            )
        })?;
    let date = jiff::Zoned::now().date().to_string();
    Ok(template
        .replace("{folder}", folder)
        .replace("{date}", &date))
}

fn foldry_error(code: ErrorCode, message: String, path: Option<String>) -> FoldryError {
    FoldryError {
        code,
        message,
        retryable: matches!(
            code,
            ErrorCode::SourceUnavailable
                | ErrorCode::OutputUnavailable
                | ErrorCode::ReadFailed
                | ErrorCode::WriteFailed
        ),
        path,
        extensions: Extensions::new(),
    }
}

fn cancelled_error() -> FoldryError {
    foldry_error(ErrorCode::Cancelled, "execution was stopped".into(), None)
}

fn failed_summary(error: FoldryError, elapsed: Duration) -> ResultSummary {
    ResultSummary {
        outcome: RunOutcome::Failed,
        included_entries: 0,
        skipped_entries: 0,
        source_bytes: 0,
        duration_ms: duration_ms(elapsed),
        artifact: None,
        warnings: Vec::new(),
        error: Some(error),
    }
}

fn stopped_summary(elapsed: Duration) -> ResultSummary {
    ResultSummary {
        outcome: RunOutcome::Stopped,
        included_entries: 0,
        skipped_entries: 0,
        source_bytes: 0,
        duration_ms: duration_ms(elapsed),
        artifact: None,
        warnings: Vec::new(),
        error: None,
    }
}

fn duration_ms(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}
