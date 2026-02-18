use anyhow::Result;
use async_trait::async_trait;
use tokio::time::{Duration, sleep};
use tracing::{debug, info};

use super::Agent;
use crate::llm::{LlmProvider, Message};
use crate::tools::ToolRegistry;

const MAX_ITERATIONS: usize = 15;

/// An agent that runs tests and validates implementation
pub struct TesterAgent;

impl TesterAgent {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TesterAgent {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Agent for TesterAgent {
    fn system_prompt(&self) -> String {
        r#"You are a testing agent that validates software implementations.

Your job is to:
1. Run the test suite to verify the implementation
2. Check that new code compiles without errors
3. Verify that the implementation meets the requirements
4. Report any issues found

You have access to shell commands to run tests and file tools to inspect code.

Your workflow:
1. First, run `cargo check` to verify compilation
2. Run `cargo test` to run the test suite
3. If tests fail, analyze the failures
4. Report the results clearly

Your output should include:
- Whether compilation succeeded
- Test results (pass/fail counts)
- Any error messages or failures
- A clear PASS or FAIL verdict

Format your final response like this:
## Test Results

### Compilation
[PASS/FAIL]: [details]

### Tests
[X passed, Y failed]

### Issues Found
- [List any issues]

### Verdict
[PASS/FAIL]: [summary]

Important:
- Run tests with `cargo test`
- Check compilation with `cargo check`
- Be specific about what failed and why
"#
        .to_string()
    }

    async fn run(
        &self,
        task: &str,
        provider: &dyn LlmProvider,
        tools: &ToolRegistry,
    ) -> Result<String> {
        info!("tester agent starting");

        let mut messages = vec![Message::user(format!(
            "Test and validate the following implementation:\n\n{}",
            task
        ))];

        for iteration in 0..MAX_ITERATIONS {
            debug!(iteration, "tester iteration");

            // Rate limiting to avoid hammering the API
            if iteration > 0 {
                sleep(Duration::from_millis(100)).await;
            }

            // Tester needs shell (for cargo test) and read-only file tools
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

            debug!(content = %response.message.content, "tester response");

            let tool_calls = response.tool_calls;

            if tool_calls.is_empty() {
                info!("tester completed");
                return Ok(response.message.content);
            }

            // Execute tool calls
            let mut tool_results = Vec::with_capacity(tool_calls.len());
            for tool_call in &tool_calls {
                debug!(tool = %tool_call.name, "tester executing tool");

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

        anyhow::bail!("tester exceeded maximum iterations ({})", MAX_ITERATIONS);
    }
}
