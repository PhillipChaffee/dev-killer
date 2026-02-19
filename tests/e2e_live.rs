mod common;

use std::io::Write;

use dev_killer::{AnthropicProvider, CoderAgent, Executor, OpenAIProvider};
use tempfile::NamedTempFile;

use common::create_test_tool_registry;

#[tokio::test]
#[ignore]
async fn test_anthropic_simple_response() {
    if std::env::var("ANTHROPIC_API_KEY").is_err() {
        return;
    }

    let provider = AnthropicProvider::sonnet().expect("create provider");
    let tools = create_test_tool_registry();
    let executor = Executor::new(tools);
    let agent = CoderAgent::new();

    let result = executor
        .run(&agent, "respond with only the word HELLO", &provider)
        .await
        .expect("should get response");

    assert!(
        result.to_uppercase().contains("HELLO"),
        "expected HELLO in response, got: {}",
        result
    );
}

#[tokio::test]
#[ignore]
async fn test_agent_reads_file() {
    if std::env::var("ANTHROPIC_API_KEY").is_err() {
        return;
    }

    let mut tmp = NamedTempFile::new().expect("create temp file");
    writeln!(tmp, "unique_test_content_abc123").expect("write temp file");
    let tmp_path = tmp.path().to_string_lossy().to_string();

    let provider = AnthropicProvider::sonnet().expect("create provider");
    let tools = create_test_tool_registry();
    let executor = Executor::new(tools);
    let agent = CoderAgent::new();

    let result = executor
        .run(
            &agent,
            &format!("read the file at {} and tell me what it contains", tmp_path),
            &provider,
        )
        .await
        .expect("should get response");

    assert!(
        result.contains("unique_test_content_abc123"),
        "expected file contents in response, got: {}",
        result
    );
}

#[tokio::test]
#[ignore]
async fn test_shell_tool_execution() {
    if std::env::var("ANTHROPIC_API_KEY").is_err() {
        return;
    }

    let provider = AnthropicProvider::sonnet().expect("create provider");
    let tools = create_test_tool_registry();
    let executor = Executor::new(tools);
    let agent = CoderAgent::new();

    let result = executor
        .run(
            &agent,
            "run the command 'echo hello world' using the shell tool and tell me the output",
            &provider,
        )
        .await
        .expect("should get response");

    assert!(
        result.contains("hello world"),
        "expected 'hello world' in response, got: {}",
        result
    );
}

#[tokio::test]
#[ignore]
async fn test_policy_blocks_dangerous_commands() {
    if std::env::var("ANTHROPIC_API_KEY").is_err() {
        return;
    }

    let provider = AnthropicProvider::sonnet().expect("create provider");
    let tools = create_test_tool_registry();
    let executor = Executor::new(tools);
    let agent = CoderAgent::new();

    let result = executor
        .run(
            &agent,
            "run the command 'sudo rm -rf /' and tell me what happened",
            &provider,
        )
        .await
        .expect("should get response (policy blocks it, doesn't crash)");

    // The tool should have returned an error about blocked commands
    let lower = result.to_lowercase();
    assert!(
        lower.contains("block")
            || lower.contains("denied")
            || lower.contains("error")
            || lower.contains("not allowed")
            || lower.contains("policy"),
        "expected policy rejection message, got: {}",
        result
    );
}

#[tokio::test]
#[ignore]
async fn test_openai_provider_works() {
    if std::env::var("OPENAI_API_KEY").is_err() {
        return;
    }

    let provider = OpenAIProvider::gpt4o().expect("create provider");
    let tools = create_test_tool_registry();
    let executor = Executor::new(tools);
    let agent = CoderAgent::new();

    let result = executor
        .run(&agent, "respond with only the word HELLO", &provider)
        .await
        .expect("should get response");

    assert!(
        result.to_uppercase().contains("HELLO"),
        "expected HELLO in response, got: {}",
        result
    );
}
