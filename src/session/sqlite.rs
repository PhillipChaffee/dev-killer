use anyhow::{Context, Result};
use async_trait::async_trait;
use rusqlite::Connection;
use std::path::PathBuf;
use tokio::task;
use tracing::debug;

use super::{SessionState, Storage};

/// SQLite-based session storage
pub struct SqliteStorage {
    /// Path to the SQLite database file
    db_path: PathBuf,
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
        let conn = Connection::open(&self.db_path)
            .with_context(|| format!("failed to open database: {}", self.db_path.display()))?;

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
            let conn = Connection::open(&db_path)?;

            // Serialize full session data as JSON
            let data = serde_json::to_string(&session)?;

            conn.execute(
                "INSERT OR REPLACE INTO sessions (id, task, status, phase, working_dir, created_at, updated_at, error, data)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
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
            let conn = Connection::open(&db_path)?;

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
            let conn = Connection::open(&db_path)?;

            let mut stmt = conn.prepare(
                "SELECT id, task, status, phase, working_dir, created_at, updated_at, error
                 FROM sessions
                 ORDER BY updated_at DESC",
            )?;

            let sessions = stmt
                .query_map([], |row| {
                    Ok(SessionSummary {
                        id: row.get(0)?,
                        task: row.get(1)?,
                        status: row.get(2)?,
                        phase: row.get(3)?,
                        working_dir: row.get(4)?,
                        created_at: row.get(5)?,
                        updated_at: row.get(6)?,
                        error: row.get(7)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;

            Ok(sessions)
        })
        .await
        .context("spawn_blocking failed")?
    }

    async fn delete(&self, id: &str) -> Result<()> {
        let id = id.to_string();
        let db_path = self.db_path.clone();

        task::spawn_blocking(move || {
            let conn = Connection::open(&db_path)?;
            conn.execute("DELETE FROM sessions WHERE id = ?1", [&id])?;
            debug!(id = %id, "deleted session");
            Ok::<_, anyhow::Error>(())
        })
        .await
        .context("spawn_blocking failed")??;

        Ok(())
    }
}

/// Summary of a session for listing (without full message history)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSummary {
    pub id: String,
    pub task: String,
    pub status: String,
    pub phase: String,
    pub working_dir: String,
    pub created_at: String,
    pub updated_at: String,
    pub error: Option<String>,
}

impl std::fmt::Display for SessionSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Use chars() to handle UTF-8 safely
        let task_preview: String = if self.task.chars().count() > 50 {
            self.task.chars().take(47).collect::<String>() + "..."
        } else {
            self.task.clone()
        };

        // Session IDs are UUIDs so safe to slice, but use chars for safety
        let id_short: String = self.id.chars().take(8).collect();

        write!(
            f,
            "{} | {} | {} | {}",
            id_short, self.status, self.phase, task_preview
        )
    }
}
