# Rust Code Style

## General

- Follow Rust 2024 edition idioms
- Use `rustfmt` defaults - don't fight the formatter
- Run `cargo clippy` and address warnings before committing

## Naming

- Types: `PascalCase`
- Functions, variables, modules: `snake_case`
- Constants: `SCREAMING_SNAKE_CASE`
- Avoid abbreviations unless domain-standard

## Imports

- Group imports: std, external crates, internal modules
- Use specific imports over glob imports (`use std::collections::HashMap` not `use std::collections::*`)
- Prefer `use crate::` for internal imports

## Types

- Prefer `&str` over `String` in function parameters when ownership isn't needed
- Use `impl Trait` in return position for iterators and closures
- Derive common traits in this order: `Debug, Clone, Copy, PartialEq, Eq, Hash, Default`

## Comments

- Doc comments (`///`) for public API
- Regular comments (`//`) sparingly - prefer self-documenting code
- Use `//!` for module-level documentation
