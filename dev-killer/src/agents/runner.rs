use anyhow::{Context, Result};
use tokio::sync::mpsc;
use tokio::time::{Duration, sleep};
use tracing::{debug, info};

use crate::event::{ApprovalResult, Event, EventSender};
use crate::llm::{LlmProvider, LlmResponse, Message, ToolCall};
use crate::tools::ToolRegistry;

/// Token channel capacity for streaming.
const TOKEN_CHANNEL_CAPACITY: usize = 64;

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
/// - `events`: Event sender for emitting execution events and handling tool approval
#[allow(clippy::too_many_arguments)]
pub async fn agent_loop(
    agent_name: &str,
    system_prompt: &str,
    mut messages: Vec<Message>,
    provider: &dyn LlmProvider,
    tools: &ToolRegistry,
    allowed_tools: Option<&[&str]>,
    max_iterations: usize,
    events: &EventSender,
) -> Result<String> {
    let use_streaming = provider.supports_streaming() && events.is_active();

    for iteration in 0..max_iterations {
        debug!(agent = agent_name, iteration, "agent iteration");

        events.emit(Event::IterationCompleted {
            agent_name: agent_name.to_string(),
            iteration,
            max_iterations,
        });

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

        // Call the LLM (streaming or non-streaming)
        events.emit(Event::LlmRequestStarted {
            agent_name: agent_name.to_string(),
        });

        let response = if use_streaming {
            call_llm_streaming(
                agent_name,
                system_prompt,
                &messages,
                provider,
                &tool_refs,
                events,
            )
            .await
            .with_context(|| format!("{} agent: LLM streaming chat failed", agent_name))?
        } else {
            provider
                .chat(system_prompt, &messages, &tool_refs)
                .await
                .with_context(|| format!("{} agent: LLM chat failed", agent_name))?
        };

        debug!(agent = agent_name, content = %response.message.content, "llm response");

        let tool_calls = response.tool_calls;

        events.emit(Event::LlmResponseCompleted {
            agent_name: agent_name.to_string(),
            tool_call_count: tool_calls.len(),
        });

        if tool_calls.is_empty() {
            info!(agent = agent_name, "agent completed (no more tool calls)");
            return Ok(response.message.content);
        }

        // Execute each tool call with filter enforcement and approval
        let mut tool_results = Vec::with_capacity(tool_calls.len());
        for tool_call in &tool_calls {
            debug!(agent = agent_name, tool = %tool_call.name, "executing tool");

            let result = execute_with_approval(tools, tool_call, allowed_tools, events).await;

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

/// Call the LLM with streaming, forwarding tokens through the event sender.
async fn call_llm_streaming(
    agent_name: &str,
    system_prompt: &str,
    messages: &[Message],
    provider: &dyn LlmProvider,
    tools: &[&dyn crate::tools::Tool],
    events: &EventSender,
) -> Result<LlmResponse> {
    let (token_tx, mut token_rx) = mpsc::channel::<String>(TOKEN_CHANNEL_CAPACITY);
    let agent_name_owned = agent_name.to_string();
    let events_clone = events.clone();

    // Spawn a task to forward tokens to the event channel
    let forwarder = tokio::spawn(async move {
        while let Some(token) = token_rx.recv().await {
            events_clone.emit_token(&agent_name_owned, &token);
        }
    });

    let response = provider
        .chat_stream(system_prompt, messages, tools, token_tx)
        .await;

    // Wait for the forwarder to drain remaining tokens
    let _ = forwarder.await;

    response
}

/// Execute a tool call with allowed-tool filtering and approval.
async fn execute_with_approval(
    tools: &ToolRegistry,
    tool_call: &ToolCall,
    allowed_tools: Option<&[&str]>,
    events: &EventSender,
) -> String {
    // Check if tool is allowed for this agent
    if let Some(allowed) = allowed_tools {
        if !allowed.contains(&tool_call.name.as_str()) {
            return format!("Tool '{}' is not available to this agent", tool_call.name);
        }
    }

    // Check tool approval
    match events
        .request_tool_approval(&tool_call.name, &tool_call.arguments)
        .await
    {
        ApprovalResult::Approved => {}
        ApprovalResult::Denied(reason) => return reason,
    }

    // Execute the tool
    events.emit(Event::ToolStarted {
        tool_name: tool_call.name.clone(),
    });

    let result = if let Some(tool) = tools.get(&tool_call.name) {
        match tool.execute(tool_call.arguments.clone()).await {
            Ok(output) => output,
            Err(e) => format!("Error: {}", e),
        }
    } else {
        format!("Error: unknown tool '{}'", tool_call.name)
    };

    let is_error = result.starts_with("Error:");
    events.emit(Event::ToolCompleted {
        tool_name: tool_call.name.clone(),
        is_error,
    });

    result
}
