use anyhow::Result;
use async_trait::async_trait;

use super::Agent;
use super::runner::agent_loop;
use crate::event::EventSender;
use crate::llm::{LlmProvider, Message};
use crate::tools::ToolRegistry;

const MAX_ITERATIONS: usize = 10;

/// An agent that analyzes tasks and creates implementation plans
pub struct PlannerAgent;

impl PlannerAgent {
    pub fn new() -> Self {
        Self
    }
}

impl Default for PlannerAgent {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Agent for PlannerAgent {
    fn system_prompt(&self) -> String {
        r#"You are a planning agent that analyzes software development tasks and creates detailed implementation plans.

Your job is to:
1. Understand the task requirements
2. Explore the codebase to understand the current structure
3. Identify what files need to be created or modified
4. Create a step-by-step implementation plan

You have access to tools for reading files and searching the codebase. Use them to understand the existing code structure.

Your output should be a clear, numbered implementation plan that another agent can follow.

Format your plan like this:
## Implementation Plan

### Overview
[Brief description of what needs to be done]

### Steps
1. [First step with specific file paths and changes]
2. [Second step...]
...

### Files to Modify
- [file1.rs]: [what changes]
- [file2.rs]: [what changes]

### Files to Create
- [new_file.rs]: [description]

### Testing Strategy
- [How to verify the implementation works]

Important:
- Be specific about file paths
- Include code snippets where helpful
- Consider edge cases
- Think about testing requirements
- When you have gathered enough information, stop using tools and output your implementation plan
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
        let messages = vec![Message::user(format!(
            "Create an implementation plan for the following task:\n\n{}",
            task
        ))];

        agent_loop(
            "planner",
            &self.system_prompt(),
            messages,
            provider,
            tools,
            Some(&["glob", "grep", "read_file"]),
            MAX_ITERATIONS,
            events,
        )
        .await
    }
}
