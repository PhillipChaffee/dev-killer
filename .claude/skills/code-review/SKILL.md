---
name: rust-code-review
description: "Rust code review: check correctness, safety, performance, and idiomatic patterns using cargo clippy/test/fmt"
allowed-tools: Read, Grep, Glob, Bash(cargo clippy *, cargo fmt *, cargo test *)
argument-hint: [file or directory path]
---

# Rust Code Review

Perform a comprehensive code review of the Rust code at: $ARGUMENTS

If no path specified, review all changed files (`git diff --name-only`).

## Review Checklist

### 1. Correctness
- Logic errors or bugs
- Edge cases not handled
- Incorrect error handling
- Missing null/None checks

### 2. Safety
- Unsafe code blocks - are they necessary and sound?
- Potential panics (unwrap, expect, indexing)
- Race conditions or data races
- Memory safety concerns

### 3. Error Handling
- Using `?` operator appropriately
- Meaningful error context with `.context()`
- No silent error swallowing
- Proper Result/Option usage

### 4. Performance
- Unnecessary clones or allocations
- Using references where possible
- Appropriate data structures (Vec vs HashMap vs BTreeMap)
- Iterator usage vs manual loops

### 5. Idiomatic Rust
- Following Rust naming conventions
- Using standard library effectively
- Pattern matching vs if-let
- Derive macros used appropriately

### 6. Code Organization
- Module structure makes sense
- Public API is minimal and intentional
- Related code is grouped together
- No circular dependencies

### 7. Documentation
- Public items have doc comments
- Complex logic is explained
- Examples in doc comments where helpful

### 8. Testing
- Adequate test coverage
- Tests are meaningful (not just for coverage)
- Edge cases tested
- Integration tests where appropriate

## Automated Checks

Run these commands and report any issues:

1. `cargo clippy -- -D warnings` - Linter warnings
2. `cargo fmt --check` - Formatting issues
3. `cargo test` - Test failures

## Output Format

For each issue found, report:
- **File**: path/to/file.rs
- **Line**: line number(s)
- **Severity**: Critical / Warning / Suggestion
- **Category**: Which checklist item
- **Issue**: Description of the problem
- **Fix**: Suggested resolution

End with a summary:
- Total issues by severity
- Overall assessment (PASS / NEEDS FIXES / CRITICAL ISSUES)
