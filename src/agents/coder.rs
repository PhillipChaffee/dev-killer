use anyhow::Result;
use async_trait::async_trait;
use tokio::time::{Duration, sleep};
use tracing::{debug, info};

use super::Agent;
use crate::llm::{LlmProvider, Message};
use crate::tools::ToolRegistry;

const MAX_ITERATIONS: usize = 20;

/// A coding agent that can read and write files
pub struct CoderAgent;

impl CoderAgent {
    /// Create a new coder agent
    pub fn new() -> Self {
        Self
    }
}

impl Default for CoderAgent {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Agent for CoderAgent {
    fn system_prompt(&self) -> String {
        r#"You are a coding assistant that helps with software development tasks.

You have access to tools for file operations. Use them to complete tasks.

After receiving tool results, continue working on the task until it's complete.
When you're done, provide a summary of what you accomplished.

Important:
- Always read files before editing them to understand their current state
- Make sure old_string in edit_file is unique in the file
- Parent directories will be created automatically when writing files
"#
        .to_string()
    }

    async fn run(
        &self,
        task: &str,
        provider: &dyn LlmProvider,
        tools: &ToolRegistry,
    ) -> Result<String> {
        let mut messages = vec![Message::user(task)];

        for iteration in 0..MAX_ITERATIONS {
            info!(iteration, "agent iteration");

            // Basic rate limiting to avoid hammering the API
            if iteration > 0 {
                sleep(Duration::from_millis(100)).await;
            }

            // Get tools as trait object references
            let tool_refs: Vec<&dyn crate::tools::Tool> = tools.all();

            // Call the LLM
            let response = provider
                .chat(&self.system_prompt(), &messages, &tool_refs)
                .await?;

            debug!(content = %response.message.content, "llm response");

            // Get tool calls from the response (native API format)
            let tool_calls = response.tool_calls;

            if tool_calls.is_empty() {
                // No tool calls, we're done
                info!("agent completed (no more tool calls)");
                return Ok(response.message.content);
            }

            // Execute each tool call and collect results
            let mut tool_results = Vec::with_capacity(tool_calls.len());
            for tool_call in &tool_calls {
                info!(tool = %tool_call.name, "executing tool");

                let result = if let Some(tool) = tools.get(&tool_call.name) {
                    match tool.execute(tool_call.arguments.clone()).await {
                        Ok(output) => output,
                        Err(e) => format!("Error: {}", e),
                    }
                } else {
                    format!("Error: unknown tool '{}'", tool_call.name)
                };

                debug!(tool = %tool_call.name, result = %result, "tool result");

                tool_results.push((tool_call.id.clone(), result));
            }

            // Add assistant message with tool calls (move ownership, no clone needed)
            messages.push(Message::assistant_with_tools(
                &response.message.content,
                tool_calls,
            ));

            // Add tool results to messages
            for (id, result) in tool_results {
                messages.push(Message::tool_result(&id, result));
            }
        }

        anyhow::bail!("agent exceeded maximum iterations ({})", MAX_ITERATIONS);
    }
}
