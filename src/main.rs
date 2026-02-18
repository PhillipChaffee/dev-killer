use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

use dev_killer::{
    AnthropicProvider, CoderAgent, EditFileTool, Executor, LlmProvider, OpenAIProvider,
    ReadFileTool, ToolRegistry, WriteFileTool,
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
    registry.register(ReadFileTool);
    registry.register(WriteFileTool);
    registry.register(EditFileTool);
    registry
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    init_logging(cli.verbose);

    match cli.command {
        Commands::Run { task } => {
            info!(provider = %cli.provider, "starting task");

            let provider = create_provider(&cli.provider, cli.model.as_deref())
                .context("failed to create LLM provider")?;

            let tools = create_tool_registry();
            let executor = Executor::new(tools);
            let agent = CoderAgent::new();

            match executor.run(&agent, &task, provider.as_ref()).await {
                Ok(result) => {
                    println!("\n{}", result);
                }
                Err(e) => {
                    error!(error = %e, "task failed");
                    std::process::exit(1);
                }
            }
        }
    }

    Ok(())
}
