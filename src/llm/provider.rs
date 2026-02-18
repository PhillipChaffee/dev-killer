use anyhow::Result;
use async_trait::async_trait;

use super::{Message, ToolCall};
use crate::tools::Tool;

/// Response from an LLM
#[derive(Debug, Clone)]
pub struct LlmResponse {
    /// The message content
    pub message: Message,
    /// Tool calls requested by the LLM
    pub tool_calls: Vec<ToolCall>,
}

/// Trait for LLM providers
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Send messages to the LLM and get a response
    async fn chat(
        &self,
        system: &str,
        messages: &[Message],
        tools: &[&dyn Tool],
    ) -> Result<LlmResponse>;

    /// Get the provider name
    fn name(&self) -> &str;
}
