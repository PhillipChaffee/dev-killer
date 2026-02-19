use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use uuid::Uuid;

use crate::llm::Message;

/// Session state for persistence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    /// Unique session identifier
    pub id: String,

    /// Current task being worked on
    pub task: String,

    /// Conversation history
    pub messages: Vec<Message>,

    /// Current status
    pub status: SessionStatus,

    /// Current phase in the orchestration workflow
    pub phase: SessionPhase,

    /// When the session was created
    pub created_at: DateTime<Utc>,

    /// When the session was last updated
    pub updated_at: DateTime<Utc>,

    /// Working directory for the session
    pub working_dir: String,

    /// Any error message if the session failed
    pub error: Option<String>,

    /// Project identifier for cross-machine portability
    #[serde(default)]
    pub project_id: Option<String>,

    /// Relative directory within the project
    #[serde(default)]
    pub project_relative_dir: Option<String>,
}

impl SessionState {
    /// Create a new session for a task
    pub fn new(task: impl Into<String>, working_dir: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            task: task.into(),
            messages: Vec::new(),
            status: SessionStatus::Pending,
            phase: SessionPhase::NotStarted,
            created_at: now,
            updated_at: now,
            working_dir: working_dir.into(),
            error: None,
            project_id: None,
            project_relative_dir: None,
        }
    }

    /// Update the session status
    pub fn set_status(&mut self, status: SessionStatus) {
        self.status = status;
        self.updated_at = Utc::now();
    }

    /// Update the session phase
    pub fn set_phase(&mut self, phase: SessionPhase) {
        self.phase = phase;
        self.updated_at = Utc::now();
    }

    /// Add a message to the conversation history
    pub fn add_message(&mut self, message: Message) {
        self.messages.push(message);
        self.updated_at = Utc::now();
    }

    /// Set an error and mark as failed
    pub fn set_error(&mut self, error: impl Into<String>) {
        self.error = Some(error.into());
        self.status = SessionStatus::Failed;
        self.updated_at = Utc::now();
    }

    /// Mark the session as completed
    pub fn complete(&mut self) {
        self.status = SessionStatus::Completed;
        self.phase = SessionPhase::Completed;
        self.updated_at = Utc::now();
    }

    /// Check if the session can be resumed
    pub fn can_resume(&self) -> bool {
        matches!(
            self.status,
            SessionStatus::Pending | SessionStatus::InProgress | SessionStatus::Interrupted
        )
    }
}

impl Default for SessionState {
    fn default() -> Self {
        Self::new("", ".")
    }
}

/// Status of a session
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum SessionStatus {
    /// Session created but not started
    #[default]
    Pending,
    /// Session is currently running
    InProgress,
    /// Session completed successfully
    Completed,
    /// Session failed with an error
    Failed,
    /// Session was interrupted and can be resumed
    Interrupted,
}

/// Phase in the orchestration workflow
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum SessionPhase {
    /// Session not started
    #[default]
    NotStarted,
    /// Planning phase
    Planning,
    /// Implementation phase
    Implementing,
    /// Testing phase
    Testing,
    /// Review phase
    Reviewing,
    /// Session completed
    Completed,
}

impl std::fmt::Display for SessionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::InProgress => write!(f, "in_progress"),
            Self::Completed => write!(f, "completed"),
            Self::Failed => write!(f, "failed"),
            Self::Interrupted => write!(f, "interrupted"),
        }
    }
}

impl FromStr for SessionStatus {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pending" => Ok(Self::Pending),
            "in_progress" | "inprogress" => Ok(Self::InProgress),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "interrupted" => Ok(Self::Interrupted),
            _ => anyhow::bail!(
                "invalid session status '{}' (expected: pending, in_progress, completed, failed, interrupted)",
                s
            ),
        }
    }
}

impl std::fmt::Display for SessionPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotStarted => write!(f, "not_started"),
            Self::Planning => write!(f, "planning"),
            Self::Implementing => write!(f, "implementing"),
            Self::Testing => write!(f, "testing"),
            Self::Reviewing => write!(f, "reviewing"),
            Self::Completed => write!(f, "completed"),
        }
    }
}

impl FromStr for SessionPhase {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "not_started" | "notstarted" => Ok(Self::NotStarted),
            "planning" => Ok(Self::Planning),
            "implementing" => Ok(Self::Implementing),
            "testing" => Ok(Self::Testing),
            "reviewing" => Ok(Self::Reviewing),
            "completed" => Ok(Self::Completed),
            _ => anyhow::bail!(
                "invalid session phase '{}' (expected: not_started, planning, implementing, testing, reviewing, completed)",
                s
            ),
        }
    }
}

/// Summary of a session for listing (without full message history)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSummary {
    pub id: String,
    pub task: String,
    pub status: SessionStatus,
    pub phase: SessionPhase,
    pub working_dir: String,
    pub created_at: String,
    pub updated_at: String,
    pub error: Option<String>,
}

impl std::fmt::Display for SessionSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let task_preview: String = if self.task.chars().count() > 50 {
            self.task.chars().take(47).collect::<String>() + "..."
        } else {
            self.task.clone()
        };

        let id_short: String = self.id.chars().take(8).collect();

        write!(
            f,
            "{:<10} {:<12} {:<12} {}",
            id_short, self.status, self.phase, task_preview
        )
    }
}

/// A portable session for export/import across environments.
///
/// Strips absolute paths and carries enough context to recreate a session
/// on a different machine. Serialized as JSON for interchange.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortableSession {
    /// Schema version for forward compatibility
    pub version: u32,

    /// The original session ID
    pub original_id: String,

    /// The task being worked on
    pub task: String,

    /// Conversation history
    pub messages: Vec<Message>,

    /// Session status at export time
    pub status: SessionStatus,

    /// Session phase at export time
    pub phase: SessionPhase,

    /// Project identifier (for resolving working directory on import)
    pub project_id: Option<String>,

    /// Relative directory within the project
    pub project_relative_dir: Option<String>,

    /// When the session was originally created
    pub created_at: DateTime<Utc>,

    /// When the session was exported
    pub exported_at: DateTime<Utc>,

    /// Any error from the original session
    pub error: Option<String>,
}

impl PortableSession {
    /// Current schema version
    pub const CURRENT_VERSION: u32 = 1;

    /// Create a portable session from a full session state.
    pub fn from_session(session: &SessionState) -> Self {
        Self {
            version: Self::CURRENT_VERSION,
            original_id: session.id.clone(),
            task: session.task.clone(),
            messages: session.messages.clone(),
            status: session.status,
            phase: session.phase,
            project_id: session.project_id.clone(),
            project_relative_dir: session.project_relative_dir.clone(),
            created_at: session.created_at,
            exported_at: Utc::now(),
            error: session.error.clone(),
        }
    }

    /// Import this portable session into a new SessionState.
    ///
    /// Assigns a new session ID and resolves the working directory.
    /// If `working_dir` is provided, it's used directly; otherwise the current
    /// directory is used as a fallback.
    pub fn into_session(self, working_dir: Option<String>) -> SessionState {
        let working_dir = working_dir.unwrap_or_else(|| {
            std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| ".".to_string())
        });
        let now = Utc::now();

        SessionState {
            id: Uuid::new_v4().to_string(),
            task: self.task,
            messages: self.messages,
            // Imported sessions are always marked Interrupted so they can be resumed
            status: SessionStatus::Interrupted,
            phase: self.phase,
            created_at: self.created_at,
            updated_at: now,
            working_dir,
            error: self.error,
            project_id: self.project_id,
            project_relative_dir: self.project_relative_dir,
        }
    }
}
