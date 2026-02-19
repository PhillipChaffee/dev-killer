use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

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
    pub simple_mode: Option<bool>,

    /// Always save sessions
    #[serde(default)]
    pub save_sessions: Option<bool>,
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
                match Self::load_from_file(&global_path) {
                    Ok(global) => config = config.merge(global),
                    Err(e) => {
                        warn!(path = %global_path.display(), error = %e, "failed to load global config")
                    }
                }
            }
        }

        // Load project config (dev-killer.toml in current directory or parents)
        if let Some(project_path) = Self::find_project_config() {
            debug!(path = %project_path.display(), "loading project config");
            match Self::load_from_file(&project_path) {
                Ok(project) => config = config.merge(project),
                Err(e) => {
                    warn!(path = %project_path.display(), error = %e, "failed to load project config")
                }
            }
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
        // Deny lists should union, not replace
        self.policy.deny_paths.extend(other.policy.deny_paths);
        self.policy.deny_commands.extend(other.policy.deny_commands);
        // Allow lists replace (more specific config wins)
        if !other.policy.allow_paths.is_empty() {
            self.policy.allow_paths = other.policy.allow_paths;
        }
        if !other.policy.allow_commands.is_empty() {
            self.policy.allow_commands = other.policy.allow_commands;
        }
        // Always take explicit non-default values
        if other.max_retries != default_max_retries() {
            self.max_retries = other.max_retries;
        }
        if other.retry_delay_ms != default_retry_delay_ms() {
            self.retry_delay_ms = other.retry_delay_ms;
        }
        // Booleans: other overrides if explicitly set (Some)
        if other.simple_mode.is_some() {
            self.simple_mode = other.simple_mode;
        }
        if other.save_sessions.is_some() {
            self.save_sessions = other.save_sessions;
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
            match retries.parse() {
                Ok(n) => self.max_retries = n,
                Err(_) => warn!(
                    value = %retries,
                    "invalid DEV_KILLER_MAX_RETRIES value, ignoring"
                ),
            }
        }
        if let Ok(delay) = std::env::var("DEV_KILLER_RETRY_DELAY_MS") {
            match delay.parse() {
                Ok(n) => self.retry_delay_ms = n,
                Err(_) => warn!(
                    value = %delay,
                    "invalid DEV_KILLER_RETRY_DELAY_MS value, ignoring"
                ),
            }
        }
        if let Ok(val) = std::env::var("DEV_KILLER_SIMPLE_MODE") {
            self.simple_mode = Some(parse_bool_env(&val));
        }
        if let Ok(val) = std::env::var("DEV_KILLER_SAVE_SESSIONS") {
            self.save_sessions = Some(parse_bool_env(&val));
        }
        self
    }

    /// Get simple_mode value (defaults to false)
    pub fn is_simple_mode(&self) -> bool {
        self.simple_mode.unwrap_or(false)
    }

    /// Get save_sessions value (defaults to false)
    pub fn is_save_sessions(&self) -> bool {
        self.save_sessions.unwrap_or(false)
    }
}

/// Parse a boolean-like environment variable value
fn parse_bool_env(val: &str) -> bool {
    !matches!(val.to_lowercase().as_str(), "false" | "0" | "no" | "off")
}
