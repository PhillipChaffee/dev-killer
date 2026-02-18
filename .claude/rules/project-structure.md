# Project Structure

## Layout

```
src/
├── main.rs          # Entry point, CLI parsing, minimal logic
├── lib.rs           # Library root (if needed)
├── config.rs        # Configuration types and loading
├── commands/        # CLI subcommand implementations
│   ├── mod.rs
│   └── ...
├── core/            # Core domain logic
│   ├── mod.rs
│   └── ...
└── utils/           # Shared utilities
    ├── mod.rs
    └── ...
tests/
├── integration_test.rs
└── common/
    └── mod.rs       # Shared test utilities
```

## Module Guidelines

- Keep `main.rs` thin - delegate to library code
- One module = one responsibility
- Expose minimal public API from each module
- Use `pub(crate)` for internal-only visibility

## Dependencies

- Minimize dependency count
- Prefer well-maintained crates with recent updates
- Pin versions appropriately in `Cargo.toml`
- Use feature flags to reduce compilation scope

## Cargo.toml

```toml
[package]
name = "dev-killer"
version = "0.1.0"
edition = "2024"
rust-version = "1.85"

[dependencies]
# Keep sorted alphabetically

[dev-dependencies]
# Test-only dependencies

[profile.release]
lto = true
strip = true
```
