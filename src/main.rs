use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use tracing::{debug, error, info};
use tracing_subscriber::EnvFilter;

use dev_killer::{
    AnthropicProvider, CoderAgent, EditFileTool, Executor, GlobTool, GrepTool, LlmProvider,
    OpenAIProvider, OrchestratorAgent, ProjectConfig, ReadFileTool, SessionState, ShellTool,
    SqliteStorage, Storage, ToolRegistry, WriteFileTool,
};

#[derive(Parser)]
#[command(name = "dev-killer")]
#[command(about = "An autonomous coding agent platform", long_about = None)]
struct Cli {
    /// Enable verbose output
    #[arg(short, long, global = true)]
    verbose: bool,

    /// LLM provider to use (anthropic, openai)
    #[arg(long, default_value = "anthropic")]
    provider: String,

    /// Model to use (provider-specific)
    #[arg(long)]
    model: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run a task
    Run {
        /// The task to perform
        task: String,

        /// Use simple mode (single coder agent) instead of full orchestration
        #[arg(long)]
        simple: bool,

        /// Save session for later resume (enables persistence)
        #[arg(long)]
        save_session: bool,
    },

    /// Resume a previously interrupted session
    Resume {
        /// Session ID to resume
        session_id: String,

        /// Use simple mode (single coder agent)
        #[arg(long)]
        simple: bool,
    },

    /// List saved sessions
    Sessions {
        /// Show only sessions with this status (pending, in_progress, completed, failed, interrupted)
        #[arg(long)]
        status: Option<String>,
    },

    /// Delete a session
    DeleteSession {
        /// Session ID to delete
        session_id: String,
    },
}

fn init_logging(verbose: bool) {
    let filter = if verbose {
        EnvFilter::new("debug")
    } else {
        EnvFilter::from_default_env().add_directive("info".parse().expect("valid log directive"))
    };

    tracing_subscriber::fmt().with_env_filter(filter).init();
}

fn create_provider(provider: &str, model: Option<&str>) -> Result<Box<dyn LlmProvider>> {
    match provider {
        "anthropic" => {
            let p = if let Some(m) = model {
                AnthropicProvider::new(m)?
            } else {
                AnthropicProvider::sonnet()?
            };
            Ok(Box::new(p))
        }
        "openai" => {
            let p = if let Some(m) = model {
                OpenAIProvider::new(m)?
            } else {
                OpenAIProvider::gpt4o()?
            };
            Ok(Box::new(p))
        }
        _ => anyhow::bail!("unknown provider: {}", provider),
    }
}

fn create_tool_registry() -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    // File tools
    registry.register(ReadFileTool);
    registry.register(WriteFileTool);
    registry.register(EditFileTool);
    // Shell tool
    registry.register(ShellTool);
    // Search tools
    registry.register(GlobTool);
    registry.register(GrepTool);
    registry
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    init_logging(cli.verbose);

    // Load configuration with precedence: CLI > env > project > global > defaults
    let config = ProjectConfig::load().unwrap_or_else(|e| {
        debug!(error = %e, "failed to load config, using defaults");
        ProjectConfig::default()
    });

    match cli.command {
        Commands::Run {
            task,
            simple,
            save_session,
        } => {
            // Apply config defaults - CLI flags override config
            let use_simple = simple || config.simple_mode;
            let use_save_session = save_session || config.save_sessions;
            let provider_name = config.provider.as_deref().unwrap_or(&cli.provider);
            let model_name = cli.model.as_deref().or(config.model.as_deref());

            info!(provider = %provider_name, simple = use_simple, save_session = use_save_session, "starting task");

            let provider = create_provider(provider_name, model_name)
                .context("failed to create LLM provider")?;

            let tools = create_tool_registry();

            let result = if use_save_session {
                // Run with session tracking
                let storage = SqliteStorage::default_location()
                    .context("failed to initialize session storage")?;
                let executor = Executor::with_storage(tools, Box::new(storage));

                let working_dir = std::env::current_dir()
                    .context("failed to get current directory")?
                    .to_string_lossy()
                    .to_string();

                let mut session = SessionState::new(&task, working_dir);
                info!(session_id = %session.id, "created new session");

                if use_simple {
                    info!("using simple mode (single coder agent)");
                    let agent = CoderAgent::new();
                    executor
                        .run_with_session(&agent, &mut session, provider.as_ref())
                        .await
                } else {
                    info!("using orchestrator mode (planner -> coder -> tester -> reviewer)");
                    let agent = OrchestratorAgent::new();
                    executor
                        .run_with_session(&agent, &mut session, provider.as_ref())
                        .await
                }
            } else {
                // Run without session tracking
                let executor = Executor::new(tools);

                if use_simple {
                    info!("using simple mode (single coder agent)");
                    let agent = CoderAgent::new();
                    executor.run(&agent, &task, provider.as_ref()).await
                } else {
                    info!("using orchestrator mode (planner -> coder -> tester -> reviewer)");
                    let agent = OrchestratorAgent::new();
                    executor.run(&agent, &task, provider.as_ref()).await
                }
            };

            match result {
                Ok(output) => {
                    println!("\n{}", output);
                }
                Err(e) => {
                    error!(error = %e, "task failed");
                    std::process::exit(1);
                }
            }
        }

        Commands::Resume { session_id, simple } => {
            // Apply config defaults - CLI flags override config
            let use_simple = simple || config.simple_mode;
            let provider_name = config.provider.as_deref().unwrap_or(&cli.provider);
            let model_name = cli.model.as_deref().or(config.model.as_deref());

            info!(session_id = %session_id, "resuming session");

            let provider = create_provider(provider_name, model_name)
                .context("failed to create LLM provider")?;

            let tools = create_tool_registry();
            let storage = SqliteStorage::default_location()
                .context("failed to initialize session storage")?;
            let executor = Executor::with_storage(tools, Box::new(storage));

            let result = if use_simple {
                let agent = CoderAgent::new();
                executor
                    .resume_session(&session_id, &agent, provider.as_ref())
                    .await
            } else {
                let agent = OrchestratorAgent::new();
                executor
                    .resume_session(&session_id, &agent, provider.as_ref())
                    .await
            };

            match result {
                Ok(output) => {
                    println!("\n{}", output);
                }
                Err(e) => {
                    error!(error = %e, "resume failed");
                    std::process::exit(1);
                }
            }
        }

        Commands::Sessions { status } => {
            let storage = SqliteStorage::default_location()
                .context("failed to initialize session storage")?;

            let sessions = storage.list().await?;

            if sessions.is_empty() {
                println!("No sessions found.");
                return Ok(());
            }

            println!("{:<10} {:<12} {:<12} TASK", "ID", "STATUS", "PHASE");
            println!("{}", "-".repeat(70));

            for session in sessions {
                // Filter by status if specified
                if let Some(ref filter_status) = status {
                    if session.status != *filter_status {
                        continue;
                    }
                }

                println!("{}", session);
            }
        }

        Commands::DeleteSession { session_id } => {
            let storage = SqliteStorage::default_location()
                .context("failed to initialize session storage")?;

            storage.delete(&session_id).await?;
            println!("Deleted session: {}", session_id);
        }
    }

    Ok(())
}
