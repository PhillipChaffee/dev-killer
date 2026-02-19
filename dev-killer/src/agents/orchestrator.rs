use anyhow::Result;
use async_trait::async_trait;
use tracing::info;

use super::Agent;
use crate::event::EventSender;
use crate::llm::LlmProvider;
use crate::pipeline::{Pipeline, execute_pipeline};
use crate::tools::ToolRegistry;

/// Orchestrator agent that coordinates multiple specialized agents.
///
/// This is now a thin wrapper around [`execute_pipeline`] with the default pipeline
/// (plan -> code -> test -> review). For custom pipelines, use [`Pipeline`] directly
/// via [`DevKiller::builder().pipeline()`](crate::DevKillerBuilder::pipeline).
pub struct OrchestratorAgent {
    pipeline: Pipeline,
}

impl OrchestratorAgent {
    pub fn new() -> Self {
        Self {
            pipeline: Pipeline::default(),
        }
    }
}

impl Default for OrchestratorAgent {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Agent for OrchestratorAgent {
    fn system_prompt(&self) -> String {
        // Orchestrator doesn't use LLM directly, it coordinates other agents
        String::new()
    }

    async fn run(
        &self,
        task: &str,
        provider: &dyn LlmProvider,
        tools: &ToolRegistry,
        events: &EventSender,
    ) -> Result<String> {
        info!(task, "orchestrator starting (delegating to pipeline)");
        execute_pipeline(&self.pipeline, task, provider, tools, events).await
    }
}
