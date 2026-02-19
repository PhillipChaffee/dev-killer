pub mod agents;
pub mod config;
pub mod llm;
pub mod runtime;
pub mod session;
pub mod tools;

pub use agents::{
    Agent, AgentMessage, CoderAgent, OrchestratorAgent, PlannerAgent, ReviewerAgent, TaskContext,
    TaskId, TaskResult, TaskStatus, TesterAgent,
};
pub use config::{Policy, ProjectConfig};
pub use llm::{
    AnthropicProvider, LlmProvider, LlmResponse, Message, MessageRole, OpenAIProvider, ToolCall,
    ToolResult,
};
pub use runtime::Executor;
pub use session::{
    SessionPhase, SessionState, SessionStatus, SessionSummary, SqliteStorage, Storage,
};
pub use tools::{
    EditFileTool, GlobTool, GrepTool, ReadFileTool, ShellTool, Tool, ToolRegistry, WriteFileTool,
};

/// Multiplies two 32-bit integers and returns their product.
///
/// # Arguments
///
/// * `a` - The first integer to multiply
/// * `b` - The second integer to multiply
///
/// # Returns
///
/// The product of `a` and `b` as an `i32`
///
/// # Examples
///
/// ```
/// use dev_killer::multiply;
///
/// assert_eq!(multiply(2, 3), 6);
/// assert_eq!(multiply(-2, 3), -6);
/// assert_eq!(multiply(0, 5), 0);
/// ```
pub fn multiply(a: i32, b: i32) -> i32 {
    a * b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_multiply_positive_numbers() {
        assert_eq!(multiply(2, 3), 6);
        assert_eq!(multiply(5, 7), 35);
        assert_eq!(multiply(10, 10), 100);
    }

    #[test]
    fn test_multiply_with_zero() {
        assert_eq!(multiply(0, 5), 0);
        assert_eq!(multiply(10, 0), 0);
        assert_eq!(multiply(0, 0), 0);
    }

    #[test]
    fn test_multiply_negative_numbers() {
        assert_eq!(multiply(-2, 3), -6);
        assert_eq!(multiply(2, -3), -6);
        assert_eq!(multiply(-2, -3), 6);
        assert_eq!(multiply(-5, -7), 35);
    }

    #[test]
    fn test_multiply_by_one() {
        assert_eq!(multiply(1, 42), 42);
        assert_eq!(multiply(42, 1), 42);
        assert_eq!(multiply(-42, 1), -42);
        assert_eq!(multiply(1, -42), -42);
    }

    #[test]
    fn test_multiply_large_numbers() {
        assert_eq!(multiply(1000, 1000), 1_000_000);
        assert_eq!(multiply(-1000, 1000), -1_000_000);
        assert_eq!(multiply(32767, 2), 65534); // Close to i32::MAX / 2
    }

    #[test]
    #[should_panic]
    fn test_multiply_overflow() {
        // This should cause an overflow in debug mode
        let _ = multiply(i32::MAX, 2);
    }
}
