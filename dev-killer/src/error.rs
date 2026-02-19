#[derive(Debug, thiserror::Error)]
pub enum DevKillerError {
    #[error("configuration error: {0}")]
    Config(String),

    #[error("provider error: {0}")]
    Provider(String),

    #[error("tool error: {tool_name}: {message}")]
    Tool { tool_name: String, message: String },

    #[error("tool approval denied: {tool_name}: {reason}")]
    ToolDenied { tool_name: String, reason: String },

    #[error("agent error: {agent_name}: {message}")]
    Agent { agent_name: String, message: String },

    #[error("session error: {0}")]
    Session(String),

    #[error("pipeline error: {0}")]
    Pipeline(String),

    #[error("max iterations exceeded: {agent_name} after {iterations} iterations")]
    MaxIterations {
        agent_name: String,
        iterations: usize,
    },

    #[error("storage error: {0}")]
    Storage(String),

    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}
