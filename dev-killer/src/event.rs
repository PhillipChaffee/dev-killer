use std::collections::HashSet;
use std::sync::Arc;

use serde_json::Value;
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio::time::{Duration, timeout};
use tracing::{debug, warn};

use crate::session::SessionPhase;

/// Default timeout for tool approval requests (seconds).
const APPROVAL_TIMEOUT_SECS: u64 = 60;

/// Tools considered "dangerous" for the `ApproveDangerous` mode.
const DANGEROUS_TOOLS: &[&str] = &["shell", "write_file", "edit_file"];

/// Events emitted during agent execution.
///
/// Consumers receive these through [`RunHandle::next_event()`](crate::RunHandle::next_event).
#[derive(Debug)]
pub enum Event {
    /// Agent pipeline phase changed
    PhaseChanged {
        phase: SessionPhase,
        agent_name: String,
    },
    /// An agent started working on a task
    AgentStarted {
        agent_name: String,
        task_preview: String,
    },
    /// An agent finished its work
    AgentCompleted {
        agent_name: String,
        output_preview: String,
    },
    /// LLM request started
    LlmRequestStarted { agent_name: String },
    /// A token was received from the LLM (streaming)
    LlmToken { agent_name: String, token: String },
    /// LLM response completed
    LlmResponseCompleted {
        agent_name: String,
        tool_call_count: usize,
    },
    /// Tool approval is required before execution.
    ///
    /// The consumer must respond via the oneshot sender.
    /// If not responded to within the timeout, the tool call is denied.
    ToolApprovalRequired {
        request_id: String,
        tool_name: String,
        arguments: Value,
        response: oneshot::Sender<ToolApprovalResponse>,
    },
    /// A tool started executing
    ToolStarted { tool_name: String },
    /// A tool finished executing
    ToolCompleted { tool_name: String, is_error: bool },
    /// An agent loop iteration completed
    IterationCompleted {
        agent_name: String,
        iteration: usize,
        max_iterations: usize,
    },
    /// Session was saved to storage
    SessionSaved { session_id: String },
    /// The entire run completed
    RunCompleted { status: RunStatus },
    /// A non-fatal warning
    Warning { message: String },
    /// An error occurred
    Error { message: String },
}

/// Response to a tool approval request
#[derive(Debug)]
pub enum ToolApprovalResponse {
    /// Allow this tool call
    Approve,
    /// Deny this tool call with a reason
    Deny { reason: String },
    /// Approve this tool call and all future calls to this tool in this session
    ApproveAlways,
}

/// Type alias for custom approval functions.
pub type ApprovalFn = Arc<dyn Fn(&str, &Value) -> bool + Send + Sync>;

/// How tool calls should be approved
#[derive(Clone, Default)]
pub enum ApprovalMode {
    /// Auto-approve all tool calls (default, matches current behavior)
    #[default]
    AutoApprove,
    /// Require approval for dangerous tools (shell, write, edit)
    ApproveDangerous,
    /// Require approval for all tool calls
    ApproveAll,
    /// Custom approval function: returns true if auto-approved, false if needs approval
    Custom(ApprovalFn),
}

impl std::fmt::Debug for ApprovalMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AutoApprove => write!(f, "AutoApprove"),
            Self::ApproveDangerous => write!(f, "ApproveDangerous"),
            Self::ApproveAll => write!(f, "ApproveAll"),
            Self::Custom(_) => write!(f, "Custom(...)"),
        }
    }
}

/// Status of a completed run
#[derive(Debug, Clone)]
pub enum RunStatus {
    Success,
    Failed { error: String },
}

/// Result of a tool approval check.
pub(crate) enum ApprovalResult {
    /// Tool is approved to execute
    Approved,
    /// Tool was denied with a reason message to return to the LLM
    Denied(String),
}

/// Sender for agent events.
///
/// Wraps a `tokio::sync::mpsc::Sender<Event>` with convenience methods.
/// If constructed with `noop()`, all sends are silently dropped and all
/// tools are auto-approved.
#[derive(Clone)]
pub struct EventSender {
    inner: Option<mpsc::Sender<Event>>,
    approval_mode: ApprovalMode,
    always_approved: Arc<Mutex<HashSet<String>>>,
}

impl EventSender {
    /// Create an EventSender from an mpsc sender and approval mode.
    pub fn new(sender: mpsc::Sender<Event>, approval_mode: ApprovalMode) -> Self {
        Self {
            inner: Some(sender),
            approval_mode,
            always_approved: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Create a no-op sender that silently drops all events and auto-approves all tools.
    pub fn noop() -> Self {
        Self {
            inner: None,
            approval_mode: ApprovalMode::AutoApprove,
            always_approved: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Emit a non-critical event (best-effort, drops on backpressure).
    pub fn emit(&self, event: Event) {
        if let Some(ref sender) = self.inner {
            let _ = sender.try_send(event);
        }
    }

    /// Emit a token event from streaming LLM output.
    pub fn emit_token(&self, agent_name: &str, token: &str) {
        self.emit(Event::LlmToken {
            agent_name: agent_name.to_string(),
            token: token.to_string(),
        });
    }

    /// Emit a critical event, waiting for capacity (blocks until consumer reads).
    pub async fn emit_blocking(&self, event: Event) {
        if let Some(ref sender) = self.inner {
            let _ = sender.send(event).await;
        }
    }

    /// Returns true if this sender is connected (not noop).
    pub fn is_active(&self) -> bool {
        self.inner.is_some()
    }

    /// Check whether a tool call needs approval, and if so, request it from the consumer.
    ///
    /// Returns `Approved` if the tool should execute, or `Denied(reason)` if not.
    pub(crate) async fn request_tool_approval(
        &self,
        tool_name: &str,
        arguments: &Value,
    ) -> ApprovalResult {
        // Check if already always-approved
        {
            let approved = self.always_approved.lock().await;
            if approved.contains(tool_name) {
                debug!(tool = tool_name, "tool already always-approved");
                return ApprovalResult::Approved;
            }
        }

        // Check if approval is needed based on mode
        let needs_approval = match &self.approval_mode {
            ApprovalMode::AutoApprove => false,
            ApprovalMode::ApproveDangerous => DANGEROUS_TOOLS.contains(&tool_name),
            ApprovalMode::ApproveAll => true,
            ApprovalMode::Custom(f) => !f(tool_name, arguments),
        };

        if !needs_approval {
            return ApprovalResult::Approved;
        }

        // No event channel means we can't ask the consumer — auto-approve
        let Some(ref sender) = self.inner else {
            return ApprovalResult::Approved;
        };

        // Create oneshot channel for the response
        let (tx, rx) = oneshot::channel();
        let request_id = uuid::Uuid::new_v4().to_string();

        debug!(tool = tool_name, request_id = %request_id, "requesting tool approval");

        // Send the approval request (blocking — must be received)
        let event = Event::ToolApprovalRequired {
            request_id: request_id.clone(),
            tool_name: tool_name.to_string(),
            arguments: arguments.clone(),
            response: tx,
        };
        if sender.send(event).await.is_err() {
            warn!("event channel closed, auto-approving tool call");
            return ApprovalResult::Approved;
        }

        // Wait for response with timeout
        match timeout(Duration::from_secs(APPROVAL_TIMEOUT_SECS), rx).await {
            Ok(Ok(ToolApprovalResponse::Approve)) => {
                debug!(tool = tool_name, "tool approved");
                ApprovalResult::Approved
            }
            Ok(Ok(ToolApprovalResponse::ApproveAlways)) => {
                debug!(tool = tool_name, "tool always-approved");
                let mut approved = self.always_approved.lock().await;
                approved.insert(tool_name.to_string());
                ApprovalResult::Approved
            }
            Ok(Ok(ToolApprovalResponse::Deny { reason })) => {
                debug!(tool = tool_name, reason = %reason, "tool denied");
                ApprovalResult::Denied(format!(
                    "Tool '{}' was denied by the user: {}",
                    tool_name, reason
                ))
            }
            Ok(Err(_)) => {
                // oneshot sender dropped without sending
                warn!(
                    tool = tool_name,
                    "approval response channel dropped, denying"
                );
                ApprovalResult::Denied(format!(
                    "Tool '{}' was denied: approval response not received",
                    tool_name
                ))
            }
            Err(_) => {
                // Timeout
                warn!(tool = tool_name, "approval timed out, denying");
                ApprovalResult::Denied(format!(
                    "Tool '{}' was denied: approval timed out after {}s",
                    tool_name, APPROVAL_TIMEOUT_SECS
                ))
            }
        }
    }
}
