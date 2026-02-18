use serde::{Deserialize, Serialize};

use crate::llm::Message;

/// Session state for persistence
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionState {
    /// Unique session identifier
    pub id: String,

    /// Current task being worked on
    pub task: Option<String>,

    /// Conversation history
    pub messages: Vec<Message>,

    /// Current status
    pub status: SessionStatus,
}

/// Status of a session
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionStatus {
    #[default]
    Pending,
    InProgress,
    Completed,
    Failed,
}
