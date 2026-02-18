use anyhow::{Context, Result};
use async_trait::async_trait;
use llm::builder::{LLMBackend, LLMBuilder};
use llm::chat::{ChatMessage, ChatRole, FunctionTool, MessageType, Tool as LlmTool};
use tokio::time::{Duration, timeout};
use tracing::warn;

use super::{LlmProvider, LlmResponse, Message, MessageRole, ToolCall};
use crate::tools::Tool;

/// Anthropic LLM provider using the llm crate
pub struct AnthropicProvider {
    model: String,
    api_key: String,
}

impl AnthropicProvider {
    /// Create a new Anthropic provider with the specified model
    pub fn new(model: impl Into<String>) -> Result<Self> {
        let api_key = std::env::var("ANTHROPIC_API_KEY")
            .context("ANTHROPIC_API_KEY environment variable not set")?;
        Ok(Self {
            model: model.into(),
            api_key,
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
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    fn name(&self) -> &str {
        "anthropic"
    }

    async fn chat(
        &self,
        system: &str,
        messages: &[Message],
        tools: &[&dyn Tool],
    ) -> Result<LlmResponse> {
        // Convert tools to llm crate format
        let llm_tools: Vec<LlmTool> = tools
            .iter()
            .map(|t| LlmTool {
                tool_type: "function".to_string(),
                function: FunctionTool {
                    name: t.name().to_string(),
                    description: t.description().to_string(),
                    parameters: t.schema(),
                },
            })
            .collect();

        // NOTE: We rebuild the LLM client on each call because the llm crate requires
        // tools to be set at build time. This is a known inefficiency for tool-heavy workloads.
        let mut builder = LLMBuilder::new()
            .backend(LLMBackend::Anthropic)
            .api_key(&self.api_key)
            .model(&self.model)
            .system(system)
            .max_tokens(8192);

        // Add tools if present
        for tool in &llm_tools {
            builder = builder.function(
                llm::builder::FunctionBuilder::new(&tool.function.name)
                    .description(&tool.function.description)
                    .json_schema(tool.function.parameters.clone()),
            );
        }

        let llm = builder.build().context("failed to build LLM client")?;

        // Convert our messages to llm crate format
        let chat_messages: Vec<ChatMessage> = messages
            .iter()
            .filter_map(|msg| {
                match msg.role {
                    MessageRole::User => Some(ChatMessage {
                        role: ChatRole::User,
                        message_type: MessageType::Text,
                        content: msg.content.clone(),
                    }),
                    MessageRole::Assistant => {
                        // Include tool calls if present
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
                    MessageRole::Tool => {
                        // Tool results are sent using the ToolResult message type
                        msg.tool_result.as_ref().map(|result| {
                            let tool_call = llm::ToolCall {
                                id: result.tool_call_id.clone(),
                                call_type: "function".to_string(),
                                function: llm::FunctionCall {
                                    name: String::new(), // Not needed for results
                                    arguments: result.result.clone(),
                                },
                            };
                            ChatMessage {
                                role: ChatRole::User,
                                message_type: MessageType::ToolResult(vec![tool_call]),
                                content: String::new(),
                            }
                        })
                    }
                    MessageRole::System => None, // System messages handled separately
                }
            })
            .collect();

        // Call the LLM with a 120 second timeout
        let api_timeout = Duration::from_secs(120);
        let response = if llm_tools.is_empty() {
            timeout(api_timeout, llm.chat(&chat_messages))
                .await
                .context("Anthropic API call timed out after 120 seconds")?
                .context("failed to call Anthropic API")?
        } else {
            timeout(
                api_timeout,
                llm.chat_with_tools(&chat_messages, Some(&llm_tools)),
            )
            .await
            .context("Anthropic API call timed out after 120 seconds")?
            .context("failed to call Anthropic API with tools")?
        };

        let content = response.text().unwrap_or_else(|| {
            warn!("Anthropic API returned empty or missing response text");
            String::new()
        });

        // Extract tool calls from the native API response
        let tool_calls = response
            .tool_calls()
            .map(|calls| {
                calls
                    .iter()
                    .map(|tc| ToolCall {
                        id: tc.id.clone(),
                        name: tc.function.name.clone(),
                        arguments: serde_json::from_str(&tc.function.arguments).unwrap_or_else(
                            |e| {
                                warn!(error = %e, "failed to parse tool call arguments as JSON");
                                serde_json::Value::Null
                            },
                        ),
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(LlmResponse {
            message: Message::assistant(content),
            tool_calls,
        })
    }
}

/// OpenAI LLM provider using the llm crate
pub struct OpenAIProvider {
    model: String,
    api_key: String,
}

impl OpenAIProvider {
    /// Create a new OpenAI provider with the specified model
    pub fn new(model: impl Into<String>) -> Result<Self> {
        let api_key = std::env::var("OPENAI_API_KEY")
            .context("OPENAI_API_KEY environment variable not set")?;
        Ok(Self {
            model: model.into(),
            api_key,
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
}

#[async_trait]
impl LlmProvider for OpenAIProvider {
    fn name(&self) -> &str {
        "openai"
    }

    async fn chat(
        &self,
        system: &str,
        messages: &[Message],
        tools: &[&dyn Tool],
    ) -> Result<LlmResponse> {
        // Convert tools to llm crate format
        let llm_tools: Vec<LlmTool> = tools
            .iter()
            .map(|t| LlmTool {
                tool_type: "function".to_string(),
                function: FunctionTool {
                    name: t.name().to_string(),
                    description: t.description().to_string(),
                    parameters: t.schema(),
                },
            })
            .collect();

        // NOTE: We rebuild the LLM client on each call because the llm crate requires
        // tools to be set at build time. This is a known inefficiency for tool-heavy workloads.
        let mut builder = LLMBuilder::new()
            .backend(LLMBackend::OpenAI)
            .api_key(&self.api_key)
            .model(&self.model)
            .system(system)
            .max_tokens(8192);

        // Add tools if present
        for tool in &llm_tools {
            builder = builder.function(
                llm::builder::FunctionBuilder::new(&tool.function.name)
                    .description(&tool.function.description)
                    .json_schema(tool.function.parameters.clone()),
            );
        }

        let llm = builder.build().context("failed to build LLM client")?;

        // Convert our messages to llm crate format
        let chat_messages: Vec<ChatMessage> = messages
            .iter()
            .filter_map(|msg| {
                match msg.role {
                    MessageRole::User => Some(ChatMessage {
                        role: ChatRole::User,
                        message_type: MessageType::Text,
                        content: msg.content.clone(),
                    }),
                    MessageRole::Assistant => {
                        // Include tool calls if present
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
                    MessageRole::Tool => {
                        // Tool results are sent using the ToolResult message type
                        msg.tool_result.as_ref().map(|result| {
                            let tool_call = llm::ToolCall {
                                id: result.tool_call_id.clone(),
                                call_type: "function".to_string(),
                                function: llm::FunctionCall {
                                    name: String::new(), // Not needed for results
                                    arguments: result.result.clone(),
                                },
                            };
                            ChatMessage {
                                role: ChatRole::User,
                                message_type: MessageType::ToolResult(vec![tool_call]),
                                content: String::new(),
                            }
                        })
                    }
                    MessageRole::System => None, // System messages handled separately
                }
            })
            .collect();

        // Call the LLM with a 120 second timeout
        let api_timeout = Duration::from_secs(120);
        let response = if llm_tools.is_empty() {
            timeout(api_timeout, llm.chat(&chat_messages))
                .await
                .context("OpenAI API call timed out after 120 seconds")?
                .context("failed to call OpenAI API")?
        } else {
            timeout(
                api_timeout,
                llm.chat_with_tools(&chat_messages, Some(&llm_tools)),
            )
            .await
            .context("OpenAI API call timed out after 120 seconds")?
            .context("failed to call OpenAI API with tools")?
        };

        let content = response.text().unwrap_or_else(|| {
            warn!("OpenAI API returned empty or missing response text");
            String::new()
        });

        // Extract tool calls from the native API response
        let tool_calls = response
            .tool_calls()
            .map(|calls| {
                calls
                    .iter()
                    .map(|tc| ToolCall {
                        id: tc.id.clone(),
                        name: tc.function.name.clone(),
                        arguments: serde_json::from_str(&tc.function.arguments).unwrap_or_else(
                            |e| {
                                warn!(error = %e, "failed to parse tool call arguments as JSON");
                                serde_json::Value::Null
                            },
                        ),
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(LlmResponse {
            message: Message::assistant(content),
            tool_calls,
        })
    }
}
