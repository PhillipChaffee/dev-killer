# Contributing to dev-killer

Thanks for your interest in contributing! This guide will help you get set up and submit a quality pull request.

## Prerequisites

- Rust 1.85+ (edition 2024)
- An `ANTHROPIC_API_KEY` for running smoke tests (and optionally an `OPENAI_API_KEY` for Phase 5)

## Getting Started

```bash
git clone https://github.com/PhillipChaffee/dev-killer.git
cd dev-killer
cargo build
```

## Before Submitting a PR

### 1. Format, lint, and run unit tests

```bash
cargo fmt
cargo clippy -- -D warnings
cargo test
```

All three must pass with no warnings or failures.

### 2. Run the smoke tests

Smoke tests are manual end-to-end tests that verify the full agent pipeline against a real LLM. Run the smoke tests for any phase whose functionality your changes touch. When in doubt, run all of them.

Each smoke test requires `cargo build` first and an `ANTHROPIC_API_KEY` set in your environment.

#### Phase 1: Core Infrastructure (LLM + File Tools + Basic Agent)

```bash
cargo run -- run --simple "respond with only the word HELLO"
cargo run -- run --simple "read the file src/lib.rs and tell me the first function name you see"
cargo run -- run --simple "write a file called /tmp/dev-killer-test.txt containing 'smoke test passed' then read it back and tell me what it says"
```

#### Phase 2: Tool Expansion & Shell (Shell + Search + Policy)

```bash
cargo run -- run --simple "run the command 'echo hello world' using the shell tool and tell me the output"
cargo run -- run --simple "use the glob tool to find all .rs files in the src/ directory and count them"
cargo run -- run --simple "use the grep tool to search for 'fn main' in src/main.rs and show me the match"
cargo run -- run --simple "run the command 'sudo rm -rf /' and tell me what happened"
```

#### Phase 3: Multi-Agent Orchestration (Orchestrator Pipeline)

```bash
cargo run -- run "read src/lib.rs and summarize what it does"
cargo run -- run --simple "edit src/lib.rs to add a comment '// smoke test' at the top of the file, then read it to confirm the comment is there"
```

**Note:** Revert the `// smoke test` comment from `src/lib.rs` before committing.

#### Phase 4: Persistence & Sessions (SQLite + Resume)

```bash
cargo run -- run --save-session --simple "respond with only the word HELLO"
cargo run -- sessions
cargo run -- sessions --status completed
# Use a session ID from the output above:
cargo run -- delete-session <session-id>
cargo run -- sessions
```

#### Phase 5: Multi-Provider & Polish (OpenAI + Config + Logging)

Requires `OPENAI_API_KEY` for the first test.

```bash
cargo run -- --provider openai run --simple "respond with only the word HELLO"
cargo run -- -v run --simple "respond with only the word HELLO"
```

### 3. Include smoke test results in your PR

Paste the output of the smoke tests you ran into your PR description. This helps reviewers verify that the changes work end-to-end. Use a collapsible section to keep the PR tidy:

```markdown
<details>
<summary>Smoke test results</summary>

<!-- Paste your smoke test output here -->

</details>
```

You only need to include results for phases relevant to your changes. If your change touches the shell tool, include Phase 2 results. If it touches the LLM provider layer, include Phase 1 and Phase 5. If you're unsure, include all of them.

## What to Contribute

- Bug fixes
- New tools (see `CLAUDE.md` for the pattern)
- New agents
- Performance improvements
- Documentation improvements

## Code Style

- Run `cargo fmt` — don't fight the formatter
- Address all `cargo clippy` warnings
- Follow existing patterns in the codebase (see `CLAUDE.md` for conventions)
