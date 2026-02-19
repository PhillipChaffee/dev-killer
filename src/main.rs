use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

use dev_killer::{
    AnthropicProvider, CoderAgent, EditFileTool, Executor, GlobTool, GrepTool, LlmProvider,
    OpenAIProvider, OrchestratorAgent, ReadFileTool, SessionState, ShellTool, SqliteStorage,
    Storage, ToolRegistry, WriteFileTool,
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

    match cli.command {
        Commands::Run {
            task,
            simple,
            save_session,
        } => {
            info!(provider = %cli.provider, simple, save_session, "starting task");

            let provider = create_provider(&cli.provider, cli.model.as_deref())
                .context("failed to create LLM provider")?;

            let tools = create_tool_registry();

            let result = if save_session {
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

                if simple {
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

                if simple {
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
            info!(session_id = %session_id, "resuming session");

            let provider = create_provider(&cli.provider, cli.model.as_deref())
                .context("failed to create LLM provider")?;

            let tools = create_tool_registry();
            let storage = SqliteStorage::default_location()
                .context("failed to initialize session storage")?;
            let executor = Executor::with_storage(tools, Box::new(storage));

            let result = if simple {
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
