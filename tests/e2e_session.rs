use dev_killer::{SessionState, SessionStatus, SqliteStorage, Storage};
use tempfile::TempDir;

#[tokio::test]
async fn test_save_and_load_session() {
    let tmp_dir = TempDir::new().expect("create temp dir");
    let db_path = tmp_dir.path().join("sessions.db");
    let storage = SqliteStorage::new(&db_path).expect("create storage");

    let session = SessionState::new("implement feature X", "/home/user/project");
    let session_id = session.id.clone();

    storage.save(&session).await.expect("save should succeed");

    let loaded = storage
        .load(&session_id)
        .await
        .expect("load should succeed")
        .expect("session should exist");

    assert_eq!(loaded.id, session_id);
    assert_eq!(loaded.task, "implement feature X");
    assert_eq!(loaded.working_dir, "/home/user/project");
    assert_eq!(loaded.status, SessionStatus::Pending);
}

#[tokio::test]
async fn test_list_sessions() {
    let tmp_dir = TempDir::new().expect("create temp dir");
    let db_path = tmp_dir.path().join("sessions.db");
    let storage = SqliteStorage::new(&db_path).expect("create storage");

    let s1 = SessionState::new("task one", "/tmp");
    let s2 = SessionState::new("task two", "/tmp");
    let s3 = SessionState::new("task three", "/tmp");

    storage.save(&s1).await.expect("save s1");
    storage.save(&s2).await.expect("save s2");
    storage.save(&s3).await.expect("save s3");

    let sessions = storage.list().await.expect("list should succeed");
    assert_eq!(sessions.len(), 3);
}

#[tokio::test]
async fn test_delete_session() {
    let tmp_dir = TempDir::new().expect("create temp dir");
    let db_path = tmp_dir.path().join("sessions.db");
    let storage = SqliteStorage::new(&db_path).expect("create storage");

    let session = SessionState::new("to be deleted", "/tmp");
    let session_id = session.id.clone();

    storage.save(&session).await.expect("save should succeed");

    // Verify it exists
    let loaded = storage
        .load(&session_id)
        .await
        .expect("load should succeed");
    assert!(loaded.is_some());

    // Delete it
    storage
        .delete(&session_id)
        .await
        .expect("delete should succeed");

    // Verify it's gone
    let loaded = storage
        .load(&session_id)
        .await
        .expect("load should succeed");
    assert!(loaded.is_none());
}

#[tokio::test]
async fn test_save_and_load_session_with_different_statuses() {
    let tmp_dir = TempDir::new().expect("create temp dir");
    let db_path = tmp_dir.path().join("sessions.db");
    let storage = SqliteStorage::new(&db_path).expect("create storage");

    // Create sessions with different statuses
    let mut completed = SessionState::new("completed task", "/tmp");
    completed.complete();

    let mut failed = SessionState::new("failed task", "/tmp");
    failed.set_error("something went wrong");

    let pending = SessionState::new("pending task", "/tmp");

    storage.save(&completed).await.expect("save completed");
    storage.save(&failed).await.expect("save failed");
    storage.save(&pending).await.expect("save pending");

    // Load each and verify status
    let loaded_completed = storage
        .load(&completed.id)
        .await
        .expect("load")
        .expect("exists");
    assert_eq!(loaded_completed.status, SessionStatus::Completed);

    let loaded_failed = storage
        .load(&failed.id)
        .await
        .expect("load")
        .expect("exists");
    assert_eq!(loaded_failed.status, SessionStatus::Failed);
    assert_eq!(loaded_failed.error.as_deref(), Some("something went wrong"));

    let loaded_pending = storage
        .load(&pending.id)
        .await
        .expect("load")
        .expect("exists");
    assert_eq!(loaded_pending.status, SessionStatus::Pending);
}
