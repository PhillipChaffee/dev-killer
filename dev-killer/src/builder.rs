use tracing::debug;

use crate::config::{Policy, ProjectConfig};
use crate::dev_killer::DevKiller;
use crate::error::DevKillerError;
use crate::event::ApprovalMode;
use crate::llm::{AnthropicProvider, LlmProvider, OpenAIProvider};
use crate::pipeline::Pipeline;
use crate::session::{SqliteStorage, Storage};
use crate::tools::ToolRegistry;

/// Builder for constructing a [`DevKiller`] instance.
///
/// # Example
///
/// ```no_run
/// # use dev_killer::DevKiller;
/// # async fn example() -> Result<(), dev_killer::DevKillerError> {
/// let dk = DevKiller::builder()
///     .anthropic(None)?
///     .default_tools()
///     .build()?;
///
/// let handle = dk.run("read src/lib.rs and summarize it").await?;
/// println!("{}", handle.output().await?);
/// # Ok(())
/// # }
/// ```
pub struct DevKillerBuilder {
    provider: Option<Box<dyn LlmProvider>>,
    tools: Option<ToolRegistry>,
    storage: Option<Box<dyn Storage>>,
    policy: Policy,
    pipeline: Option<Pipeline>,
    use_default_tools: bool,
    approval_mode: ApprovalMode,
}

impl DevKillerBuilder {
    pub fn new() -> Self {
        Self {
            provider: None,
            tools: None,
            storage: None,
            policy: Policy::default(),
            pipeline: None,
            use_default_tools: false,
            approval_mode: ApprovalMode::default(),
        }
    }

    /// Set a custom LLM provider.
    pub fn provider(mut self, provider: impl LlmProvider + 'static) -> Self {
        self.provider = Some(Box::new(provider));
        self
    }

    /// Configure the Anthropic provider.
    ///
    /// If `model` is `None`, defaults to Claude Sonnet.
    pub fn anthropic(mut self, model: Option<&str>) -> Result<Self, DevKillerError> {
        let p = if let Some(m) = model {
            AnthropicProvider::new(m)?
        } else {
            AnthropicProvider::sonnet()?
        };
        self.provider = Some(Box::new(p));
        Ok(self)
    }

    /// Configure the OpenAI provider.
    ///
    /// If `model` is `None`, defaults to GPT-4o.
    pub fn openai(mut self, model: Option<&str>) -> Result<Self, DevKillerError> {
        let p = if let Some(m) = model {
            OpenAIProvider::new(m)?
        } else {
            OpenAIProvider::gpt4o()?
        };
        self.provider = Some(Box::new(p));
        Ok(self)
    }

    /// Configure a provider by name ("anthropic" or "openai").
    pub fn provider_by_name(self, name: &str, model: Option<&str>) -> Result<Self, DevKillerError> {
        match name {
            "anthropic" => self.anthropic(model),
            "openai" => self.openai(model),
            _ => Err(DevKillerError::Provider(format!(
                "unknown provider: {}",
                name
            ))),
        }
    }

    /// Register all default tools (file, shell, search) using the configured policy.
    ///
    /// Tools are created during [`build()`](Self::build) using the policy set at that time.
    pub fn default_tools(mut self) -> Self {
        self.use_default_tools = true;
        self
    }

    /// Set a custom tool registry (overrides default tools).
    pub fn tools(mut self, tools: ToolRegistry) -> Self {
        self.tools = Some(tools);
        self.use_default_tools = false;
        self
    }

    /// Add a single tool to the registry.
    ///
    /// If no tools have been set yet, starts with an empty registry.
    pub fn add_tool(mut self, tool: impl crate::tools::Tool + 'static) -> Self {
        let registry = self.tools.get_or_insert_with(ToolRegistry::new);
        registry.register(tool);
        self
    }

    /// Set a custom storage backend.
    pub fn storage(mut self, storage: impl Storage + 'static) -> Self {
        self.storage = Some(Box::new(storage));
        self
    }

    /// Use SQLite storage at the default location (~/.dev-killer/sessions.db).
    pub fn sqlite_storage(mut self) -> Result<Self, DevKillerError> {
        let storage = SqliteStorage::default_location().map_err(|e| {
            DevKillerError::Storage(format!("failed to initialize SQLite storage: {}", e))
        })?;
        self.storage = Some(Box::new(storage));
        Ok(self)
    }

    /// Set the security policy for tools.
    pub fn policy(mut self, policy: Policy) -> Self {
        self.policy = policy;
        self
    }

    /// Set a custom pipeline.
    pub fn pipeline(mut self, pipeline: Pipeline) -> Self {
        self.pipeline = Some(pipeline);
        self
    }

    /// Enable or disable simple mode (single coder agent vs full orchestrator pipeline).
    ///
    /// This is a convenience method. When `simple` is true, sets `Pipeline::simple()`.
    /// When false, uses the default pipeline (plan -> code -> test -> review).
    pub fn simple_mode(mut self, simple: bool) -> Self {
        if simple {
            self.pipeline = Some(Pipeline::simple());
        }
        self
    }

    /// Set the tool approval mode.
    ///
    /// - `AutoApprove` (default): all tools execute without asking
    /// - `ApproveDangerous`: shell, write_file, and edit_file require approval
    /// - `ApproveAll`: every tool call requires approval
    /// - `Custom(fn)`: your function decides which tools need approval
    pub fn approval_mode(mut self, mode: ApprovalMode) -> Self {
        self.approval_mode = mode;
        self
    }

    /// Apply settings from the project configuration file.
    ///
    /// Loads config with precedence: project file > global file > defaults.
    /// Settings applied here can still be overridden by subsequent builder calls.
    pub fn from_config(mut self) -> Result<Self, DevKillerError> {
        let config = ProjectConfig::load()
            .map_err(|e| DevKillerError::Config(format!("failed to load configuration: {}", e)))?;

        debug!("loaded project configuration");

        self.policy = config.policy;
        if let Some(true) = config.simple_mode {
            self.pipeline = Some(Pipeline::simple());
        }

        // Provider from config (can be overridden by explicit provider_by_name call later)
        if let Some(ref provider_name) = config.provider {
            if self.provider.is_none() {
                self = self.provider_by_name(provider_name, config.model.as_deref())?;
            }
        }

        Ok(self)
    }

    /// Build the [`DevKiller`] instance.
    ///
    /// Fails if no provider has been configured.
    pub fn build(self) -> Result<DevKiller, DevKillerError> {
        let provider = self
            .provider
            .ok_or_else(|| DevKillerError::Config("no LLM provider configured".to_string()))?;

        let tools = if let Some(tools) = self.tools {
            tools
        } else if self.use_default_tools {
            ToolRegistry::with_default_tools(&self.policy)
        } else {
            ToolRegistry::new()
        };

        let pipeline = self.pipeline.unwrap_or_default();

        Ok(DevKiller::from_parts(
            provider,
            tools,
            self.storage,
            pipeline,
            self.approval_mode,
        ))
    }
}

impl Default for DevKillerBuilder {
    fn default() -> Self {
        Self::new()
    }
}
