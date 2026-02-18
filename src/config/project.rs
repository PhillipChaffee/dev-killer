use serde::{Deserialize, Serialize};

use super::Policy;

/// Project-level configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectConfig {
    /// LLM provider to use (e.g., "anthropic", "openai")
    #[serde(default)]
    pub provider: Option<String>,

    /// Model to use
    #[serde(default)]
    pub model: Option<String>,

    /// Security policy
    #[serde(default)]
    pub policy: Policy,
}
