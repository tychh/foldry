use std::{
    path::Path,
    sync::{Mutex, MutexGuard},
    time::Duration,
};

use foldry_application::{
    LogRecord, LogRepository, PageRequest, RepositoryError, RunHistoryRepository, RunId, RunRecord,
};
use jiff::Timestamp;
use rusqlite::{Connection, OptionalExtension, Transaction, params};

const SCHEMA_VERSION: i64 = 1;

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
    let version = connection
        .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
        .map_err(repository_error)?;
    if version > SCHEMA_VERSION {
        return Err(RepositoryError::new(format!(
            "database schema {version} is newer than supported schema {SCHEMA_VERSION}"
        )));
    }
    if version == SCHEMA_VERSION {
        return Ok(());
    }
    let transaction = connection
        .unchecked_transaction()
        .map_err(repository_error)?;
    transaction
        .execute_batch(
            "
            CREATE TABLE runs (
                run_id TEXT PRIMARY KEY,
                task_id TEXT NOT NULL,
                state TEXT NOT NULL,
                started_at TEXT NOT NULL,
                finished_at TEXT,
                snapshot_json TEXT NOT NULL,
                summary_json TEXT
            );
            CREATE INDEX runs_started_at_idx ON runs(started_at DESC);
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
        .map_err(repository_error)?;
    transaction.commit().map_err(repository_error)
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

    fn get(&self, run_id: RunId) -> Result<Option<RunRecord>, RepositoryError> {
        self.connection()?
            .query_row(
                "SELECT run_id, task_id, state, started_at, finished_at, snapshot_json, summary_json
                 FROM runs WHERE run_id = ?1",
                [run_id.to_string()],
                decode_run_row,
            )
            .optional()
            .map_err(repository_error)
    }

    fn page(&self, page: PageRequest) -> Result<Vec<RunRecord>, RepositoryError> {
        let connection = self.connection()?;
        let offset = sql_integer(page.offset, "page offset")?;
        let mut statement = connection
            .prepare(
                "SELECT run_id, task_id, state, started_at, finished_at, snapshot_json, summary_json
                 FROM runs ORDER BY started_at DESC, run_id DESC LIMIT ?1 OFFSET ?2",
            )
            .map_err(repository_error)?;
        statement
            .query_map(params![page.limit.clamp(1, 1000), offset], decode_run_row)
            .map_err(repository_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(repository_error)
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
         (run_id, task_id, state, started_at, finished_at, snapshot_json, summary_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(run_id) DO UPDATE SET
           task_id = excluded.task_id,
           state = excluded.state,
           started_at = excluded.started_at,
           finished_at = excluded.finished_at,
           snapshot_json = excluded.snapshot_json,
           summary_json = excluded.summary_json"
    } else {
        "INSERT INTO runs
         (run_id, task_id, state, started_at, finished_at, snapshot_json, summary_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"
    };
    transaction
        .execute(
            statement,
            params![
                run.run_id.to_string(),
                run.task_id.to_string(),
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
        task_id: parse_field(row.get::<_, String>(1)?)?,
        state: parse_enum(row.get::<_, String>(2)?)?,
        started_at: parse_field(row.get::<_, String>(3)?)?,
        finished_at: row
            .get::<_, Option<String>>(4)?
            .map(parse_field)
            .transpose()?,
        snapshot: parse_json(row.get::<_, String>(5)?)?,
        summary: row
            .get::<_, Option<String>>(6)?
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
