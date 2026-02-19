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

## Smoke Tests

End-to-end verification tests organized by implementation phase. After making changes to the codebase, re-run the smoke tests for any phase whose functionality you touched to make sure existing behavior still works. For example, if you modify the shell tool, re-run the Phase 2 tests. If you change the LLM provider layer, re-run Phase 1 and Phase 5 tests. When in doubt, run all of them.

When adding new features, add corresponding smoke tests here. When modifying existing behavior, update any affected smoke tests to match the new expected behavior. Keep each phase between 1 and 5 tests.

All require a `cargo build` first. Tests marked with (API key) require `ANTHROPIC_API_KEY` or `OPENAI_API_KEY` set.

### Phase 1: Core Infrastructure (LLM + File Tools + Basic Agent)

1. Basic LLM round-trip (API key):
   ```bash
   cargo run -- run --simple "respond with only the word HELLO"
   ```
2. Agent can read files (API key):
   ```bash
   cargo run -- run --simple "read the file src/lib.rs and tell me the first function name you see"
   ```
3. Agent can write and read files together (API key):
   ```bash
   cargo run -- run --simple "write a file called /tmp/dev-killer-test.txt containing 'smoke test passed' then read it back and tell me what it says"
   ```

### Phase 2: Tool Expansion & Shell (Shell + Search + Policy)

1. Shell tool executes commands (API key):
   ```bash
   cargo run -- run --simple "run the command 'echo hello world' using the shell tool and tell me the output"
   ```
2. Glob search works (API key):
   ```bash
   cargo run -- run --simple "use the glob tool to find all .rs files in the src/ directory and count them"
   ```
3. Grep search works (API key):
   ```bash
   cargo run -- run --simple "use the grep tool to search for 'fn main' in src/main.rs and show me the match"
   ```
4. Policy blocks dangerous commands (API key):
   ```bash
   cargo run -- run --simple "run the command 'sudo rm -rf /' and tell me what happened"
   ```

### Phase 3: Multi-Agent Orchestration (Orchestrator Pipeline)

1. Full orchestrator pipeline - plan, code, test, review (API key):
   ```bash
   cargo run -- run "read src/lib.rs and summarize what it does"
   ```
2. Agent can edit files in a multi-step task (API key):
   ```bash
   cargo run -- run --simple "edit src/lib.rs to add a comment '// smoke test' at the top of the file, then read it to confirm the comment is there"
   ```

### Phase 4: Persistence & Sessions (SQLite + Resume)

1. Session is created and listed (API key):
   ```bash
   cargo run -- run --save-session --simple "respond with only the word HELLO"
   cargo run -- sessions
   ```
2. Session filtering by status works:
   ```bash
   cargo run -- sessions --status completed
   ```
3. Session deletion works:
   ```bash
   cargo run -- delete-session <session-id>
   cargo run -- sessions
   ```

### Phase 5: Multi-Provider & Polish (OpenAI + Config + Logging)

1. OpenAI provider works end-to-end (OPENAI_API_KEY):
   ```bash
   cargo run -- --provider openai run --simple "respond with only the word HELLO"
   ```
2. Verbose logging produces debug output (API key):
   ```bash
   cargo run -- -v run --simple "respond with only the word HELLO"
   ```

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
