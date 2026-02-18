mod coder;

pub use coder::CoderAgent;

use anyhow::Result;
use async_trait::async_trait;

use crate::llm::LlmProvider;
use crate::tools::ToolRegistry;

/// An agent that can perform tasks using LLM and tools
#[async_trait]
pub trait Agent: Send + Sync {
    /// The system prompt for this agent
    fn system_prompt(&self) -> String;

    /// Run the agent with a task
    async fn run(
        &self,
        task: &str,
        provider: &dyn LlmProvider,
        tools: &ToolRegistry,
    ) -> Result<String>;
}
