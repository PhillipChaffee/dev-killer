use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use tracing::{debug, error, info};
use tracing_subscriber::EnvFilter;

use dev_killer::{
    DevKiller, PortableSession, ProjectConfig, SessionStatus, SqliteStorage, Storage,
};

#[derive(Parser)]
#[command(name = "dev-killer", version)]
#[command(about = "An autonomous coding agent platform", long_about = None)]
struct Cli {
    /// Enable verbose output
    #[arg(short, long, global = true)]
    verbose: bool,

    /// LLM provider to use (anthropic, openai)
    #[arg(long)]
    provider: Option<String>,

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

    /// Export a session to a JSON file for transfer to another environment
    ExportSession {
        /// Session ID to export
        session_id: String,

        /// Output file path (defaults to <session-id>.json)
        #[arg(short, long)]
        output: Option<String>,
    },

    /// Import a session from a JSON file
    ImportSession {
        /// Path to the exported session JSON file
        file: String,

        /// Working directory to use for the imported session
        #[arg(long)]
        working_dir: Option<String>,
    },

    /// Respond with HELLO
    Hello,

    /// Respond with PIPELINE
    Pipeline,
}

fn init_logging(verbose: bool) {
    let filter = if verbose {
        EnvFilter::new("debug")
    } else {
        EnvFilter::from_default_env().add_directive("info".parse().expect("valid log directive"))
    };

    tracing_subscriber::fmt().with_env_filter(filter).init();
}

/// Resolve which provider name to use.
/// CLI argument takes highest precedence, then config file, then default.
fn resolve_provider<'a>(
    cli_provider: Option<&'a str>,
    config_provider: Option<&'a str>,
) -> &'a str {
    cli_provider.or(config_provider).unwrap_or("anthropic")
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
            let use_simple = simple || config.is_simple_mode();
            let use_save_session = save_session || config.is_save_sessions();
            let provider_name =
                resolve_provider(cli.provider.as_deref(), config.provider.as_deref());
            let model_name = cli.model.as_deref().or(config.model.as_deref());

            info!(provider = %provider_name, simple = use_simple, save_session = use_save_session, "starting task");

            let mut builder = DevKiller::builder()
                .provider_by_name(provider_name, model_name)
                .context("failed to create LLM provider")?
                .policy(config.policy)
                .default_tools()
                .simple_mode(use_simple);

            if use_save_session {
                builder = builder
                    .sqlite_storage()
                    .context("failed to initialize session storage")?;
            }

            let dk = builder.build().context("failed to build DevKiller")?;

            match dk.run(&task).await {
                Ok(handle) => {
                    let output = handle.output().await.context("task execution failed")?;
                    println!("\n{}", output);
                }
                Err(e) => {
                    error!(error = %e, "task failed");
                    anyhow::bail!("task failed: {}", e);
                }
            }
        }

        Commands::Resume { session_id, simple } => {
            let use_simple = simple || config.is_simple_mode();
            let provider_name =
                resolve_provider(cli.provider.as_deref(), config.provider.as_deref());
            let model_name = cli.model.as_deref().or(config.model.as_deref());

            info!(session_id = %session_id, "resuming session");

            let dk = DevKiller::builder()
                .provider_by_name(provider_name, model_name)
                .context("failed to create LLM provider")?
                .policy(config.policy)
                .default_tools()
                .simple_mode(use_simple)
                .sqlite_storage()
                .context("failed to initialize session storage")?
                .build()
                .context("failed to build DevKiller")?;

            match dk.resume(&session_id).await {
                Ok(handle) => {
                    let output = handle.output().await.context("resume execution failed")?;
                    println!("\n{}", output);
                }
                Err(e) => {
                    error!(error = %e, "resume failed");
                    anyhow::bail!("resume failed: {}", e);
                }
            }
        }

        Commands::Sessions { status } => {
            let storage = SqliteStorage::default_location()
                .context("failed to initialize session storage")?;

            let sessions = storage.list().await?;

            // Parse status filter if provided
            let status_filter = if let Some(ref s) = status {
                Some(
                    s.parse::<SessionStatus>()
                        .with_context(|| format!("invalid status filter: {}", s))?,
                )
            } else {
                None
            };

            if sessions.is_empty() {
                println!("No sessions found.");
                return Ok(());
            }

            println!("{:<10} {:<12} {:<12} TASK", "ID", "STATUS", "PHASE");
            println!("{}", "-".repeat(70));

            for session in sessions {
                if let Some(filter_status) = status_filter {
                    if session.status != filter_status {
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

        Commands::ExportSession { session_id, output } => {
            let storage = SqliteStorage::default_location()
                .context("failed to initialize session storage")?;

            let session = storage
                .load(&session_id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("session not found: {}", session_id))?;

            info!(session_id = %session_id, "exporting session");
            let portable = PortableSession::from_session(&session);

            let output_path = output.unwrap_or_else(|| format!("{}.json", session_id));
            let json =
                serde_json::to_string_pretty(&portable).context("failed to serialize session")?;

            std::fs::write(&output_path, json)
                .with_context(|| format!("failed to write to {}", output_path))?;

            println!("Exported session {} to {}", session_id, output_path);
        }

        Commands::ImportSession { file, working_dir } => {
            let storage = SqliteStorage::default_location()
                .context("failed to initialize session storage")?;

            let json = std::fs::read_to_string(&file)
                .with_context(|| format!("failed to read {}", file))?;

            let portable: PortableSession =
                serde_json::from_str(&json).context("failed to parse session JSON")?;

            let original_id = portable.original_id.clone();
            let session = portable.into_session(working_dir);
            let new_id = session.id.clone();

            storage
                .save(&session)
                .await
                .context("failed to save imported session")?;

            info!(new_id = %new_id, original_id = %original_id, "imported session");
            println!("Imported session as {} (ready to resume)", new_id);
        }

        Commands::Hello => {
            println!("HELLO");
        }

        Commands::Pipeline => {
            println!("PIPELINE");
        }
    }

    Ok(())
}
