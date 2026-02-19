# dev-killer

Autonomous coding agent platform built in Rust (edition 2024).

## Build & Test

```bash
cargo build              # Debug build
cargo build --release    # Release build
cargo test               # Run all tests
cargo test <name>        # Run specific test
cargo clippy             # Lint
cargo fmt                # Format
```

## Before Committing

1. `cargo fmt`
2. `cargo clippy -- -D warnings`
3. `cargo test`

## Project Conventions

- **Error handling**: Use `anyhow` with `.context()` for meaningful errors
- **Logging**: Use `tracing` macros (`info!`, `debug!`, `warn!`, `error!`)
- **Async runtime**: `tokio` - never block in async code
- **CLI**: `clap` with derive API

## Architecture Overview

### Core Components

| Module | Purpose |
|--------|---------|
| `agents/` | Agent implementations (orchestrator, planner, coder, tester, reviewer) |
| `llm/` | LLM abstraction (providers, messages, tool calls, retry logic) |
| `tools/` | Tool implementations (file ops, shell, search) |
| `session/` | Session persistence (SQLite storage) |
| `config/` | Configuration loading and security policies |
| `runtime/` | Agent execution loop |

### Agent Hierarchy

```
OrchestratorAgent
├── PlannerAgent    (creates implementation plan)
├── CoderAgent      (implements changes)
├── TesterAgent     (runs tests)
└── ReviewerAgent   (validates completion)
```

### Key Traits

- `Agent`: Core agent interface (`system_prompt()`, `run()`)
- `LlmProvider`: LLM abstraction (`chat()`)
- `Tool`: Tool interface (`name()`, `description()`, `schema()`, `execute()`)
- `Storage`: Session persistence (`save()`, `load()`, `list()`, `delete()`)

## Common Patterns

### Adding a New Tool

```rust
// In src/tools/your_tool.rs
use super::Tool;

pub struct YourTool;

#[async_trait]
impl Tool for YourTool {
    fn name(&self) -> &'static str { "your_tool" }
    fn description(&self) -> &'static str { "What it does" }
    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": { /* ... */ },
            "required": ["param1"]
        })
    }
    async fn execute(&self, params: serde_json::Value) -> Result<String> {
        // Implementation
    }
}

// Register in main.rs create_tool_registry()
registry.register(YourTool);
```

### Adding a New Agent

```rust
// In src/agents/your_agent.rs
use super::Agent;

pub struct YourAgent;

#[async_trait]
impl Agent for YourAgent {
    fn system_prompt(&self) -> String {
        "You are a specialized agent that...".to_string()
    }

    async fn run(
        &self,
        task: &str,
        provider: &dyn LlmProvider,
        tools: &ToolRegistry,
    ) -> Result<String> {
        // Agent loop implementation
    }
}
```

## Security Considerations

- **Path validation**: All file tools validate paths against policy allow/deny lists
- **Shell commands**: Validated against command allow/deny lists
- **Sensitive paths**: `/etc/`, `/private/etc/` (macOS) blocked by default
- **Dangerous commands**: `rm -rf /`, `sudo` blocked by default

## Configuration Precedence

1. CLI arguments (highest)
2. Environment variables (`DEV_KILLER_*`)
3. Project config (`dev-killer.toml`)
4. Global config (`~/.config/dev-killer/config.toml`)
5. Defaults (lowest)

## Testing Strategy

- Unit tests: In same file with `#[cfg(test)]` module
- Test file operations with `tempfile` crate
- Test async code with `#[tokio::test]`
- Use descriptive test names: `test_<function>_<scenario>_<expected>`

## Debugging

```bash
# Verbose logging
RUST_LOG=debug cargo run -- -v run --simple "task"

# Specific module logging
RUST_LOG=dev_killer::agents=debug cargo run -- run "task"
```

## Key Files

| File | Description |
|------|-------------|
| `src/main.rs` | CLI entry point, command dispatch |
| `src/lib.rs` | Public API exports |
| `src/agents/orchestrator.rs` | Multi-agent coordination |
| `src/agents/coder.rs` | Main coding agent with tool loop |
| `src/llm/provider.rs` | LLM provider trait |
| `src/llm/anthropic.rs` | Anthropic/OpenAI implementations |
| `src/tools/registry.rs` | Tool registration and lookup |
| `src/session/sqlite.rs` | SQLite session storage |
| `src/config/project.rs` | Configuration loading |
