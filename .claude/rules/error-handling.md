# Error Handling

## Strategy

This is an application, not a library. Use `anyhow` for error handling.

```rust
use anyhow::{Context, Result};

fn do_something() -> Result<()> {
    let data = std::fs::read_to_string("config.toml")
        .context("failed to read config file")?;
    Ok(())
}
```

## Guidelines

- Use `?` operator liberally - propagate errors up
- Add context with `.context()` or `.with_context(|| ...)` at meaningful boundaries
- Reserve `unwrap()` and `expect()` for:
  - Tests
  - Cases where failure is truly impossible (document why)
  - Early startup before error handling is established

## Panics

- Never panic in library code paths
- `panic!` is acceptable for unrecoverable programmer errors (invariant violations)
- Prefer `expect("reason")` over `unwrap()` when panic is intentional

## Option Handling

- Use `ok_or_else()` to convert `Option` to `Result` with context
- Prefer `if let Some(x)` over `.unwrap()` when you can handle the None case
