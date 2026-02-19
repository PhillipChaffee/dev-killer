use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::error::DevKillerError;
use crate::event::Event;

/// Output from a completed run
#[derive(Debug, Clone)]
pub struct RunOutput {
    /// The final output text from the agent
    pub output: String,
    /// Session ID if session persistence was enabled
    pub session_id: Option<String>,
}

/// Handle to a running agent execution.
///
/// Provides an event stream for monitoring progress and a way to wait
/// for the final result.
///
/// # Event consumption
///
/// Call [`next_event()`](Self::next_event) in a loop to receive events,
/// then call [`wait()`](Self::wait) to get the final output.
///
/// # Example
///
/// ```no_run
/// # use dev_killer::{RunHandle, RunOutput, DevKillerError};
/// # async fn example(mut handle: RunHandle) -> Result<RunOutput, DevKillerError> {
/// while let Some(event) = handle.next_event().await {
///     println!("Event: {:?}", event);
/// }
/// handle.wait().await
/// # }
/// ```
pub struct RunHandle {
    events: mpsc::Receiver<Event>,
    completion: JoinHandle<Result<RunOutput, DevKillerError>>,
}

impl RunHandle {
    pub(crate) fn new(
        events: mpsc::Receiver<Event>,
        completion: JoinHandle<Result<RunOutput, DevKillerError>>,
    ) -> Self {
        Self { events, completion }
    }

    /// Receive the next event from the running agent.
    ///
    /// Returns `None` when the event stream is closed (run is complete or failed).
    pub async fn next_event(&mut self) -> Option<Event> {
        self.events.recv().await
    }

    /// Wait for the run to complete and return the final output.
    ///
    /// Consumes any remaining events. If you want to process events,
    /// call [`next_event()`](Self::next_event) first.
    pub async fn wait(self) -> Result<RunOutput, DevKillerError> {
        self.completion
            .await
            .map_err(|e| DevKillerError::Internal(anyhow::anyhow!("task join error: {}", e)))?
    }

    /// Convenience: drain all events and wait for the result.
    ///
    /// Returns the final output, discarding all events.
    pub async fn output(mut self) -> Result<String, DevKillerError> {
        // Drain events to avoid backpressure blocking the task
        while self.events.recv().await.is_some() {}
        let result = self.wait().await?;
        Ok(result.output)
    }
}
