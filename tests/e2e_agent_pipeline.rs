mod common;

use std::io::Write;

use dev_killer::{
    CoderAgent, Executor, LlmResponse, Message, SessionState, SqliteStorage, Storage, ToolCall,
};
use tempfile::{NamedTempFile, TempDir};

use common::{MockLlmProvider, create_test_tool_registry};

#[tokio::test]
async fn test_coder_agent_completes_simple_task() {
    let provider = MockLlmProvider::single_response("Task completed successfully.");
    let tools = create_test_tool_registry();
    let executor = Executor::new(tools);
    let agent = CoderAgent::new();

    let result = executor
        .run(&agent, "say hello", &provider)
        .await
        .expect("agent should complete");

    assert_eq!(result, "Task completed successfully.");
}

#[tokio::test]
async fn test_coder_agent_executes_tool_then_completes() {
    // Create a temp file with known content
    let mut tmp = NamedTempFile::new().expect("create temp file");
    writeln!(tmp, "hello from temp file").expect("write temp file");
    let tmp_path = tmp.path().to_string_lossy().to_string();

    // First response: request read_file tool call
    let tool_call_response = LlmResponse {
        message: Message::assistant("I'll read the file for you."),
        tool_calls: vec![ToolCall {
            id: "call_1".to_string(),
            name: "read_file".to_string(),
            arguments: serde_json::json!({ "path": tmp_path }),
        }],
    };

    // Second response: final text after seeing tool result
    let final_response = LlmResponse {
        message: Message::assistant("The file contains: hello from temp file"),
        tool_calls: vec![],
    };

    let provider = MockLlmProvider::with_responses(vec![tool_call_response, final_response]);
    let tools = create_test_tool_registry();
    let executor = Executor::new(tools);
    let agent = CoderAgent::new();

    let result = executor
        .run(&agent, "read the temp file", &provider)
        .await
        .expect("agent should complete");

    assert!(result.contains("hello from temp file"));
}

#[tokio::test]
async fn test_coder_agent_handles_unknown_tool_gracefully() {
    // First response: request a nonexistent tool
    let bad_tool_response = LlmResponse {
        message: Message::assistant("Let me use a special tool."),
        tool_calls: vec![ToolCall {
            id: "call_1".to_string(),
            name: "nonexistent_tool".to_string(),
            arguments: serde_json::json!({}),
        }],
    };

    // Second response: final text (agent recovers)
    let final_response = LlmResponse {
        message: Message::assistant("That tool wasn't available, but task is done."),
        tool_calls: vec![],
    };

    let provider = MockLlmProvider::with_responses(vec![bad_tool_response, final_response]);
    let tools = create_test_tool_registry();
    let executor = Executor::new(tools);
    let agent = CoderAgent::new();

    let result = executor
        .run(&agent, "do something", &provider)
        .await
        .expect("agent should complete despite unknown tool");

    assert!(result.contains("task is done"));
}

#[tokio::test]
async fn test_executor_with_session_tracking() {
    let tmp_dir = TempDir::new().expect("create temp dir");
    let db_path = tmp_dir.path().join("test_sessions.db");
    let storage = SqliteStorage::new(&db_path).expect("create storage");

    let provider = MockLlmProvider::single_response("Done.");
    let tools = create_test_tool_registry();
    let executor = Executor::with_storage(tools, Box::new(storage));

    let mut session = SessionState::new("test task", "/tmp");

    let session_id = session.id.clone();

    let result = executor
        .run_with_session(&agent(), &mut session, &provider)
        .await
        .expect("session should complete");

    assert_eq!(result, "Done.");

    // Verify session was persisted as completed
    let storage2 = SqliteStorage::new(&db_path).expect("reopen storage");
    let loaded = storage2
        .load(&session_id)
        .await
        .expect("load should succeed")
        .expect("session should exist");

    assert_eq!(loaded.status.to_string(), "completed");
    assert_eq!(loaded.phase.to_string(), "completed");
}

fn agent() -> CoderAgent {
    CoderAgent::new()
}
