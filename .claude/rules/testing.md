# Testing

## Structure

- Unit tests: in same file with `#[cfg(test)]` module
- Integration tests: in `tests/` directory
- Test utilities: in `tests/common/mod.rs`

## Naming

Use descriptive names that explain the scenario:

```rust
// Good
#[test]
fn parse_config_returns_error_when_file_missing() { }

// Bad
#[test]
fn test_parse() { }
```

## Guidelines

- Test behavior, not implementation
- One assertion per test when practical
- Test edge cases: empty input, boundary conditions, error paths
- Use `#[should_panic(expected = "...")]` for panic tests
- Use `assert_eq!` over `assert!` for better error messages

## Async Tests

```rust
#[tokio::test]
async fn async_operation_succeeds() {
    let result = some_async_fn().await;
    assert!(result.is_ok());
}
```

## Test Organization

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_test_here() { }
}
```
