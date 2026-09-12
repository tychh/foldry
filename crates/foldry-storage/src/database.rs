use std::{
    path::Path,
    sync::{Mutex, MutexGuard},
    time::Duration,
};

use foldry_application::{
    ActionCheckpoint, ActionId, ActionOperationalState, FolderId, LogRecord, LogRepository,
    OutputDirectoryRegistry, PageRequest, RepositoryError, RunHistoryRepository, RunId, RunRecord,
    TerminalRunCommit,
};
use jiff::Timestamp;
use rusqlite::{Connection, OptionalExtension, Transaction, params};

const SCHEMA_VERSION: i64 = 3;

pub struct SqliteRepository {
    connection: Mutex<Connection>,
}

impl SqliteRepository {
    pub fn open(path: &Path) -> Result<Self, RepositoryError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(repository_error)?;
        }
        let connection = Connection::open(path).map_err(repository_error)?;
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(repository_error)?;
        connection
            .execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")
            .map_err(repository_error)?;
        migrate(&connection)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub fn open_in_memory() -> Result<Self, RepositoryError> {
        let connection = Connection::open_in_memory().map_err(repository_error)?;
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(repository_error)?;
        migrate(&connection)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    fn connection(&self) -> Result<MutexGuard<'_, Connection>, RepositoryError> {
        self.connection
            .lock()
            .map_err(|_| RepositoryError::new("SQLite repository lock is poisoned"))
    }
}

fn migrate(connection: &Connection) -> Result<(), RepositoryError> {
    let mut version = connection
        .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
        .map_err(repository_error)?;
    if version > SCHEMA_VERSION {
        return Err(RepositoryError::new(format!(
            "database schema {version} is newer than supported schema {SCHEMA_VERSION}"
        )));
    }
    while version < SCHEMA_VERSION {
        let transaction = connection
            .unchecked_transaction()
            .map_err(repository_error)?;
        match version + 1 {
            1 => transaction
                .execute_batch(
                    "
            CREATE TABLE runs (
                run_id TEXT PRIMARY KEY,
                folder_id TEXT NOT NULL,
                action_id TEXT NOT NULL,
                state TEXT NOT NULL,
                started_at TEXT NOT NULL,
                finished_at TEXT,
                snapshot_json TEXT NOT NULL,
                summary_json TEXT
            );
            CREATE INDEX runs_started_at_idx ON runs(started_at DESC);
            CREATE INDEX runs_folder_action_idx ON runs(folder_id, action_id, started_at DESC);
            CREATE TABLE run_warnings (
                run_id TEXT NOT NULL REFERENCES runs(run_id) ON DELETE CASCADE,
                ordinal INTEGER NOT NULL,
                payload_json TEXT NOT NULL,
                PRIMARY KEY (run_id, ordinal)
            );
            CREATE TABLE run_errors (
                run_id TEXT PRIMARY KEY REFERENCES runs(run_id) ON DELETE CASCADE,
                payload_json TEXT NOT NULL
            );
            CREATE TABLE logs (
                run_id TEXT NOT NULL REFERENCES runs(run_id) ON DELETE CASCADE,
                sequence INTEGER NOT NULL,
                occurred_at TEXT NOT NULL,
                level TEXT NOT NULL,
                message TEXT NOT NULL,
                path TEXT,
                PRIMARY KEY (run_id, sequence)
            );
            CREATE INDEX logs_run_sequence_idx ON logs(run_id, sequence);
            PRAGMA user_version = 1;
            ",
                )
                .map_err(repository_error)?,
            2 => transaction
                .execute_batch(
                    "
                    CREATE TABLE output_directories (
                        path TEXT PRIMARY KEY,
                        last_used_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                    );
                    PRAGMA user_version = 2;
                    ",
                )
                .map_err(repository_error)?,
            3 => transaction
                .execute_batch(
                    "
                    CREATE TABLE action_checkpoints (
                        folder_id TEXT NOT NULL,
                        action_id TEXT NOT NULL,
                        algorithm_version INTEGER NOT NULL,
                        source_fingerprint TEXT NOT NULL,
                        effective_profile_hash TEXT NOT NULL,
                        source_summary_json TEXT NOT NULL,
                        run_id TEXT NOT NULL,
                        completed_at TEXT NOT NULL,
                        PRIMARY KEY (folder_id, action_id)
                    );
                    CREATE INDEX action_checkpoints_folder_idx
                        ON action_checkpoints(folder_id);
                    CREATE TABLE action_operational_state (
                        folder_id TEXT NOT NULL,
                        action_id TEXT NOT NULL,
                        latest_run_id TEXT NOT NULL,
                        latest_outcome TEXT NOT NULL,
                        latest_finished_at TEXT NOT NULL,
                        last_successful_artifact_json TEXT,
                        PRIMARY KEY (folder_id, action_id)
                    );
                    CREATE INDEX action_operational_state_folder_idx
                        ON action_operational_state(folder_id);
                    PRAGMA user_version = 3;
                    ",
                )
                .map_err(repository_error)?,
            target => {
                return Err(RepositoryError::new(format!(
                    "missing database migration to schema {target}"
                )));
            }
        }
        transaction.commit().map_err(repository_error)?;
        version += 1;
    }
    Ok(())
}

impl OutputDirectoryRegistry for SqliteRepository {
    fn register(&self, directory: &Path) -> Result<(), RepositoryError> {
        let canonical = std::fs::canonicalize(directory).map_err(repository_error)?;
        if !canonical.is_dir() {
            return Err(RepositoryError::new(format!(
                "output directory is not a directory: {}",
                canonical.display()
            )));
        }
        self.connection()?
            .execute(
                "INSERT INTO output_directories(path, last_used_at)
                 VALUES (?1, CURRENT_TIMESTAMP)
                 ON CONFLICT(path) DO UPDATE SET last_used_at = excluded.last_used_at",
                [canonical.to_string_lossy().as_ref()],
            )
            .map(|_| ())
            .map_err(repository_error)
    }

    fn known_directories(&self) -> Result<Vec<std::path::PathBuf>, RepositoryError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare("SELECT path FROM output_directories ORDER BY path")
            .map_err(repository_error)?;
        statement
            .query_map([], |row| {
                row.get::<_, String>(0).map(std::path::PathBuf::from)
            })
            .map_err(repository_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(repository_error)
    }

    fn prune_except(&self, directories: &[std::path::PathBuf]) -> Result<u64, RepositoryError> {
        let canonical = directories
            .iter()
            .filter_map(|directory| std::fs::canonicalize(directory).ok())
            .map(|directory| directory.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        let connection = self.connection()?;
        let deleted = if canonical.is_empty() {
            connection
                .execute("DELETE FROM output_directories", [])
                .map_err(repository_error)?
        } else {
            let placeholders = std::iter::repeat_n("?", canonical.len())
                .collect::<Vec<_>>()
                .join(",");
            connection
                .execute(
                    &format!("DELETE FROM output_directories WHERE path NOT IN ({placeholders})"),
                    rusqlite::params_from_iter(canonical.iter()),
                )
                .map_err(repository_error)?
        };
        Ok(u64::try_from(deleted).unwrap_or(u64::MAX))
    }
}

impl RunHistoryRepository for SqliteRepository {
    fn insert(&self, run: &RunRecord) -> Result<(), RepositoryError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction().map_err(repository_error)?;
        write_run(&transaction, run, false)?;
        transaction.commit().map_err(repository_error)
    }

    fn update(&self, run: &RunRecord) -> Result<(), RepositoryError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction().map_err(repository_error)?;
        write_run(&transaction, run, true)?;
        transaction.commit().map_err(repository_error)
    }

    fn commit_terminal_run(&self, commit: &TerminalRunCommit) -> Result<(), RepositoryError> {
        let finished_at = commit
            .run
            .finished_at
            .ok_or_else(|| RepositoryError::new("terminal run requires finished_at"))?;
        let summary = commit
            .run
            .summary
            .as_ref()
            .ok_or_else(|| RepositoryError::new("terminal run requires summary"))?;
        let artifact_json = summary
            .artifact
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(repository_error)?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction().map_err(repository_error)?;
        write_run(&transaction, &commit.run, true)?;
        transaction
            .execute(
                "INSERT INTO action_operational_state
                 (folder_id, action_id, latest_run_id, latest_outcome, latest_finished_at,
                  last_successful_artifact_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(folder_id, action_id) DO UPDATE SET
                   latest_run_id = excluded.latest_run_id,
                   latest_outcome = excluded.latest_outcome,
                   latest_finished_at = excluded.latest_finished_at,
                   last_successful_artifact_json = COALESCE(
                     excluded.last_successful_artifact_json,
                     action_operational_state.last_successful_artifact_json
                   )",
                params![
                    commit.run.folder_id.to_string(),
                    commit.run.action_id.to_string(),
                    commit.run.run_id.to_string(),
                    enum_text(&summary.outcome)?,
                    finished_at.to_string(),
                    artifact_json,
                ],
            )
            .map_err(repository_error)?;
        if let Some(checkpoint) = &commit.checkpoint {
            transaction
                .execute(
                    "INSERT INTO action_checkpoints
                     (folder_id, action_id, algorithm_version, source_fingerprint,
                      effective_profile_hash, source_summary_json, run_id, completed_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                     ON CONFLICT(folder_id, action_id) DO UPDATE SET
                       algorithm_version = excluded.algorithm_version,
                       source_fingerprint = excluded.source_fingerprint,
                       effective_profile_hash = excluded.effective_profile_hash,
                       source_summary_json = excluded.source_summary_json,
                       run_id = excluded.run_id,
                       completed_at = excluded.completed_at",
                    params![
                        checkpoint.folder_id.to_string(),
                        checkpoint.action_id.to_string(),
                        checkpoint.algorithm_version,
                        checkpoint.source_fingerprint,
                        checkpoint.effective_profile_hash,
                        serde_json::to_string(&checkpoint.source_summary)
                            .map_err(repository_error)?,
                        checkpoint.run_id.to_string(),
                        checkpoint.completed_at.to_string(),
                    ],
                )
                .map_err(repository_error)?;
        }
        transaction.commit().map_err(repository_error)
    }

    fn get(&self, run_id: RunId) -> Result<Option<RunRecord>, RepositoryError> {
        self.connection()?
            .query_row(
                "SELECT run_id, folder_id, action_id, state, started_at, finished_at, snapshot_json, summary_json
                 FROM runs WHERE run_id = ?1",
                [run_id.to_string()],
                decode_run_row,
            )
            .optional()
            .map_err(repository_error)
    }

    fn page_filtered(
        &self,
        page: PageRequest,
        folder_id: Option<FolderId>,
        action_id: Option<ActionId>,
    ) -> Result<Vec<RunRecord>, RepositoryError> {
        let connection = self.connection()?;
        let offset = sql_integer(page.offset, "page offset")?;
        let limit = page.limit.clamp(1, 1000);
        let select = "SELECT run_id, folder_id, action_id, state, started_at, finished_at, snapshot_json, summary_json FROM runs";
        match (folder_id, action_id) {
            (Some(folder_id), Some(action_id)) => {
                let mut statement = connection
                    .prepare(&format!(
                        "{select} WHERE folder_id = ?1 AND action_id = ?2 ORDER BY started_at DESC, run_id DESC LIMIT ?3 OFFSET ?4"
                    ))
                    .map_err(repository_error)?;
                statement
                    .query_map(
                        params![folder_id.to_string(), action_id.to_string(), limit, offset],
                        decode_run_row,
                    )
                    .map_err(repository_error)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(repository_error)
            }
            (Some(folder_id), None) => {
                let mut statement = connection
                    .prepare(&format!(
                        "{select} WHERE folder_id = ?1 ORDER BY started_at DESC, run_id DESC LIMIT ?2 OFFSET ?3"
                    ))
                    .map_err(repository_error)?;
                statement
                    .query_map(
                        params![folder_id.to_string(), limit, offset],
                        decode_run_row,
                    )
                    .map_err(repository_error)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(repository_error)
            }
            (None, Some(action_id)) => {
                let mut statement = connection
                    .prepare(&format!(
                        "{select} WHERE action_id = ?1 ORDER BY started_at DESC, run_id DESC LIMIT ?2 OFFSET ?3"
                    ))
                    .map_err(repository_error)?;
                statement
                    .query_map(
                        params![action_id.to_string(), limit, offset],
                        decode_run_row,
                    )
                    .map_err(repository_error)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(repository_error)
            }
            (None, None) => {
                let mut statement = connection
                    .prepare(&format!(
                        "{select} ORDER BY started_at DESC, run_id DESC LIMIT ?1 OFFSET ?2"
                    ))
                    .map_err(repository_error)?;
                statement
                    .query_map(params![limit, offset], decode_run_row)
                    .map_err(repository_error)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(repository_error)
            }
        }
    }

    fn non_terminal_for_folder(
        &self,
        folder_id: FolderId,
    ) -> Result<Vec<RunRecord>, RepositoryError> {
        self.query_non_terminal("folder_id = ?1", &[folder_id.to_string()])
    }

    fn non_terminal_for_action(
        &self,
        folder_id: FolderId,
        action_id: ActionId,
    ) -> Result<Vec<RunRecord>, RepositoryError> {
        self.query_non_terminal(
            "folder_id = ?1 AND action_id = ?2",
            &[folder_id.to_string(), action_id.to_string()],
        )
    }

    fn mark_unfinished_interrupted(&self, at: Timestamp) -> Result<u64, RepositoryError> {
        self.connection()?
            .execute(
                "UPDATE runs SET state = 'interrupted', finished_at = ?1
                 WHERE state IN ('queued','planning','running','paused','stopping')",
                [at.to_string()],
            )
            .map(|count| count as u64)
            .map_err(repository_error)
    }

    fn apply_retention(
        &self,
        now: Timestamp,
        max_age_days: u32,
        max_entries: u32,
        unlimited: bool,
    ) -> Result<u64, RepositoryError> {
        if unlimited {
            return Ok(0);
        }
        let mut connection = self.connection()?;
        let transaction = connection.transaction().map_err(repository_error)?;
        let age_modifier = format!("-{max_age_days} days");
        let age_deleted = transaction
            .execute(
                "DELETE FROM runs WHERE julianday(started_at) <
                 julianday(?1, ?2)",
                params![now.to_string(), age_modifier],
            )
            .map_err(repository_error)?;
        let count_deleted = transaction
            .execute(
                "DELETE FROM runs WHERE run_id IN (
                   SELECT run_id FROM runs ORDER BY started_at DESC, run_id DESC
                   LIMIT -1 OFFSET ?1
                 )",
                [max_entries],
            )
            .map_err(repository_error)?;
        transaction.commit().map_err(repository_error)?;
        Ok((age_deleted + count_deleted) as u64)
    }

    fn operational_states(
        &self,
        folder_ids: &[FolderId],
    ) -> Result<Vec<ActionOperationalState>, RepositoryError> {
        if folder_ids.is_empty() {
            return Ok(Vec::new());
        }
        let requested = folder_ids
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        let placeholders = std::iter::repeat_n("?", requested.len())
            .collect::<Vec<_>>()
            .join(",");
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(&format!(
                "SELECT folder_id, action_id, latest_run_id, latest_outcome,
                        latest_finished_at, last_successful_artifact_json
                 FROM action_operational_state
                 WHERE folder_id IN ({placeholders})
                 ORDER BY folder_id, action_id",
            ))
            .map_err(repository_error)?;
        statement
            .query_map(rusqlite::params_from_iter(requested.iter()), |row| {
                Ok(ActionOperationalState {
                    folder_id: parse_field(row.get::<_, String>(0)?)?,
                    action_id: parse_field(row.get::<_, String>(1)?)?,
                    latest_run_id: parse_field(row.get::<_, String>(2)?)?,
                    latest_outcome: parse_enum(row.get::<_, String>(3)?)?,
                    latest_finished_at: parse_field(row.get::<_, String>(4)?)?,
                    last_successful_artifact: row
                        .get::<_, Option<String>>(5)?
                        .map(parse_json)
                        .transpose()?,
                })
            })
            .map_err(repository_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(repository_error)
    }

    fn checkpoints(
        &self,
        folder_ids: &[FolderId],
    ) -> Result<Vec<ActionCheckpoint>, RepositoryError> {
        if folder_ids.is_empty() {
            return Ok(Vec::new());
        }
        let requested = folder_ids
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        let placeholders = std::iter::repeat_n("?", requested.len())
            .collect::<Vec<_>>()
            .join(",");
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(&format!(
                "SELECT folder_id, action_id, algorithm_version, source_fingerprint,
                        effective_profile_hash, source_summary_json, run_id, completed_at
                 FROM action_checkpoints
                 WHERE folder_id IN ({placeholders})
                 ORDER BY folder_id, action_id",
            ))
            .map_err(repository_error)?;
        statement
            .query_map(rusqlite::params_from_iter(requested.iter()), |row| {
                let algorithm_version = row.get::<_, i64>(2)?;
                Ok(ActionCheckpoint {
                    folder_id: parse_field(row.get::<_, String>(0)?)?,
                    action_id: parse_field(row.get::<_, String>(1)?)?,
                    algorithm_version: u16::try_from(algorithm_version)
                        .map_err(sql_decode_error)?,
                    source_fingerprint: row.get(3)?,
                    effective_profile_hash: row.get(4)?,
                    source_summary: parse_json(row.get::<_, String>(5)?)?,
                    run_id: parse_field(row.get::<_, String>(6)?)?,
                    completed_at: parse_field(row.get::<_, String>(7)?)?,
                })
            })
            .map_err(repository_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(repository_error)
    }

    fn forget_action(
        &self,
        folder_id: FolderId,
        action_id: foldry_application::ActionId,
    ) -> Result<(), RepositoryError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction().map_err(repository_error)?;
        let parameters = params![folder_id.to_string(), action_id.to_string()];
        transaction
            .execute(
                "DELETE FROM action_checkpoints WHERE folder_id = ?1 AND action_id = ?2",
                parameters,
            )
            .map_err(repository_error)?;
        transaction
            .execute(
                "DELETE FROM action_operational_state WHERE folder_id = ?1 AND action_id = ?2",
                params![folder_id.to_string(), action_id.to_string()],
            )
            .map_err(repository_error)?;
        transaction.commit().map_err(repository_error)
    }
}

impl SqliteRepository {
    fn query_non_terminal(
        &self,
        predicate: &str,
        values: &[String],
    ) -> Result<Vec<RunRecord>, RepositoryError> {
        let connection = self.connection()?;
        let sql = format!(
            "SELECT run_id, folder_id, action_id, state, started_at, finished_at, snapshot_json, summary_json
             FROM runs
             WHERE {predicate}
               AND state IN ('queued','planning','running','paused','stopping')
             ORDER BY started_at, run_id"
        );
        let mut statement = connection.prepare(&sql).map_err(repository_error)?;
        let parameters = rusqlite::params_from_iter(values.iter());
        statement
            .query_map(parameters, decode_run_row)
            .map_err(repository_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(repository_error)
    }
}

impl LogRepository for SqliteRepository {
    fn append(&self, record: &LogRecord) -> Result<(), RepositoryError> {
        let sequence = sql_integer(record.sequence, "log sequence")?;
        self.connection()?
            .execute(
                "INSERT INTO logs(run_id, sequence, occurred_at, level, message, path)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    record.run_id.to_string(),
                    sequence,
                    record.occurred_at.to_string(),
                    enum_text(&record.level)?,
                    record.message,
                    record.path
                ],
            )
            .map(|_| ())
            .map_err(repository_error)
    }

    fn page(&self, run_id: RunId, page: PageRequest) -> Result<Vec<LogRecord>, RepositoryError> {
        let connection = self.connection()?;
        let offset = sql_integer(page.offset, "page offset")?;
        let mut statement = connection
            .prepare(
                "SELECT run_id, sequence, occurred_at, level, message, path
                 FROM logs WHERE run_id = ?1 ORDER BY sequence LIMIT ?2 OFFSET ?3",
            )
            .map_err(repository_error)?;
        statement
            .query_map(
                params![run_id.to_string(), page.limit.clamp(1, 1000), offset],
                |row| {
                    let sequence = row.get::<_, i64>(1)?;
                    Ok(LogRecord {
                        run_id: parse_field(row.get::<_, String>(0)?)?,
                        sequence: u64::try_from(sequence).map_err(sql_decode_error)?,
                        occurred_at: parse_field(row.get::<_, String>(2)?)?,
                        level: parse_enum(row.get::<_, String>(3)?)?,
                        message: row.get(4)?,
                        path: row.get(5)?,
                    })
                },
            )
            .map_err(repository_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(repository_error)
    }

    fn apply_retention(
        &self,
        now: Timestamp,
        max_age_days: u32,
        max_runs: u32,
        unlimited: bool,
    ) -> Result<u64, RepositoryError> {
        if unlimited {
            return Ok(0);
        }
        let mut connection = self.connection()?;
        let transaction = connection.transaction().map_err(repository_error)?;
        let age_modifier = format!("-{max_age_days} days");
        let age_deleted = transaction
            .execute(
                "DELETE FROM logs WHERE julianday(occurred_at) < julianday(?1, ?2)",
                params![now.to_string(), age_modifier],
            )
            .map_err(repository_error)?;
        let count_deleted = transaction
            .execute(
                "DELETE FROM logs WHERE run_id NOT IN (
                   SELECT run_id FROM logs GROUP BY run_id
                   ORDER BY MAX(occurred_at) DESC, run_id DESC LIMIT ?1
                 )",
                [max_runs],
            )
            .map_err(repository_error)?;
        transaction.commit().map_err(repository_error)?;
        Ok((age_deleted + count_deleted) as u64)
    }
}

fn write_run(
    transaction: &Transaction<'_>,
    run: &RunRecord,
    replace: bool,
) -> Result<(), RepositoryError> {
    let statement = if replace {
        "INSERT INTO runs
         (run_id, folder_id, action_id, state, started_at, finished_at, snapshot_json, summary_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(run_id) DO UPDATE SET
           folder_id = excluded.folder_id,
           action_id = excluded.action_id,
           state = excluded.state,
           started_at = excluded.started_at,
           finished_at = excluded.finished_at,
           snapshot_json = excluded.snapshot_json,
           summary_json = excluded.summary_json"
    } else {
        "INSERT INTO runs
         (run_id, folder_id, action_id, state, started_at, finished_at, snapshot_json, summary_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"
    };
    transaction
        .execute(
            statement,
            params![
                run.run_id.to_string(),
                run.folder_id.to_string(),
                run.action_id.to_string(),
                enum_text(&run.state)?,
                run.started_at.to_string(),
                run.finished_at.map(|time| time.to_string()),
                serde_json::to_string(&run.snapshot).map_err(repository_error)?,
                run.summary
                    .as_ref()
                    .map(serde_json::to_string)
                    .transpose()
                    .map_err(repository_error)?
            ],
        )
        .map_err(repository_error)?;
    transaction
        .execute(
            "DELETE FROM run_warnings WHERE run_id = ?1",
            [run.run_id.to_string()],
        )
        .map_err(repository_error)?;
    transaction
        .execute(
            "DELETE FROM run_errors WHERE run_id = ?1",
            [run.run_id.to_string()],
        )
        .map_err(repository_error)?;
    if let Some(summary) = &run.summary {
        for (ordinal, warning) in summary.warnings.iter().enumerate() {
            let ordinal = i64::try_from(ordinal)
                .map_err(|_| RepositoryError::new("warning ordinal exceeds SQLite INTEGER"))?;
            transaction
                .execute(
                    "INSERT INTO run_warnings(run_id, ordinal, payload_json) VALUES (?1, ?2, ?3)",
                    params![
                        run.run_id.to_string(),
                        ordinal,
                        serde_json::to_string(warning).map_err(repository_error)?
                    ],
                )
                .map_err(repository_error)?;
        }
        if let Some(error) = &summary.error {
            transaction
                .execute(
                    "INSERT INTO run_errors(run_id, payload_json) VALUES (?1, ?2)",
                    params![
                        run.run_id.to_string(),
                        serde_json::to_string(error).map_err(repository_error)?
                    ],
                )
                .map_err(repository_error)?;
        }
    }
    Ok(())
}

fn decode_run_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RunRecord> {
    Ok(RunRecord {
        run_id: parse_field(row.get::<_, String>(0)?)?,
        folder_id: parse_field(row.get::<_, String>(1)?)?,
        action_id: parse_field(row.get::<_, String>(2)?)?,
        state: parse_enum(row.get::<_, String>(3)?)?,
        started_at: parse_field(row.get::<_, String>(4)?)?,
        finished_at: row
            .get::<_, Option<String>>(5)?
            .map(parse_field)
            .transpose()?,
        snapshot: parse_json(row.get::<_, String>(6)?)?,
        summary: row
            .get::<_, Option<String>>(7)?
            .map(parse_json)
            .transpose()?,
    })
}

fn enum_text<T: serde::Serialize>(value: &T) -> Result<String, RepositoryError> {
    serde_json::to_value(value)
        .map_err(repository_error)?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| RepositoryError::new("enum did not serialize as text"))
}

fn parse_enum<T: serde::de::DeserializeOwned>(value: String) -> rusqlite::Result<T> {
    serde_json::from_value(serde_json::Value::String(value)).map_err(sql_decode_error)
}

fn parse_json<T: serde::de::DeserializeOwned>(value: String) -> rusqlite::Result<T> {
    serde_json::from_str(&value).map_err(sql_decode_error)
}

fn parse_field<T: std::str::FromStr>(value: String) -> rusqlite::Result<T>
where
    T::Err: std::fmt::Display,
{
    value.parse().map_err(sql_decode_error)
}

fn sql_decode_error(error: impl std::fmt::Display) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(RepositoryError::new(error.to_string())),
    )
}

fn repository_error(error: impl std::fmt::Display) -> RepositoryError {
    RepositoryError::new(error.to_string())
}

fn sql_integer(value: u64, field: &str) -> Result<i64, RepositoryError> {
    i64::try_from(value)
        .map_err(|_| RepositoryError::new(format!("{field} exceeds SQLite INTEGER")))
}
