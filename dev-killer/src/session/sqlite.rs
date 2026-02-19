use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use async_trait::async_trait;
use rusqlite::Connection;
use tokio::task;
use tracing::{debug, warn};

use super::state::SessionSummary;
use super::{SessionPhase, SessionState, SessionStatus, Storage};

/// SQLite-based session storage
pub struct SqliteStorage {
    /// Path to the SQLite database file
    db_path: PathBuf,
}

/// Open a SQLite connection with standard pragmas (busy_timeout).
fn open_connection(db_path: &Path) -> Result<Connection> {
    let conn = Connection::open(db_path)
        .with_context(|| format!("failed to open database: {}", db_path.display()))?;
    conn.execute_batch("PRAGMA busy_timeout=5000;")
        .context("failed to set busy_timeout")?;
    Ok(conn)
}

impl SqliteStorage {
    /// Create a new SQLite storage at the given path
    pub fn new(db_path: impl Into<PathBuf>) -> Result<Self> {
        let db_path = db_path.into();

        // Create parent directories if they don't exist
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory: {}", parent.display()))?;
        }

        let storage = Self { db_path };
        storage.init_schema()?;

        Ok(storage)
    }

    /// Create storage using default location (~/.dev-killer/sessions.db)
    pub fn default_location() -> Result<Self> {
        let home = std::env::var("HOME").context("HOME environment variable not set")?;
        let db_path = PathBuf::from(home).join(".dev-killer").join("sessions.db");
        Self::new(db_path)
    }

    /// Initialize the database schema
    fn init_schema(&self) -> Result<()> {
        let conn = open_connection(&self.db_path)?;

        // Enable WAL mode for better concurrent read/write performance
        conn.execute_batch("PRAGMA journal_mode=WAL;")
            .context("failed to set WAL mode")?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                task TEXT NOT NULL,
                status TEXT NOT NULL,
                phase TEXT NOT NULL,
                working_dir TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                error TEXT,
                data TEXT NOT NULL
            )",
            [],
        )
        .context("failed to create sessions table")?;

        // Index for listing sessions by status
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_sessions_status ON sessions(status)",
            [],
        )
        .context("failed to create status index")?;

        // Index for listing sessions by updated_at
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_sessions_updated ON sessions(updated_at)",
            [],
        )
        .context("failed to create updated_at index")?;

        // Migration: add portability columns (nullable, safe to run multiple times)
        let columns: Vec<String> = conn
            .prepare("PRAGMA table_info(sessions)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;

        if !columns.iter().any(|c| c == "project_id") {
            conn.execute("ALTER TABLE sessions ADD COLUMN project_id TEXT", [])
                .context("failed to add project_id column")?;
            debug!("migrated: added project_id column");
        }
        if !columns.iter().any(|c| c == "project_relative_dir") {
            conn.execute(
                "ALTER TABLE sessions ADD COLUMN project_relative_dir TEXT",
                [],
            )
            .context("failed to add project_relative_dir column")?;
            debug!("migrated: added project_relative_dir column");
        }

        debug!(path = %self.db_path.display(), "initialized SQLite storage");

        Ok(())
    }
}

#[async_trait]
impl Storage for SqliteStorage {
    async fn save(&self, session: &SessionState) -> Result<()> {
        let session = session.clone();
        let db_path = self.db_path.clone();

        task::spawn_blocking(move || {
            let conn = open_connection(&db_path)?;

            // Serialize full session data as JSON
            let data = serde_json::to_string(&session)?;

            conn.execute(
                "INSERT OR REPLACE INTO sessions (id, task, status, phase, working_dir, created_at, updated_at, error, data, project_id, project_relative_dir)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                rusqlite::params![
                    session.id,
                    session.task,
                    session.status.to_string(),
                    session.phase.to_string(),
                    session.working_dir,
                    session.created_at.to_rfc3339(),
                    session.updated_at.to_rfc3339(),
                    session.error,
                    data,
                    session.project_id,
                    session.project_relative_dir,
                ],
            )?;

            debug!(id = %session.id, "saved session");

            Ok::<_, anyhow::Error>(())
        })
        .await
        .context("spawn_blocking failed")??;

        Ok(())
    }

    async fn load(&self, id: &str) -> Result<Option<SessionState>> {
        let id = id.to_string();
        let db_path = self.db_path.clone();

        task::spawn_blocking(move || {
            let conn = open_connection(&db_path)?;

            let mut stmt = conn.prepare("SELECT data FROM sessions WHERE id = ?1")?;

            let result = stmt.query_row([&id], |row| {
                let data: String = row.get(0)?;
                Ok(data)
            });

            match result {
                Ok(data) => {
                    let session: SessionState = serde_json::from_str(&data)?;
                    debug!(id = %session.id, "loaded session");
                    Ok(Some(session))
                }
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(e.into()),
            }
        })
        .await
        .context("spawn_blocking failed")?
    }

    async fn list(&self) -> Result<Vec<SessionSummary>> {
        let db_path = self.db_path.clone();

        task::spawn_blocking(move || {
            let conn = open_connection(&db_path)?;

            let mut stmt = conn.prepare(
                "SELECT id, task, status, phase, working_dir, created_at, updated_at, error
                 FROM sessions
                 ORDER BY updated_at DESC",
            )?;

            let sessions = stmt
                .query_map([], |row| {
                    let status_str: String = row.get(2)?;
                    let phase_str: String = row.get(3)?;
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        status_str,
                        phase_str,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, Option<String>>(7)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;

            let mut result = Vec::with_capacity(sessions.len());
            for (id, task, status_str, phase_str, working_dir, created_at, updated_at, error) in
                sessions
            {
                let status = status_str.parse::<SessionStatus>().unwrap_or_else(|e| {
                    warn!(id = %id, status = %status_str, error = %e, "invalid status in database, defaulting to Pending");
                    SessionStatus::Pending
                });
                let phase = phase_str.parse::<SessionPhase>().unwrap_or_else(|e| {
                    warn!(id = %id, phase = %phase_str, error = %e, "invalid phase in database, defaulting to NotStarted");
                    SessionPhase::NotStarted
                });
                result.push(SessionSummary {
                    id,
                    task,
                    status,
                    phase,
                    working_dir,
                    created_at,
                    updated_at,
                    error,
                });
            }

            Ok(result)
        })
        .await
        .context("spawn_blocking failed")?
    }

    async fn delete(&self, id: &str) -> Result<()> {
        let id = id.to_string();
        let db_path = self.db_path.clone();

        task::spawn_blocking(move || {
            let conn = open_connection(&db_path)?;
            conn.execute("DELETE FROM sessions WHERE id = ?1", [&id])?;
            let changes = conn.changes();
            if changes == 0 {
                anyhow::bail!("session '{}' not found", id);
            }
            debug!(id = %id, "deleted session");
            Ok::<_, anyhow::Error>(())
        })
        .await
        .context("spawn_blocking failed")??;

        Ok(())
    }
}
