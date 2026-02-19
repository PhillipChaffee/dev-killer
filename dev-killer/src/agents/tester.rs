use anyhow::Result;
use async_trait::async_trait;

use super::Agent;
use super::runner::agent_loop;
use crate::event::EventSender;
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
- Do NOT modify any source files. Only use shell for diagnostic commands like `cargo check` and `cargo test`
- You may read files to understand failures, but never write or edit them
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
            "Test and validate the following implementation:\n\n{}",
            task
        ))];

        agent_loop(
            "tester",
            &self.system_prompt(),
            messages,
            provider,
            tools,
            Some(&["shell", "glob", "grep", "read_file"]),
            MAX_ITERATIONS,
            events,
        )
        .await
    }
}
