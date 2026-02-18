use anyhow::Result;
use async_trait::async_trait;
use tokio::time::{Duration, sleep};
use tracing::{debug, info};

use super::Agent;
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
"#
        .to_string()
    }

    async fn run(
        &self,
        task: &str,
        provider: &dyn LlmProvider,
        tools: &ToolRegistry,
    ) -> Result<String> {
        info!("planner agent starting");

        let mut messages = vec![Message::user(format!(
            "Create an implementation plan for the following task:\n\n{}",
            task
        ))];

        for iteration in 0..MAX_ITERATIONS {
            debug!(iteration, "planner iteration");

            // Rate limiting to avoid hammering the API
            if iteration > 0 {
                sleep(Duration::from_millis(100)).await;
            }

            // Get read-only tools (glob, grep, read_file)
            let tool_refs: Vec<&dyn crate::tools::Tool> = tools
                .all()
                .into_iter()
                .filter(|t| {
                    let name = t.name();
                    name == "glob" || name == "grep" || name == "read_file"
                })
                .collect();

            let response = provider
                .chat(&self.system_prompt(), &messages, &tool_refs)
                .await?;

            debug!(content = %response.message.content, "planner response");

            let tool_calls = response.tool_calls;

            if tool_calls.is_empty() {
                info!("planner completed");
                return Ok(response.message.content);
            }

            // Execute tool calls
            let mut tool_results = Vec::with_capacity(tool_calls.len());
            for tool_call in &tool_calls {
                debug!(tool = %tool_call.name, "planner executing tool");

                let result = if let Some(tool) = tools.get(&tool_call.name) {
                    match tool.execute(tool_call.arguments.clone()).await {
                        Ok(output) => output,
                        Err(e) => format!("Error: {}", e),
                    }
                } else {
                    format!("Error: unknown tool '{}'", tool_call.name)
                };

                tool_results.push((tool_call.id.clone(), result));
            }

            messages.push(Message::assistant_with_tools(
                &response.message.content,
                tool_calls,
            ));

            for (id, result) in tool_results {
                messages.push(Message::tool_result(&id, result));
            }
        }

        anyhow::bail!("planner exceeded maximum iterations ({})", MAX_ITERATIONS);
    }
}
