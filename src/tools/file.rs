use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

use super::Tool;

/// Validates a file path for security.
///
/// Performs the following checks:
/// 1. Canonicalizes the path to resolve symlinks and relative paths
/// 2. Rejects paths containing ".." traversal components
/// 3. Rejects paths to sensitive locations (/etc, ~/.ssh, .env files)
fn validate_path(path: &str) -> Result<PathBuf> {
    // Check for path traversal attempts before canonicalization
    if path.contains("..") {
        anyhow::bail!("path traversal detected: '..' is not allowed in paths");
    }

    // Canonicalize the path to resolve symlinks and relative components
    let canonical = std::fs::canonicalize(path)
        .or_else(|_| {
            // If the file doesn't exist yet (for write operations),
            // canonicalize the parent directory and append the filename
            let p = Path::new(path);
            if let (Some(parent), Some(file_name)) = (p.parent(), p.file_name()) {
                let parent_path = if parent.as_os_str().is_empty() {
                    Path::new(".")
                } else {
                    parent
                };
                let canonical_parent = std::fs::canonicalize(parent_path)?;
                Ok(canonical_parent.join(file_name))
            } else {
                anyhow::bail!("invalid path: {}", path)
            }
        })
        .with_context(|| format!("failed to resolve path: {}", path))?;

    let path_str = canonical.to_string_lossy();

    // Check for sensitive system directories
    // Note: On macOS, /etc is a symlink to /private/etc
    if path_str.starts_with("/etc/")
        || path_str == "/etc"
        || path_str.starts_with("/private/etc/")
        || path_str == "/private/etc"
    {
        anyhow::bail!("access to /etc is not allowed");
    }

    // Check for SSH directory
    if let Ok(home) = std::env::var("HOME") {
        let ssh_dir = Path::new(&home).join(".ssh");
        if canonical.starts_with(&ssh_dir) {
            anyhow::bail!("access to ~/.ssh is not allowed");
        }

        // Check for GPG directory
        let gnupg_dir = Path::new(&home).join(".gnupg");
        if canonical.starts_with(&gnupg_dir) {
            anyhow::bail!("access to ~/.gnupg is not allowed");
        }

        // Check for AWS credentials directory
        let aws_dir = Path::new(&home).join(".aws");
        if canonical.starts_with(&aws_dir) {
            anyhow::bail!("access to ~/.aws is not allowed");
        }

        // Check for config directory (may contain tokens)
        let config_dir = Path::new(&home).join(".config");
        if canonical.starts_with(&config_dir) {
            anyhow::bail!("access to ~/.config is not allowed");
        }
    }

    // Check for .git directories (could expose repo secrets via hooks or config)
    if path_str.contains("/.git/") || path_str.ends_with("/.git") {
        anyhow::bail!("access to .git directories is not allowed");
    }

    // Check for system pseudo-filesystems
    if path_str.starts_with("/proc/") || path_str == "/proc" {
        anyhow::bail!("access to /proc is not allowed");
    }
    if path_str.starts_with("/sys/") || path_str == "/sys" {
        anyhow::bail!("access to /sys is not allowed");
    }
    if path_str.starts_with("/dev/") || path_str == "/dev" {
        anyhow::bail!("access to /dev is not allowed");
    }

    // Check for system logs
    if path_str.starts_with("/var/log/") || path_str == "/var/log" {
        anyhow::bail!("access to /var/log is not allowed");
    }

    // Check for .env files
    if let Some(file_name) = canonical.file_name() {
        let name = file_name.to_string_lossy();
        if name == ".env" || name.starts_with(".env.") {
            anyhow::bail!("access to .env files is not allowed");
        }
    }

    Ok(canonical)
}

/// Tool for reading files
pub struct ReadFileTool;

#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        "Read the contents of a file at the given path"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "The path to the file to read"
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, params: Value) -> Result<String> {
        let path = params["path"]
            .as_str()
            .context("missing 'path' parameter")?;

        let validated_path = validate_path(path)?;

        let content = tokio::fs::read_to_string(&validated_path)
            .await
            .with_context(|| format!("failed to read file: {}", path))?;

        Ok(content)
    }
}

/// Tool for writing files
pub struct WriteFileTool;

#[async_trait]
impl Tool for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }

    fn description(&self) -> &str {
        "Write content to a file at the given path, creating parent directories if needed"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "The path to the file to write"
                },
                "content": {
                    "type": "string",
                    "description": "The content to write to the file"
                }
            },
            "required": ["path", "content"]
        })
    }

    async fn execute(&self, params: Value) -> Result<String> {
        let path = params["path"]
            .as_str()
            .context("missing 'path' parameter")?;
        let content = params["content"]
            .as_str()
            .context("missing 'content' parameter")?;

        // First validate the path to ensure it's not in a restricted location
        let validated_path = validate_path(path)?;

        // Create parent directories using the validated path, not the raw input
        if let Some(parent) = validated_path.parent() {
            if !parent.as_os_str().is_empty() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .with_context(|| format!("failed to create directory: {}", parent.display()))?;
            }
        }

        tokio::fs::write(&validated_path, content)
            .await
            .with_context(|| format!("failed to write file: {}", path))?;

        Ok(format!(
            "Successfully wrote {} bytes to {}",
            content.len(),
            path
        ))
    }
}

/// Tool for editing files (find and replace)
pub struct EditFileTool;

#[async_trait]
impl Tool for EditFileTool {
    fn name(&self) -> &str {
        "edit_file"
    }

    fn description(&self) -> &str {
        "Edit a file by replacing old_string with new_string. The old_string must be unique in the file."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "The path to the file to edit"
                },
                "old_string": {
                    "type": "string",
                    "description": "The string to find and replace (must be unique in the file)"
                },
                "new_string": {
                    "type": "string",
                    "description": "The string to replace it with"
                }
            },
            "required": ["path", "old_string", "new_string"]
        })
    }

    async fn execute(&self, params: Value) -> Result<String> {
        let path = params["path"]
            .as_str()
            .context("missing 'path' parameter")?;
        let old_string = params["old_string"]
            .as_str()
            .context("missing 'old_string' parameter")?;
        let new_string = params["new_string"]
            .as_str()
            .context("missing 'new_string' parameter")?;

        let validated_path = validate_path(path)?;

        let content = tokio::fs::read_to_string(&validated_path)
            .await
            .with_context(|| format!("failed to read file: {}", path))?;

        let count = content.matches(old_string).count();
        if count == 0 {
            anyhow::bail!("old_string not found in file: {}", path);
        }
        if count > 1 {
            anyhow::bail!(
                "old_string found {} times in file (must be unique): {}",
                count,
                path
            );
        }

        let new_content = content.replacen(old_string, new_string, 1);

        tokio::fs::write(&validated_path, &new_content)
            .await
            .with_context(|| format!("failed to write file: {}", path))?;

        Ok(format!("Successfully edited {}", path))
    }
}
