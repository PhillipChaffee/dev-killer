# Async Patterns

## Runtime

Use `tokio` as the async runtime.

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ...
}
```

## Guidelines

- Never block in async code - use `tokio::task::spawn_blocking` for CPU-heavy or blocking operations
- Tasks only yield at `.await` points - keep work between awaits short
- Limit task spawning - don't spawn a task for every small operation
- Handle cancellation gracefully - dropping a future cancels it

## Common Patterns

```rust
// Concurrent operations
let (a, b) = tokio::join!(fetch_a(), fetch_b());

// With timeout
use tokio::time::{timeout, Duration};
let result = timeout(Duration::from_secs(5), some_async_op()).await?;

// Select first to complete
tokio::select! {
    result = operation_a() => { /* handle a */ }
    result = operation_b() => { /* handle b */ }
}
```

## Avoid

- `block_on` inside async context
- Holding locks across `.await` points (use `tokio::sync` types instead)
- Spawning unbounded tasks without backpressure
