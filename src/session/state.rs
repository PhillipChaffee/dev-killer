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
