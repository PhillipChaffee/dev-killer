use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::{Value, json};
use std::process::Stdio;
use tokio::process::Command;
use tokio::time::{Duration, timeout};

use super::Tool;
use super::validate_path;
use crate::config::Policy;

const DEFAULT_TIMEOUT_SECS: u64 = 120;
const MAX_TIMEOUT_SECS: u64 = 300;
const MAX_OUTPUT_BYTES: usize = 100_000;

/// Find the largest byte index <= `index` that is a valid char boundary.
fn floor_char_boundary(s: &str, index: usize) -> usize {
    if index >= s.len() {
        return s.len();
    }
    let mut i = index;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Tool for executing shell commands
pub struct ShellTool {
    pub policy: Policy,
}

#[async_trait]
impl Tool for ShellTool {
    fn name(&self) -> &str {
        "shell"
    }

    fn description(&self) -> &str {
        "Execute a shell command and return the output. Use for running builds, tests, git commands, etc."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The shell command to execute"
                },
                "working_dir": {
                    "type": "string",
                    "description": "Optional working directory for the command"
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Optional timeout in seconds (default: 120, max: 300)"
                }
            },
            "required": ["command"]
        })
    }

    async fn execute(&self, params: Value) -> Result<String> {
        let command = params["command"]
            .as_str()
            .context("missing 'command' parameter")?;

        let working_dir = params["working_dir"].as_str();
        let timeout_secs = params["timeout_secs"]
            .as_u64()
            .unwrap_or(DEFAULT_TIMEOUT_SECS)
            .min(MAX_TIMEOUT_SECS);

        // Validate command for dangerous patterns
        validate_command(command, &self.policy)?;

        // Validate working directory if provided
        if let Some(dir) = working_dir {
            validate_path(dir, &self.policy)?;
        }

        // Build the command
        let mut cmd = Command::new("bash");
        cmd.arg("-c").arg(command);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        cmd.kill_on_drop(true);

        if let Some(dir) = working_dir {
            cmd.current_dir(dir);
        }

        // Spawn and wait with timeout — kill_on_drop ensures the child is
        // killed if the future is dropped (e.g. on timeout)
        let child = cmd
            .spawn()
            .with_context(|| format!("failed to spawn command: {}", command))?;

        let output =
            match timeout(Duration::from_secs(timeout_secs), child.wait_with_output()).await {
                Ok(result) => {
                    result.with_context(|| format!("failed to execute command: {}", command))?
                }
                Err(_) => {
                    anyhow::bail!("command timed out after {} seconds", timeout_secs);
                }
            };

        // Collect output
        let mut result = String::new();

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if !stdout.is_empty() {
            result.push_str(&stdout);
        }

        if !stderr.is_empty() {
            if !result.is_empty() {
                result.push_str("\n--- stderr ---\n");
            }
            result.push_str(&stderr);
        }

        // Add exit status
        if !output.status.success() {
            let code = output.status.code().unwrap_or(-1);
            result.push_str(&format!("\n[exit code: {}]", code));
        }

        // Truncate if too long (find nearest char boundary to avoid panic)
        if result.len() > MAX_OUTPUT_BYTES {
            let boundary = floor_char_boundary(&result, MAX_OUTPUT_BYTES);
            result.truncate(boundary);
            result.push_str("\n... [output truncated]");
        }

        if result.is_empty() {
            result = "[no output]".to_string();
        }

        Ok(result)
    }
}

/// Validate command for dangerous patterns
fn validate_command(command: &str, policy: &Policy) -> Result<()> {
    // Check policy deny_commands
    let command_lower = command.to_lowercase();
    for denied in &policy.deny_commands {
        if command_lower.contains(&denied.to_lowercase()) {
            anyhow::bail!("command '{}' is denied by policy", denied);
        }
    }

    // Deny list of dangerous command patterns
    let dangerous_patterns = [
        "rm -rf /",
        "rm -rf /*",
        "rm -rf ~",
        "rm -rf $HOME",
        ":(){:|:&};:", // Fork bomb
        "mkfs.",
        "dd if=/dev/zero",
        "dd if=/dev/random",
        "> /dev/sda",
        "chmod -R 777 /",
        "chown -R",
        "sudo rm",
        "sudo dd",
        "sudo mkfs",
    ];

    for pattern in &dangerous_patterns {
        if command_lower.contains(&pattern.to_lowercase()) {
            anyhow::bail!("command contains dangerous pattern: {}", pattern);
        }
    }

    // Check for attempts to read sensitive files via shell commands
    validate_sensitive_paths(command)?;

    Ok(())
}

/// Check if command attempts to access sensitive file paths
fn validate_sensitive_paths(command: &str) -> Result<()> {
    // Sensitive path prefixes to block
    // Note: On macOS, /etc is a symlink to /private/etc
    let sensitive_paths = [
        "/etc/",
        "/private/etc/",
        "/proc/",
        "/sys/",
        "/dev/",
        "/var/log/",
        "~/.ssh",
        "$HOME/.ssh",
        "~/.gnupg",
        "$HOME/.gnupg",
        "~/.aws",
        "$HOME/.aws",
        "~/.config",
        "$HOME/.config",
    ];

    // Commands that read file contents
    let read_commands = [
        "cat ", "head ", "tail ", "less ", "more ", "vim ", "nano ", "vi ",
    ];

    // Check if command contains a read command followed by a sensitive path
    for sensitive in &sensitive_paths {
        if command.contains(sensitive) {
            // Check if this is a file-reading command
            for read_cmd in &read_commands {
                if command.contains(read_cmd) {
                    anyhow::bail!(
                        "access to sensitive path {} via shell is not allowed",
                        sensitive
                    );
                }
            }
            // Also check for redirects or pipes that could read from these paths
            if command.contains(&format!("< {}", sensitive))
                || command.contains(&format!("<{}", sensitive))
            {
                anyhow::bail!(
                    "access to sensitive path {} via shell is not allowed",
                    sensitive
                );
            }
        }
    }

    // Also check for .env files
    if command.contains(".env") {
        for read_cmd in &read_commands {
            if command.contains(read_cmd) {
                anyhow::bail!("access to .env files via shell is not allowed");
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_policy() -> Policy {
        Policy::default()
    }

    #[test]
    fn validate_safe_commands() {
        let policy = default_policy();
        assert!(validate_command("ls -la", &policy).is_ok());
        assert!(validate_command("cargo build", &policy).is_ok());
        assert!(validate_command("git status", &policy).is_ok());
        assert!(validate_command("echo hello", &policy).is_ok());
    }

    #[test]
    fn validate_dangerous_commands() {
        let policy = default_policy();
        assert!(validate_command("rm -rf /", &policy).is_err());
        assert!(validate_command("sudo rm -rf /tmp", &policy).is_err());
        assert!(validate_command("dd if=/dev/zero of=/dev/sda", &policy).is_err());
    }

    #[test]
    fn validate_sensitive_path_access() {
        let policy = default_policy();
        // Should block reading /etc files
        assert!(validate_command("cat /etc/passwd", &policy).is_err());
        assert!(validate_command("head /etc/shadow", &policy).is_err());
        assert!(validate_command("tail /etc/hosts", &policy).is_err());

        // Should block reading ~/.ssh
        assert!(validate_command("cat ~/.ssh/id_rsa", &policy).is_err());
        assert!(validate_command("cat $HOME/.ssh/config", &policy).is_err());

        // Should block reading .env files
        assert!(validate_command("cat .env", &policy).is_err());
        assert!(validate_command("cat .env.local", &policy).is_err());

        // Should allow non-reading commands in general
        assert!(validate_command("ls /etc", &policy).is_ok());
        assert!(validate_command("ls -la", &policy).is_ok());
    }

    #[test]
    fn validate_policy_deny_commands() {
        let policy = Policy {
            deny_commands: vec!["git push".to_string()],
            ..Policy::default()
        };
        assert!(validate_command("git push origin main", &policy).is_err());
        assert!(validate_command("git status", &policy).is_ok());
    }

    #[test]
    fn validate_nested_shell_dangerous() {
        let policy = default_policy();
        // Nested shell with dangerous command
        assert!(validate_command("bash -c 'rm -rf /'", &policy).is_err());
    }

    #[test]
    fn validate_fork_bomb() {
        let policy = default_policy();
        assert!(validate_command(":(){:|:&};:", &policy).is_err());
    }

    #[test]
    fn validate_redirect_sensitive_path() {
        let policy = default_policy();
        assert!(validate_command("python < /etc/passwd", &policy).is_err());
    }
}
