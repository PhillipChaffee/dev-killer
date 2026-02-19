use anyhow::Result;
use async_trait::async_trait;

use super::SessionState;
use super::state::SessionSummary;

/// Storage backend for sessions
#[async_trait]
pub trait Storage: Send + Sync {
    /// Save a session
    async fn save(&self, session: &SessionState) -> Result<()>;

    /// Load a session by ID
    async fn load(&self, id: &str) -> Result<Option<SessionState>>;

    /// List all sessions (returns summaries, not full data)
    async fn list(&self) -> Result<Vec<SessionSummary>>;

    /// Delete a session
    async fn delete(&self, id: &str) -> Result<()>;
}
