use serde::{Deserialize, Serialize};

/// Security policy configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Policy {
    /// Paths that are allowed for file operations
    #[serde(default)]
    pub allow_paths: Vec<String>,

    /// Paths that are denied for file operations
    #[serde(default)]
    pub deny_paths: Vec<String>,

    /// Commands that are allowed for shell execution
    #[serde(default)]
    pub allow_commands: Vec<String>,

    /// Commands that are denied for shell execution
    #[serde(default)]
    pub deny_commands: Vec<String>,
}
