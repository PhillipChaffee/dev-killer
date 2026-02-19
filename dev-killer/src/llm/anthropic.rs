use anyhow::{Context, Result};
use async_trait::async_trait;
use futures::StreamExt;
use llm::builder::{LLMBackend, LLMBuilder};
use llm::chat::{ChatMessage, ChatRole, FunctionTool, MessageType, StreamChunk, Tool as LlmTool};
use tokio::sync::mpsc;
use tokio::time::{Duration, timeout};
use tracing::warn;

use super::{LlmProvider, LlmResponse, Message, MessageRole, ToolCall};
use crate::tools::Tool;

const DEFAULT_MAX_TOKENS: u32 = 8192;
const API_TIMEOUT_SECS: u64 = 120;

/// Parameters for the shared LLM chat implementation
struct ChatParams<'a> {
    backend: LLMBackend,
    provider_name: &'a str,
    api_key: &'a str,
    model: &'a str,
    max_tokens: u32,
    system: &'a str,
    messages: &'a [Message],
    tools: &'a [&'a dyn Tool],
}

/// Build llm crate tool definitions from our tools.
fn build_llm_tools(tools: &[&dyn Tool]) -> Vec<LlmTool> {
    tools
        .iter()
        .map(|t| LlmTool {
            tool_type: "function".to_string(),
            function: FunctionTool {
                name: t.name().to_string(),
                description: t.description().to_string(),
                parameters: t.schema(),
            },
        })
        .collect()
}

/// Build the llm crate client from shared parameters.
fn build_llm_client(
    params: &ChatParams<'_>,
    llm_tools: &[LlmTool],
) -> Result<Box<dyn llm::LLMProvider>> {
    // NOTE: We rebuild the LLM client on each call because the llm crate requires
    // tools to be set at build time. This is a known inefficiency for tool-heavy workloads.
    let mut builder = LLMBuilder::new()
        .backend(params.backend.clone())
        .api_key(params.api_key)
        .model(params.model)
        .system(params.system)
        .max_tokens(params.max_tokens);

    for tool in llm_tools {
        builder = builder.function(
            llm::builder::FunctionBuilder::new(&tool.function.name)
                .description(&tool.function.description)
                .json_schema(tool.function.parameters.clone()),
        );
    }

    builder.build().context("failed to build LLM client")
}

/// Parse tool calls from the response trait object.
fn parse_tool_calls(response: &dyn llm::chat::ChatResponse) -> Vec<ToolCall> {
    response
        .tool_calls()
        .map(|calls| {
            calls
                .iter()
                .map(|tc| {
                    let arguments = match serde_json::from_str(&tc.function.arguments) {
                        Ok(args) => args,
                        Err(e) => {
                            warn!(
                                tool = %tc.function.name,
                                error = %e,
                                "failed to parse tool call arguments as JSON"
                            );
                            serde_json::json!({
                                "error": format!("Failed to parse arguments: {}", e)
                            })
                        }
                    };
                    ToolCall {
                        id: tc.id.clone(),
                        name: tc.function.name.clone(),
                        arguments,
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Shared non-streaming implementation for LLM providers backed by the `llm` crate.
async fn chat_impl(params: ChatParams<'_>) -> Result<LlmResponse> {
    let llm_tools = build_llm_tools(params.tools);
    let llm = build_llm_client(&params, &llm_tools)?;
    let chat_messages: Vec<ChatMessage> =
        params.messages.iter().filter_map(convert_message).collect();

    let api_timeout = Duration::from_secs(API_TIMEOUT_SECS);
    let timeout_msg = format!(
        "{} API call timed out after {} seconds",
        params.provider_name, API_TIMEOUT_SECS
    );
    let error_msg = format!("failed to call {} API", params.provider_name);

    let response: Box<dyn llm::chat::ChatResponse> = if llm_tools.is_empty() {
        timeout(api_timeout, llm.chat(&chat_messages))
            .await
            .context(timeout_msg)?
            .context(error_msg)?
    } else {
        timeout(
            api_timeout,
            llm.chat_with_tools(&chat_messages, Some(&llm_tools)),
        )
        .await
        .context(timeout_msg)?
        .context(error_msg)?
    };

    let tool_calls = parse_tool_calls(response.as_ref());

    let content = response.text().unwrap_or_else(|| {
        // Only warn if there are no tool calls — empty content is normal for tool-use responses
        if tool_calls.is_empty() {
            warn!("{} API returned empty response text", params.provider_name);
        }
        String::new()
    });

    Ok(LlmResponse {
        message: Message::assistant(content),
        tool_calls,
    })
}

/// Shared streaming implementation for LLM providers backed by the `llm` crate.
async fn chat_stream_impl(
    params: ChatParams<'_>,
    token_sender: mpsc::Sender<String>,
) -> Result<LlmResponse> {
    let llm_tools = build_llm_tools(params.tools);
    let llm = build_llm_client(&params, &llm_tools)?;
    let chat_messages: Vec<ChatMessage> =
        params.messages.iter().filter_map(convert_message).collect();

    let api_timeout = Duration::from_secs(API_TIMEOUT_SECS);
    let timeout_msg = format!(
        "{} streaming API call timed out after {} seconds",
        params.provider_name, API_TIMEOUT_SECS
    );
    let stream_err_msg = format!("failed to start {} streaming", params.provider_name);

    let mut stream = timeout(
        api_timeout,
        llm.chat_stream_with_tools(&chat_messages, Some(&llm_tools)),
    )
    .await
    .context(timeout_msg)?
    .context(stream_err_msg)?;

    let mut content = String::new();
    let mut tool_calls: Vec<ToolCall> = Vec::new();

    while let Some(chunk_result) = stream.next().await {
        match chunk_result {
            Ok(StreamChunk::Text(text)) => {
                content.push_str(&text);
                // Best-effort send — drop if the receiver is full or closed
                let _ = token_sender.try_send(text);
            }
            Ok(StreamChunk::ToolUseComplete { tool_call, .. }) => {
                let arguments = match serde_json::from_str(&tool_call.function.arguments) {
                    Ok(args) => args,
                    Err(e) => {
                        warn!(
                            tool = %tool_call.function.name,
                            error = %e,
                            "failed to parse streamed tool call arguments"
                        );
                        serde_json::json!({
                            "error": format!("Failed to parse arguments: {}", e)
                        })
                    }
                };
                tool_calls.push(ToolCall {
                    id: tool_call.id,
                    name: tool_call.function.name,
                    arguments,
                });
            }
            Ok(StreamChunk::Done { .. }) => break,
            Ok(_) => {
                // ToolUseStart, ToolUseInputDelta — intermediate events, skip
            }
            Err(e) => {
                warn!(error = %e, "stream chunk error");
                anyhow::bail!("streaming error: {}", e);
            }
        }
    }

    Ok(LlmResponse {
        message: Message::assistant(content),
        tool_calls,
    })
}

/// Convert our Message to the llm crate's ChatMessage format
fn convert_message(msg: &Message) -> Option<ChatMessage> {
    match msg.role {
        MessageRole::User => Some(ChatMessage {
            role: ChatRole::User,
            message_type: MessageType::Text,
            content: msg.content.clone(),
        }),
        MessageRole::Assistant => {
            if msg.tool_calls.is_empty() {
                Some(ChatMessage {
                    role: ChatRole::Assistant,
                    message_type: MessageType::Text,
                    content: msg.content.clone(),
                })
            } else {
                let tool_calls: Vec<llm::ToolCall> = msg
                    .tool_calls
                    .iter()
                    .map(|tc| llm::ToolCall {
                        id: tc.id.clone(),
                        call_type: "function".to_string(),
                        function: llm::FunctionCall {
                            name: tc.name.clone(),
                            arguments: tc.arguments.to_string(),
                        },
                    })
                    .collect();
                Some(ChatMessage {
                    role: ChatRole::Assistant,
                    message_type: MessageType::ToolUse(tool_calls),
                    content: msg.content.clone(),
                })
            }
        }
        MessageRole::Tool => msg.tool_result.as_ref().map(|result| {
            let tool_call = llm::ToolCall {
                id: result.tool_call_id.clone(),
                call_type: "function".to_string(),
                function: llm::FunctionCall {
                    name: String::new(),
                    arguments: result.result.clone(),
                },
            };
            ChatMessage {
                role: ChatRole::User,
                message_type: MessageType::ToolResult(vec![tool_call]),
                content: String::new(),
            }
        }),
    }
}

/// Anthropic LLM provider using the llm crate
pub struct AnthropicProvider {
    model: String,
    api_key: String,
    max_tokens: u32,
}

impl AnthropicProvider {
    /// Create a new Anthropic provider with the specified model
    pub fn new(model: impl Into<String>) -> Result<Self> {
        let api_key = std::env::var("ANTHROPIC_API_KEY")
            .context("ANTHROPIC_API_KEY environment variable not set")?;
        Ok(Self {
            model: model.into(),
            api_key,
            max_tokens: DEFAULT_MAX_TOKENS,
        })
    }

    /// Create a provider using Claude Sonnet
    pub fn sonnet() -> Result<Self> {
        Self::new("claude-sonnet-4-20250514")
    }

    /// Create a provider using Claude Haiku
    pub fn haiku() -> Result<Self> {
        Self::new("claude-3-5-haiku-20241022")
    }

    fn chat_params<'a>(
        &'a self,
        system: &'a str,
        messages: &'a [Message],
        tools: &'a [&'a dyn Tool],
    ) -> ChatParams<'a> {
        ChatParams {
            backend: LLMBackend::Anthropic,
            provider_name: "Anthropic",
            api_key: &self.api_key,
            model: &self.model,
            max_tokens: self.max_tokens,
            system,
            messages,
            tools,
        }
    }
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    fn name(&self) -> &str {
        "anthropic"
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    async fn chat(
        &self,
        system: &str,
        messages: &[Message],
        tools: &[&dyn Tool],
    ) -> Result<LlmResponse> {
        chat_impl(self.chat_params(system, messages, tools)).await
    }

    async fn chat_stream(
        &self,
        system: &str,
        messages: &[Message],
        tools: &[&dyn Tool],
        token_sender: mpsc::Sender<String>,
    ) -> Result<LlmResponse> {
        chat_stream_impl(self.chat_params(system, messages, tools), token_sender).await
    }
}

/// OpenAI LLM provider using the llm crate
pub struct OpenAIProvider {
    model: String,
    api_key: String,
    max_tokens: u32,
}

impl OpenAIProvider {
    /// Create a new OpenAI provider with the specified model
    pub fn new(model: impl Into<String>) -> Result<Self> {
        let api_key = std::env::var("OPENAI_API_KEY")
            .context("OPENAI_API_KEY environment variable not set")?;
        Ok(Self {
            model: model.into(),
            api_key,
            max_tokens: DEFAULT_MAX_TOKENS,
        })
    }

    /// Create a provider using GPT-4o
    pub fn gpt4o() -> Result<Self> {
        Self::new("gpt-4o")
    }

    /// Create a provider using GPT-4o-mini
    pub fn gpt4o_mini() -> Result<Self> {
        Self::new("gpt-4o-mini")
    }

    fn chat_params<'a>(
        &'a self,
        system: &'a str,
        messages: &'a [Message],
        tools: &'a [&'a dyn Tool],
    ) -> ChatParams<'a> {
        ChatParams {
            backend: LLMBackend::OpenAI,
            provider_name: "OpenAI",
            api_key: &self.api_key,
            model: &self.model,
            max_tokens: self.max_tokens,
            system,
            messages,
            tools,
        }
    }
}

#[async_trait]
impl LlmProvider for OpenAIProvider {
    fn name(&self) -> &str {
        "openai"
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    async fn chat(
        &self,
        system: &str,
        messages: &[Message],
        tools: &[&dyn Tool],
    ) -> Result<LlmResponse> {
        chat_impl(self.chat_params(system, messages, tools)).await
    }

    async fn chat_stream(
        &self,
        system: &str,
        messages: &[Message],
        tools: &[&dyn Tool],
        token_sender: mpsc::Sender<String>,
    ) -> Result<LlmResponse> {
        chat_stream_impl(self.chat_params(system, messages, tools), token_sender).await
    }
}
