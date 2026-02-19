use anyhow::Result;
use async_trait::async_trait;

use super::Agent;
use super::runner::agent_loop;
use crate::event::EventSender;
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
        r#"You are a coding agent that implements software changes.

Available tools:
- read_file: Read file contents
- write_file: Create or overwrite a file (parent dirs created automatically)
- edit_file: Find-and-replace in a file (old_string must be unique)
- shell: Run shell commands (builds, tests, git, etc.)
- glob: Find files by pattern
- grep: Search file contents by regex

Workflow:
1. Read relevant files to understand context before making changes
2. Implement changes using write_file or edit_file
3. After making changes, run `cargo check` (or equivalent) to verify compilation
4. Fix any compilation errors before declaring completion

Important rules:
- ALWAYS read a file before editing it
- The old_string in edit_file must match exactly and uniquely
- For multi-file changes, verify compilation after each significant change
- When done, provide a structured summary of what was changed and why

Output format when complete:
## Summary
[What was done]

## Files Modified
- [file]: [what changed]
"#
        .to_string()
    }

    async fn run(
        &self,
        task: &str,
        provider: &dyn LlmProvider,
        tools: &ToolRegistry,
        events: &EventSender,
    ) -> Result<String> {
        let messages = vec![Message::user(task)];

        agent_loop(
            "coder",
            &self.system_prompt(),
            messages,
            provider,
            tools,
            None, // All tools available
            MAX_ITERATIONS,
            events,
        )
        .await
    }
}
