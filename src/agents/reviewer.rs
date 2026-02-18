use anyhow::Result;
use async_trait::async_trait;
use tokio::time::{Duration, sleep};
use tracing::{debug, info};

use super::Agent;
use crate::llm::{LlmProvider, Message};
use crate::tools::ToolRegistry;

const MAX_ITERATIONS: usize = 10;

/// An agent that reviews implementations and validates task completion
pub struct ReviewerAgent;

impl ReviewerAgent {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ReviewerAgent {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Agent for ReviewerAgent {
    fn system_prompt(&self) -> String {
        r#"You are a code review agent that validates whether a task has been completed correctly.

Your job is to:
1. Review the implementation against the original requirements
2. Check code quality and correctness
3. Verify that tests exist and pass
4. Make a final determination: APPROVED or NEEDS_WORK

You have access to read-only tools to inspect the codebase.

Review checklist:
- [ ] Implementation matches requirements
- [ ] Code compiles without errors
- [ ] Tests exist for new functionality
- [ ] Tests pass
- [ ] No obvious bugs or issues
- [ ] Code follows project conventions

Your output should include:
- Summary of what was implemented
- Any issues found
- A clear verdict: APPROVED or NEEDS_WORK

Format your response like this:
## Code Review

### Summary
[What was implemented]

### Checklist
- [x] Implementation matches requirements: [details]
- [x] Code compiles: [details]
- [x] Tests exist: [details]
- [x] Tests pass: [details]
- [ ] Issues found: [list any issues]

### Issues
[List any problems that need to be fixed]

### Verdict
[APPROVED/NEEDS_WORK]: [summary]

If NEEDS_WORK, explain what needs to be fixed.

Important:
- Be thorough but fair
- Focus on correctness and requirements
- Minor style issues should not block approval
"#
        .to_string()
    }

    async fn run(
        &self,
        task: &str,
        provider: &dyn LlmProvider,
        tools: &ToolRegistry,
    ) -> Result<String> {
        info!("reviewer agent starting");

        let mut messages = vec![Message::user(format!(
            "Review the following implementation and determine if it is complete:\n\n{}",
            task
        ))];

        for iteration in 0..MAX_ITERATIONS {
            debug!(iteration, "reviewer iteration");

            // Rate limiting to avoid hammering the API
            if iteration > 0 {
                sleep(Duration::from_millis(100)).await;
            }

            // Reviewer uses read-only tools and shell (for running tests)
            let tool_refs: Vec<&dyn crate::tools::Tool> = tools
                .all()
                .into_iter()
                .filter(|t| {
                    let name = t.name();
                    name == "shell" || name == "glob" || name == "grep" || name == "read_file"
                })
                .collect();

            let response = provider
                .chat(&self.system_prompt(), &messages, &tool_refs)
                .await?;

            debug!(content = %response.message.content, "reviewer response");

            let tool_calls = response.tool_calls;

            if tool_calls.is_empty() {
                info!("reviewer completed");
                return Ok(response.message.content);
            }

            // Execute tool calls
            let mut tool_results = Vec::with_capacity(tool_calls.len());
            for tool_call in &tool_calls {
                debug!(tool = %tool_call.name, "reviewer executing tool");

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

        anyhow::bail!("reviewer exceeded maximum iterations ({})", MAX_ITERATIONS);
    }
}
