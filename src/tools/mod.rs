mod file;
mod registry;

pub use file::{EditFileTool, ReadFileTool, WriteFileTool};
pub use registry::ToolRegistry;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

/// A tool that can be executed by an agent
#[async_trait]
pub trait Tool: Send + Sync {
    /// The unique name of this tool
    fn name(&self) -> &str;

    /// A description of what this tool does
    fn description(&self) -> &str;

    /// JSON schema for the tool's parameters
    fn schema(&self) -> Value;

    /// Execute the tool with the given parameters
    async fn execute(&self, params: Value) -> Result<String>;
}
