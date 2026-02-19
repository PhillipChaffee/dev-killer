use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc;

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

    /// Send messages to the LLM with token-by-token streaming.
    ///
    /// Each text token is sent through `token_sender` as it arrives.
    /// The full `LlmResponse` (with accumulated content and tool calls) is returned
    /// when the stream completes.
    ///
    /// Default implementation falls back to non-streaming `chat()` and sends the
    /// complete response as a single token.
    async fn chat_stream(
        &self,
        system: &str,
        messages: &[Message],
        tools: &[&dyn Tool],
        token_sender: mpsc::Sender<String>,
    ) -> Result<LlmResponse> {
        let response = self.chat(system, messages, tools).await?;
        let _ = token_sender.send(response.message.content.clone()).await;
        Ok(response)
    }

    /// Whether this provider supports streaming.
    ///
    /// When `true`, [`chat_stream`](Self::chat_stream) will emit tokens incrementally.
    /// When `false`, it falls back to `chat()`.
    fn supports_streaming(&self) -> bool {
        false
    }

    /// Get the provider name
    fn name(&self) -> &str;
}
