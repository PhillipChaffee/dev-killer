mod coder;
mod orchestrator;
mod planner;
mod reviewer;
pub(crate) mod runner;
mod tester;

pub use coder::CoderAgent;
pub use orchestrator::OrchestratorAgent;
pub use planner::PlannerAgent;
pub use reviewer::ReviewerAgent;
pub use tester::TesterAgent;

use anyhow::Result;
use async_trait::async_trait;

use crate::event::EventSender;
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
        events: &EventSender,
    ) -> Result<String>;
}
