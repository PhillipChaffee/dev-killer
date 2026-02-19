use anyhow::{Context, Result};
use tokio::time::{Duration, sleep};
use tracing::{debug, info};

use crate::llm::{LlmProvider, Message};
use crate::tools::ToolRegistry;

/// Shared agent execution loop.
///
/// Handles the common pattern of iterating with an LLM, executing tool calls,
/// and assembling messages until the LLM stops requesting tools.
///
/// - `agent_name`: For logging (e.g., "planner", "coder")
/// - `system_prompt`: The system prompt for this agent
/// - `messages`: Initial messages (typically a single user message)
/// - `provider`: LLM provider to use
/// - `tools`: Full tool registry
/// - `allowed_tools`: If `Some`, only these tools are presented and allowed for execution.
///   If `None`, all tools are available.
/// - `max_iterations`: Maximum number of LLM round-trips before bailing
pub async fn agent_loop(
    agent_name: &str,
    system_prompt: &str,
    mut messages: Vec<Message>,
    provider: &dyn LlmProvider,
    tools: &ToolRegistry,
    allowed_tools: Option<&[&str]>,
    max_iterations: usize,
) -> Result<String> {
    for iteration in 0..max_iterations {
        debug!(agent = agent_name, iteration, "agent iteration");

        // Rate limiting to avoid hammering the API
        if iteration > 0 {
            sleep(Duration::from_millis(100)).await;
        }

        // Build tool references — filter if allowed_tools is specified
        let tool_refs: Vec<&dyn crate::tools::Tool> = if let Some(allowed) = allowed_tools {
            tools
                .all()
                .into_iter()
                .filter(|t| allowed.contains(&t.name()))
                .collect()
        } else {
            tools.all()
        };

        // Call the LLM
        let response = provider
            .chat(system_prompt, &messages, &tool_refs)
            .await
            .with_context(|| format!("{} agent: LLM chat failed", agent_name))?;

        debug!(agent = agent_name, content = %response.message.content, "llm response");

        let tool_calls = response.tool_calls;

        if tool_calls.is_empty() {
            info!(agent = agent_name, "agent completed (no more tool calls)");
            return Ok(response.message.content);
        }

        // Execute each tool call with filter enforcement
        let mut tool_results = Vec::with_capacity(tool_calls.len());
        for tool_call in &tool_calls {
            debug!(agent = agent_name, tool = %tool_call.name, "executing tool");

            let result = if let Some(allowed) = allowed_tools {
                if !allowed.contains(&tool_call.name.as_str()) {
                    format!("Tool '{}' is not available to this agent", tool_call.name)
                } else {
                    execute_tool_call(tools, tool_call).await
                }
            } else {
                execute_tool_call(tools, tool_call).await
            };

            debug!(agent = agent_name, tool = %tool_call.name, result = %result, "tool result");
            tool_results.push((tool_call.id.clone(), result));
        }

        // Add assistant message with tool calls
        messages.push(Message::assistant_with_tools(
            &response.message.content,
            tool_calls,
        ));

        // Add tool results to messages
        for (id, result) in tool_results {
            messages.push(Message::tool_result(&id, result));
        }
    }

    anyhow::bail!(
        "{} agent exceeded maximum iterations ({})",
        agent_name,
        max_iterations
    );
}

async fn execute_tool_call(tools: &ToolRegistry, tool_call: &crate::llm::ToolCall) -> String {
    if let Some(tool) = tools.get(&tool_call.name) {
        match tool.execute(tool_call.arguments.clone()).await {
            Ok(output) => output,
            Err(e) => format!("Error: {}", e),
        }
    } else {
        format!("Error: unknown tool '{}'", tool_call.name)
    }
}
