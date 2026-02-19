use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::debug;

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

    /// Maximum retries for LLM API calls
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,

    /// Base delay for exponential backoff (milliseconds)
    #[serde(default = "default_retry_delay_ms")]
    pub retry_delay_ms: u64,

    /// Use simple mode (single coder agent) by default
    #[serde(default)]
    pub simple_mode: bool,

    /// Always save sessions
    #[serde(default)]
    pub save_sessions: bool,
}

fn default_max_retries() -> u32 {
    3
}

fn default_retry_delay_ms() -> u64 {
    1000
}

impl ProjectConfig {
    /// Load configuration with precedence: project -> global -> defaults
    pub fn load() -> Result<Self> {
        let mut config = Self::default();

        // Load global config first (~/.config/dev-killer/config.toml)
        if let Some(global_path) = Self::global_config_path() {
            if global_path.exists() {
                debug!(path = %global_path.display(), "loading global config");
                let global = Self::load_from_file(&global_path)?;
                config = config.merge(global);
            }
        }

        // Load project config (dev-killer.toml in current directory or parents)
        if let Some(project_path) = Self::find_project_config() {
            debug!(path = %project_path.display(), "loading project config");
            let project = Self::load_from_file(&project_path)?;
            config = config.merge(project);
        }

        // Environment variable overrides
        config = config.apply_env_overrides();

        Ok(config)
    }

    /// Load config from a specific file
    pub fn load_from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read config file: {}", path.display()))?;

        toml::from_str(&content)
            .with_context(|| format!("failed to parse config file: {}", path.display()))
    }

    /// Get global config path (~/.config/dev-killer/config.toml)
    fn global_config_path() -> Option<PathBuf> {
        std::env::var("HOME").ok().map(|home| {
            PathBuf::from(home)
                .join(".config")
                .join("dev-killer")
                .join("config.toml")
        })
    }

    /// Find project config by searching current directory and parents
    fn find_project_config() -> Option<PathBuf> {
        let mut current = std::env::current_dir().ok()?;

        loop {
            let config_path = current.join("dev-killer.toml");
            if config_path.exists() {
                return Some(config_path);
            }

            if !current.pop() {
                break;
            }
        }

        None
    }

    /// Merge another config into this one (other takes precedence)
    fn merge(mut self, other: Self) -> Self {
        if other.provider.is_some() {
            self.provider = other.provider;
        }
        if other.model.is_some() {
            self.model = other.model;
        }
        if !other.policy.allow_paths.is_empty() {
            self.policy.allow_paths = other.policy.allow_paths;
        }
        if !other.policy.deny_paths.is_empty() {
            self.policy.deny_paths = other.policy.deny_paths;
        }
        if !other.policy.allow_commands.is_empty() {
            self.policy.allow_commands = other.policy.allow_commands;
        }
        if !other.policy.deny_commands.is_empty() {
            self.policy.deny_commands = other.policy.deny_commands;
        }
        // Always take explicit non-default values
        if other.max_retries != default_max_retries() {
            self.max_retries = other.max_retries;
        }
        if other.retry_delay_ms != default_retry_delay_ms() {
            self.retry_delay_ms = other.retry_delay_ms;
        }
        if other.simple_mode {
            self.simple_mode = true;
        }
        if other.save_sessions {
            self.save_sessions = true;
        }
        self
    }

    /// Apply environment variable overrides
    fn apply_env_overrides(mut self) -> Self {
        if let Ok(provider) = std::env::var("DEV_KILLER_PROVIDER") {
            self.provider = Some(provider);
        }
        if let Ok(model) = std::env::var("DEV_KILLER_MODEL") {
            self.model = Some(model);
        }
        if let Ok(retries) = std::env::var("DEV_KILLER_MAX_RETRIES") {
            if let Ok(n) = retries.parse() {
                self.max_retries = n;
            }
        }
        if std::env::var("DEV_KILLER_SIMPLE_MODE").is_ok() {
            self.simple_mode = true;
        }
        if std::env::var("DEV_KILLER_SAVE_SESSIONS").is_ok() {
            self.save_sessions = true;
        }
        self
    }
}
