use anyhow::{Context, Result};
use tracing::{error, info};

use crate::agents::Agent;
use crate::llm::LlmProvider;
use crate::session::{SessionPhase, SessionState, SessionStatus, Storage};
use crate::tools::ToolRegistry;

/// Executor for running agents with optional session persistence
pub struct Executor {
    tools: ToolRegistry,
    storage: Option<Box<dyn Storage>>,
}

impl Executor {
    /// Create a new executor with a tool registry
    pub fn new(tools: ToolRegistry) -> Self {
        Self {
            tools,
            storage: None,
        }
    }

    /// Create an executor with session storage
    pub fn with_storage(tools: ToolRegistry, storage: Box<dyn Storage>) -> Self {
        Self {
            tools,
            storage: Some(storage),
        }
    }

    /// Run an agent with a task (no session tracking)
    pub async fn run(
        &self,
        agent: &dyn Agent,
        task: &str,
        provider: &dyn LlmProvider,
    ) -> Result<String> {
        info!(task, "starting agent execution");
        let result = agent.run(task, provider, &self.tools).await?;
        info!("agent execution completed");
        Ok(result)
    }

    /// Run an agent with session tracking
    pub async fn run_with_session(
        &self,
        agent: &dyn Agent,
        session: &mut SessionState,
        provider: &dyn LlmProvider,
    ) -> Result<String> {
        let storage = self
            .storage
            .as_ref()
            .context("storage not configured for session tracking")?;

        info!(session_id = %session.id, task = %session.task, "starting session");

        // Mark session as in progress
        session.set_status(SessionStatus::InProgress);
        session.set_phase(SessionPhase::Planning);
        storage.save(session).await?;

        // Run the agent
        match agent.run(&session.task, provider, &self.tools).await {
            Ok(output) => {
                session.complete();
                storage.save(session).await?;
                info!(session_id = %session.id, "session completed successfully");
                Ok(output)
            }
            Err(e) => {
                session.set_error(e.to_string());
                storage.save(session).await?;
                error!(session_id = %session.id, error = %e, "session failed");
                Err(e)
            }
        }
    }

    /// Resume a session from storage
    pub async fn resume_session(
        &self,
        session_id: &str,
        agent: &dyn Agent,
        provider: &dyn LlmProvider,
    ) -> Result<String> {
        let storage = self
            .storage
            .as_ref()
            .context("storage not configured for session tracking")?;

        let mut session = storage
            .load(session_id)
            .await?
            .context(format!("session not found: {}", session_id))?;

        if !session.can_resume() {
            anyhow::bail!("session cannot be resumed (status: {})", session.status);
        }

        info!(
            session_id = %session.id,
            task = %session.task,
            phase = %session.phase,
            "resuming session"
        );

        self.run_with_session(agent, &mut session, provider).await
    }

    /// Get storage reference for direct operations
    pub fn storage(&self) -> Option<&dyn Storage> {
        self.storage.as_ref().map(|s| s.as_ref())
    }
}
