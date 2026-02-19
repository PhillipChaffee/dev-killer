# Project Structure

## Layout

```
src/
├── main.rs              # CLI entry point, command dispatch
├── lib.rs               # Library root, public exports
├── agents/              # Agent implementations
│   ├── mod.rs           # Agent trait, exports
│   ├── message.rs       # Inter-agent message types
│   ├── orchestrator.rs  # Multi-agent coordination
│   ├── planner.rs       # Planning agent
│   ├── coder.rs         # Coding agent (main workhorse)
│   ├── tester.rs        # Testing agent
│   └── reviewer.rs      # Review agent
├── config/              # Configuration system
│   ├── mod.rs
│   ├── project.rs       # Config loading with precedence
│   └── policy.rs        # Security policy types
├── llm/                 # LLM abstraction layer
│   ├── mod.rs
│   ├── provider.rs      # LlmProvider trait
│   ├── anthropic.rs     # Anthropic/OpenAI implementations
│   ├── message.rs       # Message types (User, Assistant, ToolUse, ToolResult)
│   ├── tool_call.rs     # Tool call parsing
│   └── retry.rs         # Retry with exponential backoff
├── tools/               # Tool implementations
│   ├── mod.rs           # Tool trait
│   ├── registry.rs      # Tool registration and lookup
│   ├── file.rs          # ReadFile, WriteFile, EditFile
│   ├── shell.rs         # Shell command execution
│   └── search.rs        # Glob, Grep
├── session/             # Session persistence
│   ├── mod.rs
│   ├── state.rs         # SessionState, SessionStatus, SessionPhase
│   ├── storage.rs       # Storage trait
│   └── sqlite.rs        # SQLite implementation
└── runtime/             # Execution runtime
    ├── mod.rs
    └── executor.rs      # Agent execution loop
```

## Module Guidelines

- Keep `main.rs` thin - parse CLI args and delegate to library
- One module = one responsibility
- Expose minimal public API from each module
- Use `pub(crate)` for internal-only visibility
- Re-export key types in `mod.rs` files

## Key Traits

```rust
// Agent - core agent interface
trait Agent {
    fn system_prompt(&self) -> String;
    async fn run(&self, task: &str, provider: &dyn LlmProvider, tools: &ToolRegistry) -> Result<String>;
}

// LlmProvider - LLM abstraction
trait LlmProvider {
    async fn chat(&self, messages: &[Message], tools: &[ToolSchema]) -> Result<LlmResponse>;
}

// Tool - tool interface
trait Tool {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn schema(&self) -> serde_json::Value;
    async fn execute(&self, params: serde_json::Value) -> Result<String>;
}

// Storage - session persistence
trait Storage {
    async fn save(&self, session: &SessionState) -> Result<()>;
    async fn load(&self, id: &str) -> Result<Option<SessionState>>;
    async fn list(&self) -> Result<Vec<SessionSummary>>;
    async fn delete(&self, id: &str) -> Result<()>;
}
```

## Dependencies

Current dependencies (keep sorted):
- `anyhow` - Error handling
- `async-trait` - Async trait support
- `chrono` - Date/time handling
- `clap` - CLI parsing
- `glob` - File pattern matching
- `llm` - Multi-provider LLM support
- `regex` - Regular expressions
- `rusqlite` - SQLite database
- `serde`/`serde_json` - Serialization
- `tokio` - Async runtime
- `toml` - Config file parsing
- `tracing` - Logging
- `uuid` - Session IDs
