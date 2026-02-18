# Logging with Tracing

## Setup

Use `tracing` and `tracing-subscriber`:

```rust
use tracing::{info, warn, error, debug, trace};
use tracing_subscriber::EnvFilter;

fn init_logging() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
}
```

## Log Levels

- `error!` - Operation failed, user action may be needed
- `warn!` - Something unexpected but recoverable
- `info!` - High-level operation status (start/complete)
- `debug!` - Detailed diagnostic information
- `trace!` - Very verbose, typically disabled

## Structured Logging

```rust
// Include structured fields
info!(user_id = %user.id, action = "login", "user logged in");

// Spans for operations
#[tracing::instrument]
async fn process_request(request_id: Uuid) -> Result<Response> {
    // All logs inside automatically include request_id
    info!("processing request");
    // ...
}
```

## Guidelines

- Use structured fields over string interpolation
- Add spans around significant operations
- Include correlation IDs for request tracing
- Configure via `RUST_LOG` env var: `RUST_LOG=debug,hyper=warn`
