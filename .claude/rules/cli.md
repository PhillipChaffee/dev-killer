# CLI Development

## Framework

Use `clap` with derive API:

```rust
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "dev-killer")]
#[command(about = "Brief description", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Enable verbose output
    #[arg(short, long, global = true)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Do something
    Run {
        #[arg(short, long)]
        config: Option<PathBuf>,
    },
}
```

## Guidelines

- Keep `main()` minimal - parse args and delegate
- Use subcommands for distinct operations (like git/cargo)
- Provide short (`-v`) and long (`--verbose`) flags
- Include helpful descriptions in doc comments
- Support `--help` and `--version` (automatic with clap)

## User Experience

- Respect `NO_COLOR` environment variable
- Use exit codes: 0 for success, 1 for errors
- Write errors to stderr, output to stdout
- Provide progress feedback for long operations

## Configuration

Prefer this precedence (highest to lowest):
1. Command-line arguments
2. Environment variables
3. Config file
4. Defaults
