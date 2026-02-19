use std::sync::Arc;

use tokio::sync::mpsc;
use tracing::{error, info};

use crate::builder::DevKillerBuilder;
use crate::error::DevKillerError;
use crate::event::{ApprovalMode, Event, EventSender, RunStatus};
use crate::llm::LlmProvider;
use crate::pipeline::{Pipeline, execute_pipeline};
use crate::run_handle::{RunHandle, RunOutput};
use crate::session::{
    PortableSession, SessionPhase, SessionState, SessionStatus, SessionSummary, Storage,
};
use crate::tools::ToolRegistry;

const EVENT_CHANNEL_CAPACITY: usize = 256;

/// Shared inner state, wrapped in Arc so spawned tasks can reference it.
struct Inner {
    provider: Box<dyn LlmProvider>,
    tools: ToolRegistry,
    storage: Option<Box<dyn Storage>>,
    pipeline: Pipeline,
    approval_mode: ApprovalMode,
}

/// Primary entry point for the dev-killer library.
///
/// Use [`DevKiller::builder()`] to construct an instance.
///
/// # Example
///
/// ```no_run
/// # use dev_killer::DevKiller;
/// # async fn example() -> Result<(), dev_killer::DevKillerError> {
/// let dk = DevKiller::builder()
///     .anthropic(None)?
///     .default_tools()
///     .build()?;
///
/// let mut handle = dk.run("read src/lib.rs and summarize it").await?;
/// while let Some(event) = handle.next_event().await {
///     println!("{:?}", event);
/// }
/// let output = handle.wait().await?;
/// println!("{}", output.output);
/// # Ok(())
/// # }
/// ```
pub struct DevKiller {
    inner: Arc<Inner>,
}

impl DevKiller {
    pub(crate) fn from_parts(
        provider: Box<dyn LlmProvider>,
        tools: ToolRegistry,
        storage: Option<Box<dyn Storage>>,
        pipeline: Pipeline,
        approval_mode: ApprovalMode,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                provider,
                tools,
                storage,
                pipeline,
                approval_mode,
            }),
        }
    }

    /// Create a new builder for configuring a `DevKiller` instance.
    pub fn builder() -> DevKillerBuilder {
        DevKillerBuilder::new()
    }

    /// Run a task and return a handle for events and the final result.
    ///
    /// The agent executes in a background tokio task. Use the returned
    /// [`RunHandle`] to receive events and await completion.
    pub async fn run(&self, task: &str) -> Result<RunHandle, DevKillerError> {
        info!(
            task,
            steps = self.inner.pipeline.steps.len(),
            "starting task"
        );

        let (tx, rx) = mpsc::channel(EVENT_CHANNEL_CAPACITY);
        let events = EventSender::new(tx, self.inner.approval_mode.clone());
        let inner = Arc::clone(&self.inner);
        let task_str = task.to_string();

        let completion = tokio::spawn(async move {
            let result = execute_run(&inner, &task_str, &events).await;
            match &result {
                Ok(out) => {
                    events.emit(Event::RunCompleted {
                        status: RunStatus::Success,
                    });
                    info!(session_id = ?out.session_id, "run completed successfully");
                }
                Err(e) => {
                    events.emit(Event::RunCompleted {
                        status: RunStatus::Failed {
                            error: e.to_string(),
                        },
                    });
                }
            }
            // EventSender is dropped here, closing the channel
            result
        });

        Ok(RunHandle::new(rx, completion))
    }

    /// Resume a previously interrupted session.
    ///
    /// Requires storage to be configured.
    pub async fn resume(&self, session_id: &str) -> Result<RunHandle, DevKillerError> {
        let storage = self.require_storage()?;

        let mut session = storage
            .load(session_id)
            .await
            .map_err(|e| DevKillerError::Session(format!("failed to load session: {}", e)))?
            .ok_or_else(|| DevKillerError::Session(format!("session not found: {}", session_id)))?;

        if !session.can_resume() {
            return Err(DevKillerError::Session(format!(
                "session cannot be resumed (status: {})",
                session.status
            )));
        }

        info!(session_id, "resuming session");

        let (tx, rx) = mpsc::channel(EVENT_CHANNEL_CAPACITY);
        let events = EventSender::new(tx, self.inner.approval_mode.clone());
        let inner = Arc::clone(&self.inner);

        let completion = tokio::spawn(async move {
            let result = execute_with_session(&inner, &mut session, &events).await;
            let session_id = session.id.clone();
            match &result {
                Ok(_) => events.emit(Event::RunCompleted {
                    status: RunStatus::Success,
                }),
                Err(e) => events.emit(Event::RunCompleted {
                    status: RunStatus::Failed {
                        error: e.to_string(),
                    },
                }),
            }
            result.map(|output| RunOutput {
                output,
                session_id: Some(session_id),
            })
        });

        Ok(RunHandle::new(rx, completion))
    }

    /// List all saved sessions.
    pub async fn sessions(&self) -> Result<Vec<SessionSummary>, DevKillerError> {
        let storage = self.require_storage()?;
        storage
            .list()
            .await
            .map_err(|e| DevKillerError::Session(format!("failed to list sessions: {}", e)))
    }

    /// Load a specific session by ID.
    pub async fn session(&self, id: &str) -> Result<Option<SessionState>, DevKillerError> {
        let storage = self.require_storage()?;
        storage
            .load(id)
            .await
            .map_err(|e| DevKillerError::Session(format!("failed to load session: {}", e)))
    }

    /// Delete a session by ID.
    pub async fn delete_session(&self, id: &str) -> Result<(), DevKillerError> {
        let storage = self.require_storage()?;
        storage
            .delete(id)
            .await
            .map_err(|e| DevKillerError::Session(format!("failed to delete session: {}", e)))
    }

    /// Export a session as a portable JSON-serializable object.
    ///
    /// The returned [`PortableSession`] can be serialized to JSON and transferred
    /// to another environment, then imported with [`import_session`](Self::import_session).
    pub async fn export_session(&self, id: &str) -> Result<PortableSession, DevKillerError> {
        let storage = self.require_storage()?;
        let session = storage
            .load(id)
            .await
            .map_err(|e| DevKillerError::Session(format!("failed to load session: {}", e)))?
            .ok_or_else(|| DevKillerError::Session(format!("session not found: {}", id)))?;

        info!(session_id = id, "exporting session");
        Ok(PortableSession::from_session(&session))
    }

    /// Import a portable session and save it to storage.
    ///
    /// The session is assigned a new ID and marked as `Interrupted` so it can
    /// be resumed with [`resume`](Self::resume).
    ///
    /// Returns the new session ID.
    pub async fn import_session(
        &self,
        portable: PortableSession,
        working_dir: Option<String>,
    ) -> Result<String, DevKillerError> {
        let storage = self.require_storage()?;
        let session = portable.into_session(working_dir);
        let new_id = session.id.clone();

        storage.save(&session).await.map_err(|e| {
            DevKillerError::Storage(format!("failed to save imported session: {}", e))
        })?;

        info!(
            new_id = %new_id,
            original_id = %session.id,
            "imported session"
        );
        Ok(new_id)
    }

    fn require_storage(&self) -> Result<&dyn Storage, DevKillerError> {
        self.inner
            .storage
            .as_ref()
            .map(|s| s.as_ref())
            .ok_or_else(|| DevKillerError::Storage("no storage configured".to_string()))
    }
}

async fn execute_run(
    inner: &Inner,
    task: &str,
    events: &EventSender,
) -> Result<RunOutput, DevKillerError> {
    if inner.storage.is_some() {
        let working_dir = std::env::current_dir()
            .map_err(|e| DevKillerError::Internal(e.into()))?
            .to_string_lossy()
            .to_string();

        let mut session = SessionState::new(task, working_dir);
        info!(session_id = %session.id, "created new session");

        let output = execute_with_session(inner, &mut session, events).await?;
        Ok(RunOutput {
            output,
            session_id: Some(session.id),
        })
    } else {
        let output = execute_pipeline(
            &inner.pipeline,
            task,
            inner.provider.as_ref(),
            &inner.tools,
            events,
        )
        .await
        .map_err(DevKillerError::Internal)?;
        Ok(RunOutput {
            output,
            session_id: None,
        })
    }
}

async fn execute_with_session(
    inner: &Inner,
    session: &mut SessionState,
    events: &EventSender,
) -> Result<String, DevKillerError> {
    let storage = inner.storage.as_ref().ok_or_else(|| {
        DevKillerError::Storage("no storage configured for session tracking".to_string())
    })?;

    session.set_status(SessionStatus::InProgress);
    session.set_phase(SessionPhase::Planning);
    storage
        .save(session)
        .await
        .map_err(|e| DevKillerError::Storage(format!("failed to save session: {}", e)))?;
    events.emit(Event::SessionSaved {
        session_id: session.id.clone(),
    });

    let result = execute_pipeline(
        &inner.pipeline,
        &session.task,
        inner.provider.as_ref(),
        &inner.tools,
        events,
    )
    .await;

    match result {
        Ok(output) => {
            session.complete();
            storage
                .save(session)
                .await
                .map_err(|e| DevKillerError::Storage(format!("failed to save session: {}", e)))?;
            events.emit(Event::SessionSaved {
                session_id: session.id.clone(),
            });
            info!(session_id = %session.id, "session completed successfully");
            Ok(output)
        }
        Err(e) => {
            session.set_error(e.to_string());
            if let Err(save_err) = storage.save(session).await {
                error!(error = %save_err, "failed to save failed session state");
            }
            Err(DevKillerError::Internal(e))
        }
    }
}
