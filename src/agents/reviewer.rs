use anyhow::Result;
use async_trait::async_trait;

use super::Agent;
use super::runner::agent_loop;
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
- [ ] No security vulnerabilities (injection, path traversal, credential exposure)

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
VERDICT: APPROVED
or
VERDICT: NEEDS_WORK

The verdict line must appear on its own line starting with "VERDICT: ".
If NEEDS_WORK, explain what needs to be fixed.

Important:
- Be thorough but fair
- Focus on correctness and requirements
- Minor style issues should not block approval
- You are read-only — do not attempt to modify any files
"#
        .to_string()
    }

    async fn run(
        &self,
        task: &str,
        provider: &dyn LlmProvider,
        tools: &ToolRegistry,
    ) -> Result<String> {
        let messages = vec![Message::user(format!(
            "Review the following implementation and determine if it is complete:\n\n{}",
            task
        ))];

        // Reviewer is read-only — no shell, no write tools
        agent_loop(
            "reviewer",
            &self.system_prompt(),
            messages,
            provider,
            tools,
            Some(&["glob", "grep", "read_file"]),
            MAX_ITERATIONS,
        )
        .await
    }
}
