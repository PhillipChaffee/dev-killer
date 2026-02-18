use anyhow::Result;
use async_trait::async_trait;
use tracing::{info, warn};

use super::message::TaskContext;
use super::{Agent, CoderAgent, PlannerAgent, ReviewerAgent, TesterAgent};
use crate::llm::LlmProvider;
use crate::tools::ToolRegistry;

const MAX_REVIEW_ITERATIONS: usize = 3;

/// Orchestrator agent that coordinates multiple specialized agents
pub struct OrchestratorAgent {
    planner: PlannerAgent,
    coder: CoderAgent,
    tester: TesterAgent,
    reviewer: ReviewerAgent,
}

impl OrchestratorAgent {
    pub fn new() -> Self {
        Self {
            planner: PlannerAgent::new(),
            coder: CoderAgent::new(),
            tester: TesterAgent::new(),
            reviewer: ReviewerAgent::new(),
        }
    }

    /// Run tests and return the results
    async fn run_tests(
        &self,
        task: &str,
        implementation: &str,
        provider: &dyn LlmProvider,
        tools: &ToolRegistry,
    ) -> Result<String> {
        let tester_task = format!(
            "Test the implementation of this task:\n\n\
            ## Original Task\n{}\n\n\
            ## Implementation Summary\n{}",
            task, implementation
        );

        let test_results = self.tester.run(&tester_task, provider, tools).await?;
        info!("tester completed");
        Ok(test_results)
    }
}

impl Default for OrchestratorAgent {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Agent for OrchestratorAgent {
    fn system_prompt(&self) -> String {
        // Orchestrator doesn't use LLM directly, it coordinates other agents
        String::new()
    }

    async fn run(
        &self,
        task: &str,
        provider: &dyn LlmProvider,
        tools: &ToolRegistry,
    ) -> Result<String> {
        // Context tracks state for future persistence (Phase 4)
        let mut _context = TaskContext::new(task);

        info!(task, "orchestrator starting");

        // Phase 1: Planning
        info!("=== PHASE 1: PLANNING ===");

        let plan = self.planner.run(task, provider, tools).await?;
        info!(plan_length = plan.len(), "planner completed");

        _context = _context.with_previous_work(format!("Plan:\n{}", plan));

        // Phase 2: Implementation
        info!("=== PHASE 2: IMPLEMENTATION ===");

        let coder_task = format!(
            "Implement the following task according to this plan:\n\n\
            ## Original Task\n{}\n\n\
            ## Implementation Plan\n{}",
            task, plan
        );

        let mut implementation = self.coder.run(&coder_task, provider, tools).await?;
        info!(impl_length = implementation.len(), "coder completed");

        _context = _context.with_previous_work(format!("Implementation:\n{}", implementation));

        // Phase 3: Testing
        info!("=== PHASE 3: TESTING ===");

        let mut test_results = self
            .run_tests(task, &implementation, provider, tools)
            .await?;

        _context = _context.with_previous_work(format!("Test Results:\n{}", test_results));

        // Phase 4: Review (with retry loop)
        info!("=== PHASE 4: REVIEW ===");

        for review_iteration in 0..MAX_REVIEW_ITERATIONS {
            info!(iteration = review_iteration, "review iteration");

            let reviewer_task = format!(
                "Review the implementation of this task:\n\n\
                ## Original Task\n{}\n\n\
                ## Implementation Summary\n{}\n\n\
                ## Test Results\n{}",
                task, implementation, test_results
            );

            let review = self.reviewer.run(&reviewer_task, provider, tools).await?;
            info!("reviewer completed");

            // Check if approved
            let review_lower = review.to_lowercase();
            if review_lower.contains("approved") && !review_lower.contains("needs_work") {
                info!("task APPROVED");

                return Ok(format!(
                    "# Task Completed\n\n\
                    ## Original Task\n{}\n\n\
                    ## Plan\n{}\n\n\
                    ## Implementation\n{}\n\n\
                    ## Test Results\n{}\n\n\
                    ## Review\n{}\n\n\
                    ---\nStatus: SUCCESS",
                    task, plan, implementation, test_results, review
                ));
            }

            // Needs work - try to fix
            if review_iteration < MAX_REVIEW_ITERATIONS - 1 {
                warn!("review requested changes, attempting fixes");

                let fix_task = format!(
                    "Fix the following issues identified in code review:\n\n\
                    ## Original Task\n{}\n\n\
                    ## Review Feedback\n{}\n\n\
                    Please address all issues mentioned in the review.",
                    task, review
                );

                // Apply fixes
                implementation = self.coder.run(&fix_task, provider, tools).await?;
                _context = _context.with_previous_work(format!("Fix attempt:\n{}", implementation));

                // Re-run tests after fixes
                info!("re-running tests after fixes");
                test_results = self
                    .run_tests(task, &implementation, provider, tools)
                    .await?;
                _context = _context.with_previous_work(format!("Test Results:\n{}", test_results));
            }
        }

        // Max iterations reached
        warn!("max review iterations reached without approval");

        Ok(format!(
            "# Task Incomplete\n\n\
            ## Original Task\n{}\n\n\
            The task could not be completed after {} review iterations.\n\
            Please review the implementation manually.\n\n\
            ---\nStatus: NEEDS_MANUAL_REVIEW",
            task, MAX_REVIEW_ITERATIONS
        ))
    }
}
