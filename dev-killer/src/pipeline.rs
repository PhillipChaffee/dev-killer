use std::collections::HashMap;

use anyhow::Result;
use tracing::{info, warn};

use crate::agents::runner::agent_loop;
use crate::event::{Event, EventSender};
use crate::llm::{LlmProvider, Message};
use crate::session::SessionPhase;
use crate::tools::ToolRegistry;

/// A configurable agent pipeline.
///
/// Replaces the hardcoded orchestrator flow with a sequence of configurable steps.
/// Each step runs an LLM agent with a specific prompt and tool set.
///
/// # Default pipeline
///
/// `Pipeline::default()` creates: plan -> code -> test -> review (matching the
/// original `OrchestratorAgent` behavior).
///
/// # Simple pipeline
///
/// `Pipeline::simple()` creates a single coder step.
pub struct Pipeline {
    pub steps: Vec<PipelineStep>,
    pub max_review_iterations: usize,
}

impl Pipeline {
    /// Create a fully custom pipeline.
    pub fn custom(steps: Vec<PipelineStep>) -> Self {
        Self {
            steps,
            max_review_iterations: 3,
        }
    }

    /// Create a simple pipeline (single coder agent).
    pub fn simple() -> Self {
        Self {
            steps: vec![PipelineStep::code()],
            max_review_iterations: 0,
        }
    }

    /// Set the maximum review iterations before giving up.
    pub fn with_max_review_iterations(mut self, max: usize) -> Self {
        self.max_review_iterations = max;
        self
    }
}

impl Default for Pipeline {
    fn default() -> Self {
        Self {
            steps: vec![
                PipelineStep::plan(),
                PipelineStep::code(),
                PipelineStep::test(),
                PipelineStep::review(),
            ],
            max_review_iterations: 3,
        }
    }
}

/// A single step in the pipeline.
pub struct PipelineStep {
    pub name: String,
    pub system_prompt: String,
    pub allowed_tools: Option<Vec<String>>,
    pub max_iterations: usize,
    pub phase: SessionPhase,
    pub task_formatter: Box<dyn TaskFormatter>,
    pub is_review: bool,
}

impl PipelineStep {
    /// Create a planning step with default prompt.
    pub fn plan() -> Self {
        Self {
            name: "planner".to_string(),
            system_prompt: default_planner_prompt().to_string(),
            allowed_tools: Some(vec![
                "glob".to_string(),
                "grep".to_string(),
                "read_file".to_string(),
            ]),
            max_iterations: 10,
            phase: SessionPhase::Planning,
            task_formatter: Box::new(PlannerTaskFormatter),
            is_review: false,
        }
    }

    /// Create a planning step with a custom prompt.
    pub fn plan_with_prompt(prompt: &str) -> Self {
        let mut step = Self::plan();
        step.system_prompt = prompt.to_string();
        step
    }

    /// Create a coding step with default prompt.
    pub fn code() -> Self {
        Self {
            name: "coder".to_string(),
            system_prompt: default_coder_prompt().to_string(),
            allowed_tools: None, // All tools
            max_iterations: 20,
            phase: SessionPhase::Implementing,
            task_formatter: Box::new(CoderTaskFormatter),
            is_review: false,
        }
    }

    /// Create a coding step with a custom prompt.
    pub fn code_with_prompt(prompt: &str) -> Self {
        let mut step = Self::code();
        step.system_prompt = prompt.to_string();
        step
    }

    /// Create a testing step with default prompt.
    pub fn test() -> Self {
        Self {
            name: "tester".to_string(),
            system_prompt: default_tester_prompt().to_string(),
            allowed_tools: Some(vec![
                "shell".to_string(),
                "glob".to_string(),
                "grep".to_string(),
                "read_file".to_string(),
            ]),
            max_iterations: 15,
            phase: SessionPhase::Testing,
            task_formatter: Box::new(TesterTaskFormatter),
            is_review: false,
        }
    }

    /// Create a review step with default prompt.
    pub fn review() -> Self {
        Self {
            name: "reviewer".to_string(),
            system_prompt: default_reviewer_prompt().to_string(),
            allowed_tools: Some(vec![
                "glob".to_string(),
                "grep".to_string(),
                "read_file".to_string(),
            ]),
            max_iterations: 10,
            phase: SessionPhase::Reviewing,
            task_formatter: Box::new(ReviewerTaskFormatter),
            is_review: true,
        }
    }
}

/// Context passed to task formatters, containing the original task and outputs from previous steps.
pub struct PipelineContext {
    pub original_task: String,
    pub step_outputs: HashMap<String, String>,
}

/// Formats the task/prompt for a pipeline step based on previous step outputs.
pub trait TaskFormatter: Send + Sync {
    fn format_task(&self, context: &PipelineContext) -> String;
}

/// Execute a pipeline with the given provider and tools.
///
/// This replaces `OrchestratorAgent::run()` with a configurable sequence.
pub async fn execute_pipeline(
    pipeline: &Pipeline,
    task: &str,
    provider: &dyn LlmProvider,
    tools: &ToolRegistry,
    events: &EventSender,
) -> Result<String> {
    info!(task, "pipeline starting");

    let mut context = PipelineContext {
        original_task: task.to_string(),
        step_outputs: HashMap::new(),
    };

    // Find the review step index (if any) for the review loop
    let review_idx = pipeline.steps.iter().position(|s| s.is_review);

    // Execute non-review steps sequentially
    let steps_before_review = review_idx.unwrap_or(pipeline.steps.len());

    for step in &pipeline.steps[..steps_before_review] {
        let output = execute_step(step, &context, provider, tools, events).await?;
        context.step_outputs.insert(step.name.clone(), output);
    }

    // If there's a review step, run the review loop
    if let Some(review_idx) = review_idx {
        let review_step = &pipeline.steps[review_idx];

        for review_iteration in 0..pipeline.max_review_iterations {
            info!(iteration = review_iteration, "review iteration");

            let review_output =
                execute_step(review_step, &context, provider, tools, events).await?;

            if is_review_approved(&review_output) {
                info!("task APPROVED");
                context
                    .step_outputs
                    .insert(review_step.name.clone(), review_output);

                return Ok(format_success_output(&context));
            }

            // Needs work — re-run coder and tester if they exist
            if review_iteration < pipeline.max_review_iterations - 1 {
                warn!("review requested changes, attempting fixes");

                // Store review feedback for the fix iteration
                context
                    .step_outputs
                    .insert("review_feedback".to_string(), review_output);

                // Re-run implementation steps (coder and tester)
                for step in &pipeline.steps[..steps_before_review] {
                    if step.phase == SessionPhase::Implementing
                        || step.phase == SessionPhase::Testing
                    {
                        let output = execute_step(step, &context, provider, tools, events).await?;
                        context.step_outputs.insert(step.name.clone(), output);
                    }
                }
            }
        }

        warn!("max review iterations reached without approval");
        return Ok(format!(
            "# Task Incomplete\n\n\
            ## Original Task\n{}\n\n\
            The task could not be completed after {} review iterations.\n\
            Please review the implementation manually.\n\n\
            ---\nStatus: NEEDS_MANUAL_REVIEW",
            task, pipeline.max_review_iterations
        ));
    }

    // No review step — just return the last step's output
    let last_step_name = &pipeline.steps[steps_before_review - 1].name;
    Ok(context
        .step_outputs
        .get(last_step_name)
        .cloned()
        .unwrap_or_default())
}

async fn execute_step(
    step: &PipelineStep,
    context: &PipelineContext,
    provider: &dyn LlmProvider,
    tools: &ToolRegistry,
    events: &EventSender,
) -> Result<String> {
    info!(step = %step.name, "=== STEP: {} ===", step.name.to_uppercase());

    events.emit(Event::PhaseChanged {
        phase: step.phase,
        agent_name: step.name.clone(),
    });

    let task_text = step.task_formatter.format_task(context);

    events.emit(Event::AgentStarted {
        agent_name: step.name.clone(),
        task_preview: truncate(&task_text, 200),
    });

    let messages = vec![Message::user(task_text)];

    let allowed: Option<Vec<&str>> = step
        .allowed_tools
        .as_ref()
        .map(|tools| tools.iter().map(|s| s.as_str()).collect());

    let output = agent_loop(
        &step.name,
        &step.system_prompt,
        messages,
        provider,
        tools,
        allowed.as_deref(),
        step.max_iterations,
        events,
    )
    .await?;

    info!(step = %step.name, output_len = output.len(), "step completed");

    events.emit(Event::AgentCompleted {
        agent_name: step.name.clone(),
        output_preview: truncate(&output, 200),
    });

    Ok(output)
}

fn is_review_approved(review: &str) -> bool {
    for line in review.lines() {
        let trimmed = line.trim().to_uppercase();
        if trimmed == "VERDICT: APPROVED" {
            return true;
        }
        if trimmed == "VERDICT: NEEDS_WORK" {
            return false;
        }
    }
    let lower = review.to_lowercase();
    lower.contains("approved") && !lower.contains("needs_work")
}

fn format_success_output(context: &PipelineContext) -> String {
    let mut output = format!(
        "# Task Completed\n\n## Original Task\n{}",
        context.original_task
    );

    for (name, content) in &context.step_outputs {
        output.push_str(&format!("\n\n## {}\n{}", capitalize(name), content));
    }

    output.push_str("\n\n---\nStatus: SUCCESS");
    output
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().to_string() + c.as_str(),
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len])
    }
}

// --- Default task formatters ---

struct PlannerTaskFormatter;
impl TaskFormatter for PlannerTaskFormatter {
    fn format_task(&self, context: &PipelineContext) -> String {
        format!(
            "Create an implementation plan for the following task:\n\n{}",
            context.original_task
        )
    }
}

struct CoderTaskFormatter;
impl TaskFormatter for CoderTaskFormatter {
    fn format_task(&self, context: &PipelineContext) -> String {
        if let Some(plan) = context.step_outputs.get("planner") {
            if let Some(feedback) = context.step_outputs.get("review_feedback") {
                // Fix iteration — include previous implementation + feedback
                let prev_impl = context
                    .step_outputs
                    .get("coder")
                    .map(|s| s.as_str())
                    .unwrap_or("");
                let test_results = context
                    .step_outputs
                    .get("tester")
                    .map(|s| s.as_str())
                    .unwrap_or("");
                format!(
                    "Fix the following issues identified in code review:\n\n\
                    ## Original Task\n{}\n\n\
                    ## Implementation Plan\n{}\n\n\
                    ## Previous Implementation\n{}\n\n\
                    ## Test Results\n{}\n\n\
                    ## Review Feedback\n{}\n\n\
                    Please address all issues mentioned in the review.",
                    context.original_task, plan, prev_impl, test_results, feedback
                )
            } else {
                format!(
                    "Implement the following task according to this plan:\n\n\
                    ## Original Task\n{}\n\n\
                    ## Implementation Plan\n{}",
                    context.original_task, plan
                )
            }
        } else {
            // Simple mode — no plan
            context.original_task.clone()
        }
    }
}

struct TesterTaskFormatter;
impl TaskFormatter for TesterTaskFormatter {
    fn format_task(&self, context: &PipelineContext) -> String {
        let implementation = context
            .step_outputs
            .get("coder")
            .map(|s| s.as_str())
            .unwrap_or("");
        format!(
            "Test the implementation of this task:\n\n\
            ## Original Task\n{}\n\n\
            ## Implementation Summary\n{}",
            context.original_task, implementation
        )
    }
}

struct ReviewerTaskFormatter;
impl TaskFormatter for ReviewerTaskFormatter {
    fn format_task(&self, context: &PipelineContext) -> String {
        let implementation = context
            .step_outputs
            .get("coder")
            .map(|s| s.as_str())
            .unwrap_or("");
        let test_results = context
            .step_outputs
            .get("tester")
            .map(|s| s.as_str())
            .unwrap_or("");
        format!(
            "Review the implementation of this task:\n\n\
            ## Original Task\n{}\n\n\
            ## Implementation Summary\n{}\n\n\
            ## Test Results\n{}",
            context.original_task, implementation, test_results
        )
    }
}

// --- Default system prompts (extracted from agent implementations) ---

fn default_planner_prompt() -> &'static str {
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
}

fn default_coder_prompt() -> &'static str {
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
}

fn default_tester_prompt() -> &'static str {
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
}

fn default_reviewer_prompt() -> &'static str {
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
}
