use anyhow::Result;
use tracing::info;

use crate::agents::Agent;
use crate::llm::LlmProvider;
use crate::tools::ToolRegistry;

/// Executor for running agents
pub struct Executor {
    tools: ToolRegistry,
}

impl Executor {
    /// Create a new executor with a tool registry
    pub fn new(tools: ToolRegistry) -> Self {
        Self { tools }
    }

    /// Run an agent with a task
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
}
