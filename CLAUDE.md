# dev-killer

Rust project using edition 2024.

## Build & Test

```bash
cargo build              # Debug build
cargo build --release    # Release build
cargo test               # Run all tests
cargo test <name>        # Run specific test
cargo clippy             # Lint
cargo fmt                # Format
```

## Project Conventions

- Use `anyhow` for error handling (this is an application, not a library)
- Use `tracing` for logging, not `log`
- Async runtime: `tokio`
- CLI parsing: `clap` with derive API

## Before Committing

1. `cargo fmt`
2. `cargo clippy -- -D warnings`
3. `cargo test`
