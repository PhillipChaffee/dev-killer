# dev-killer

[![CI](https://github.com/PhillipChaffee/dev-killer/actions/workflows/ci.yml/badge.svg)](https://github.com/PhillipChaffee/dev-killer/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/PhillipChaffee/dev-killer/graph/badge.svg)](https://codecov.io/gh/PhillipChaffee/dev-killer)
[![Rust](https://img.shields.io/badge/Rust-1.85%2B-orange.svg)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://opensource.org/licenses/MIT)

An autonomous coding agent platform built in Rust. Designed as a foundation for building AI-powered coding tools and services.

## Features

- **Multi-Agent Orchestration**: Hierarchical agent system with specialized roles (Planner, Coder, Tester, Reviewer)
- **Multi-Provider LLM Support**: Works with Anthropic Claude and OpenAI GPT models
- **Comprehensive Tool Suite**: File operations, shell execution, glob/grep search
- **Session Persistence**: SQLite-backed sessions that survive restarts
- **Configurable Security Policies**: Allow/deny lists for paths and commands
- **Retry with Backoff**: Automatic retry for transient API failures

## Installation

### Prerequisites

- Rust 1.85+ (edition 2024)
- An API key for Anthropic or OpenAI

### Build from Source

```bash
git clone https://github.com/PhillipChaffee/dev-killer.git
cd dev-killer
cargo build --release
```

The binary will be at `target/release/dev-killer`.

## Quick Start

### Set up API key

```bash
export ANTHROPIC_API_KEY="your-api-key"
# or
export OPENAI_API_KEY="your-api-key"
```

### Run a task

```bash
# Simple mode (single coder agent)
dev-killer run --simple "add a hello world function to src/lib.rs"

# Orchestrator mode (planner -> coder -> tester -> reviewer)
dev-killer run "implement a fibonacci function with tests"
```

### With session persistence

```bash
# Save session for later resume
dev-killer run --save-session "refactor the authentication module"

# List sessions
dev-killer sessions

# Resume an interrupted session
dev-killer resume <session-id>
```

## Usage

```
dev-killer [OPTIONS] <COMMAND>

Commands:
  run             Run a task
  resume          Resume a previously interrupted session
  sessions        List saved sessions
  delete-session  Delete a session

Options:
  -v, --verbose          Enable verbose output
      --provider <NAME>  LLM provider (anthropic, openai) [default: anthropic]
      --model <MODEL>    Model to use (provider-specific)
  -h, --help             Print help
```

### Run Options

```
dev-killer run [OPTIONS] <TASK>

Arguments:
  <TASK>  The task to perform

Options:
      --simple        Use simple mode (single coder agent)
      --save-session  Save session for later resume
```

## Configuration

dev-killer uses a cascading configuration system with the following precedence (highest to lowest):

1. Command-line arguments
2. Environment variables (`DEV_KILLER_*`)
3. Project config (`dev-killer.toml` in project directory)
4. Global config (`~/.config/dev-killer/config.toml`)
5. Defaults

### Configuration File

Create a `dev-killer.toml` in your project root:

```toml
# LLM settings
provider = "anthropic"
model = "claude-sonnet-4-20250514"

# Retry settings
max_retries = 3
retry_delay_ms = 1000

# Default modes
simple_mode = false
save_sessions = true

# Security policy
[policy]
allow_paths = ["src/**", "tests/**", "Cargo.toml"]
deny_paths = [".env", "secrets/**", "/etc/**"]
allow_commands = ["cargo *", "git *", "rustfmt"]
deny_commands = ["rm -rf /", "sudo *"]
```

### Environment Variables

| Variable | Description |
|----------|-------------|
| `ANTHROPIC_API_KEY` | Anthropic API key |
| `OPENAI_API_KEY` | OpenAI API key |
| `DEV_KILLER_PROVIDER` | Override LLM provider |
| `DEV_KILLER_MODEL` | Override model |
| `DEV_KILLER_MAX_RETRIES` | Max retry attempts |
| `DEV_KILLER_SIMPLE_MODE` | Enable simple mode |
| `DEV_KILLER_SAVE_SESSIONS` | Enable session saving |

## Architecture

```
src/
├── main.rs              # CLI entry point
├── lib.rs               # Library exports
├── agents/              # Agent implementations
│   ├── orchestrator.rs  # Coordinates specialized agents
│   ├── planner.rs       # Creates implementation plans
│   ├── coder.rs         # Implements code changes
│   ├── tester.rs        # Runs tests, validates behavior
│   └── reviewer.rs      # Reviews changes, approves completion
├── config/              # Configuration system
│   ├── project.rs       # Config loading with precedence
│   └── policy.rs        # Security policies
├── llm/                 # LLM abstraction layer
│   ├── provider.rs      # Provider trait + implementations
│   ├── anthropic.rs     # Anthropic/OpenAI providers
│   ├── message.rs       # Message types
│   ├── retry.rs         # Retry with exponential backoff
│   └── tool_call.rs     # Tool call parsing
├── tools/               # Tool implementations
│   ├── file.rs          # Read, Write, Edit
│   ├── shell.rs         # Bash execution
│   ├── search.rs        # Glob, Grep
│   └── registry.rs      # Tool registration
├── session/             # Session persistence
│   ├── state.rs         # Session state types
│   ├── storage.rs       # Storage trait
│   └── sqlite.rs        # SQLite backend
└── runtime/             # Execution runtime
    └── executor.rs      # Agent execution loop
```

### Agent Workflow

In orchestrator mode, tasks flow through four phases:

1. **Planning**: Planner agent analyzes the task and creates an implementation plan
2. **Implementation**: Coder agent implements the changes according to the plan
3. **Testing**: Tester agent runs tests and validates behavior
4. **Review**: Reviewer agent checks completion; may trigger fixes and re-testing

The review phase can iterate up to 3 times before requiring manual intervention.

### Available Tools

| Tool | Description |
|------|-------------|
| `read_file` | Read file contents |
| `write_file` | Write/create files |
| `edit_file` | Make targeted edits to files |
| `shell` | Execute shell commands |
| `glob` | Find files by pattern |
| `grep` | Search file contents with regex |

## Development

### Build & Test

```bash
cargo build              # Debug build
cargo build --release    # Release build
cargo test               # Run all tests
cargo clippy             # Lint
cargo fmt                # Format
```

### Before Committing

```bash
cargo fmt
cargo clippy -- -D warnings
cargo test
```

## Using as a Library

dev-killer can be used as a Rust library:

```rust
use dev_killer::{
    CoderAgent, Executor, ToolRegistry, AnthropicProvider,
    ReadFileTool, WriteFileTool, EditFileTool, ShellTool,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Create provider
    let provider = AnthropicProvider::sonnet()?;

    // Set up tools
    let mut tools = ToolRegistry::new();
    tools.register(ReadFileTool);
    tools.register(WriteFileTool);
    tools.register(EditFileTool);
    tools.register(ShellTool);

    // Create executor and run
    let executor = Executor::new(tools);
    let agent = CoderAgent::new();

    let result = executor
        .run(&agent, "add a hello function to src/lib.rs", &provider)
        .await?;

    println!("{}", result);
    Ok(())
}
```

## License

MIT

## Contributing

Contributions welcome! Please run the test suite and linter before submitting PRs.
