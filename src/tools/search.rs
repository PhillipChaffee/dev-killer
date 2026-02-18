use anyhow::{Context, Result};
use async_trait::async_trait;
use glob::glob;
use regex::Regex;
use serde_json::{Value, json};
use std::path::Path;

use super::Tool;

const MAX_RESULTS: usize = 100;
const MAX_CONTENT_PREVIEW: usize = 200;

/// Tool for finding files by glob pattern
pub struct GlobTool;

#[async_trait]
impl Tool for GlobTool {
    fn name(&self) -> &str {
        "glob"
    }

    fn description(&self) -> &str {
        "Find files matching a glob pattern (e.g., '**/*.rs', 'src/**/*.txt')"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "The glob pattern to match files"
                },
                "base_dir": {
                    "type": "string",
                    "description": "Optional base directory to search from (default: current directory)"
                }
            },
            "required": ["pattern"]
        })
    }

    async fn execute(&self, params: Value) -> Result<String> {
        let pattern = params["pattern"]
            .as_str()
            .context("missing 'pattern' parameter")?;

        let base_dir = params["base_dir"].as_str();

        // Build the full pattern
        let full_pattern = if let Some(base) = base_dir {
            format!("{}/{}", base.trim_end_matches('/'), pattern)
        } else {
            pattern.to_string()
        };

        // Execute glob
        let entries = glob(&full_pattern)
            .with_context(|| format!("invalid glob pattern: {}", full_pattern))?;

        let mut matches = Vec::new();
        for entry in entries {
            match entry {
                Ok(path) => {
                    matches.push(path.display().to_string());
                    if matches.len() >= MAX_RESULTS {
                        break;
                    }
                }
                Err(e) => {
                    // Skip entries we can't read
                    tracing::debug!("glob entry error: {}", e);
                }
            }
        }

        if matches.is_empty() {
            Ok("No files found matching pattern".to_string())
        } else {
            let truncated = if matches.len() >= MAX_RESULTS {
                format!("\n... (truncated at {} results)", MAX_RESULTS)
            } else {
                String::new()
            };
            Ok(format!(
                "Found {} files:\n{}{}",
                matches.len(),
                matches.join("\n"),
                truncated
            ))
        }
    }
}

/// Tool for searching file contents with regex
pub struct GrepTool;

#[async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &str {
        "grep"
    }

    fn description(&self) -> &str {
        "Search for a regex pattern in files. Returns matching lines with file paths and line numbers."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "The regex pattern to search for"
                },
                "path": {
                    "type": "string",
                    "description": "File or directory to search in"
                },
                "file_pattern": {
                    "type": "string",
                    "description": "Optional glob pattern to filter files (e.g., '*.rs')"
                },
                "case_insensitive": {
                    "type": "boolean",
                    "description": "Whether to ignore case (default: false)"
                }
            },
            "required": ["pattern", "path"]
        })
    }

    async fn execute(&self, params: Value) -> Result<String> {
        let pattern = params["pattern"]
            .as_str()
            .context("missing 'pattern' parameter")?;

        let path = params["path"]
            .as_str()
            .context("missing 'path' parameter")?;

        let file_pattern = params["file_pattern"].as_str();
        let case_insensitive = params["case_insensitive"].as_bool().unwrap_or(false);

        // Build regex
        let regex = if case_insensitive {
            Regex::new(&format!("(?i){}", pattern))
        } else {
            Regex::new(pattern)
        }
        .with_context(|| format!("invalid regex pattern: {}", pattern))?;

        let path = Path::new(path);
        let mut results = Vec::new();

        if path.is_file() {
            search_file(path, &regex, &mut results)?;
        } else if path.is_dir() {
            search_directory(path, &regex, file_pattern, &mut results)?;
        } else {
            anyhow::bail!("path does not exist: {}", path.display());
        }

        if results.is_empty() {
            Ok("No matches found".to_string())
        } else {
            let truncated = if results.len() >= MAX_RESULTS {
                format!("\n... (truncated at {} results)", MAX_RESULTS)
            } else {
                String::new()
            };
            Ok(format!(
                "Found {} matches:\n{}{}",
                results.len(),
                results.join("\n"),
                truncated
            ))
        }
    }
}

fn search_file(path: &Path, regex: &Regex, results: &mut Vec<String>) -> Result<()> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Ok(()), // Skip files we can't read
    };

    for (line_num, line) in content.lines().enumerate() {
        if results.len() >= MAX_RESULTS {
            break;
        }

        if regex.is_match(line) {
            let preview = if line.len() > MAX_CONTENT_PREVIEW {
                format!("{}...", &line[..MAX_CONTENT_PREVIEW])
            } else {
                line.to_string()
            };
            results.push(format!("{}:{}: {}", path.display(), line_num + 1, preview));
        }
    }

    Ok(())
}

fn search_directory(
    dir: &Path,
    regex: &Regex,
    file_pattern: Option<&str>,
    results: &mut Vec<String>,
) -> Result<()> {
    let glob_pattern = if let Some(fp) = file_pattern {
        format!("{}/**/{}", dir.display(), fp)
    } else {
        format!("{}/**/*", dir.display())
    };

    let entries = glob(&glob_pattern).with_context(|| "failed to create glob pattern")?;

    for entry in entries {
        if results.len() >= MAX_RESULTS {
            break;
        }

        if let Ok(path) = entry {
            if path.is_file() {
                search_file(&path, regex, results)?;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_glob_finds_files() {
        let dir = tempdir().unwrap();
        let file1 = dir.path().join("test1.txt");
        let file2 = dir.path().join("test2.txt");
        fs::write(&file1, "hello").unwrap();
        fs::write(&file2, "world").unwrap();

        let tool = GlobTool;
        let params = json!({
            "pattern": "*.txt",
            "base_dir": dir.path().to_str().unwrap()
        });

        let result = tool.execute(params).await.unwrap();
        assert!(result.contains("Found 2 files"));
        assert!(result.contains("test1.txt"));
        assert!(result.contains("test2.txt"));
    }

    #[tokio::test]
    async fn test_grep_finds_matches() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("test.txt");
        fs::write(&file, "hello world\nfoo bar\nhello again").unwrap();

        let tool = GrepTool;
        let params = json!({
            "pattern": "hello",
            "path": file.to_str().unwrap()
        });

        let result = tool.execute(params).await.unwrap();
        assert!(result.contains("Found 2 matches"));
        assert!(result.contains("hello world"));
        assert!(result.contains("hello again"));
    }
}
