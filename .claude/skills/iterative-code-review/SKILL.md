---
name: iterative-code-review
description: Run code review and automatically fix issues in a loop until all issues are resolved
allowed-tools: Read, Grep, Glob, Edit, Write, Bash, Task
argument-hint: [file or directory path]
---

# Iterative Code Review

Perform iterative code review and fixes on: $ARGUMENTS

If no path specified, review all changed files.

## Process

Repeat the following cycle until no issues remain (max 5 iterations):

### Step 1: Run Code Review (Subagent)

Launch a code review subagent:
```
Use the Task tool with subagent_type="general-purpose" to run:
"Run /code-review on [target path]. Return ONLY a structured list of issues found,
or 'NO ISSUES FOUND' if the code passes all checks."
```

### Step 2: Evaluate Results

If subagent returns "NO ISSUES FOUND":
- Report success and exit loop

If subagent returns issues:
- Parse the issue list
- Prioritize: Critical > Warning > Suggestion
- Group by file for efficient fixing

### Step 3: Fix Issues (Subagent)

For each file with issues, launch a fix subagent:
```
Use the Task tool with subagent_type="general-purpose" to run:
"Fix the following issues in [file]:
[list of issues for this file]

Make minimal, targeted changes. Do not refactor unrelated code.
After fixing, run 'cargo check' to verify the file compiles."
```

### Step 4: Verify Fixes

Run:
- `cargo check` - Compilation
- `cargo clippy -- -D warnings` - Linter
- `cargo test` - Tests still pass

If verification fails, include the errors in the next iteration.

### Step 5: Report Progress

After each iteration, report:
- Iteration number
- Issues fixed this round
- Issues remaining
- Any new issues introduced

## Exit Conditions

Stop iterating when:
1. No issues found (SUCCESS)
2. Max iterations reached (5) - report remaining issues
3. Same issues persist for 2 iterations (STUCK) - escalate to user

## Final Report

```
## Iterative Code Review Complete

**Target**: [path]
**Iterations**: X
**Status**: SUCCESS / PARTIAL / STUCK

### Issues Fixed
- [list of resolved issues]

### Remaining Issues (if any)
- [list with explanations why not auto-fixed]

### Changes Made
- [list of files modified]
```
