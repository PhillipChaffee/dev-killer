#![allow(dead_code)]

use std::collections::VecDeque;
use std::sync::Mutex;

use anyhow::Result;
use async_trait::async_trait;

use dev_killer::{
    EditFileTool, GlobTool, GrepTool, LlmProvider, LlmResponse, Message, ReadFileTool, ShellTool,
    ToolRegistry, WriteFileTool,
};

/// A mock LLM provider that replays scripted responses in order.
pub struct MockLlmProvider {
    responses: Mutex<VecDeque<LlmResponse>>,
}

impl MockLlmProvider {
    /// Create a mock that returns a single text response with no tool calls.
    pub fn single_response(text: &str) -> Self {
        let response = LlmResponse {
            message: Message::assistant(text),
            tool_calls: vec![],
        };
        Self {
            responses: Mutex::new(VecDeque::from([response])),
        }
    }

    /// Create a mock from a sequence of responses (popped in order).
    pub fn with_responses(responses: Vec<LlmResponse>) -> Self {
        Self {
            responses: Mutex::new(VecDeque::from(responses)),
        }
    }
}

#[async_trait]
impl LlmProvider for MockLlmProvider {
    async fn chat(
        &self,
        _system: &str,
        _messages: &[Message],
        _tools: &[&dyn dev_killer::Tool],
    ) -> Result<LlmResponse> {
        let mut queue = self.responses.lock().unwrap();
        queue
            .pop_front()
            .ok_or_else(|| anyhow::anyhow!("MockLlmProvider: no more responses in queue"))
    }

    fn name(&self) -> &str {
        "mock"
    }
}

/// Create a tool registry with all 6 real tools (same as main.rs).
pub fn create_test_tool_registry() -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    registry.register(ReadFileTool);
    registry.register(WriteFileTool);
    registry.register(EditFileTool);
    registry.register(ShellTool);
    registry.register(GlobTool);
    registry.register(GrepTool);
    registry
}
