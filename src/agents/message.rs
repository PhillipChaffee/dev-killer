use serde::{Deserialize, Serialize};

/// Unique identifier for a task
pub type TaskId = String;

/// A message passed between agents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentMessage {
    /// Assign a task to an agent
    TaskAssignment {
        task_id: TaskId,
        task: String,
        context: TaskContext,
    },
    /// Report task completion
    TaskResult { task_id: TaskId, result: TaskResult },
    /// Request clarification or more information
    Clarification { task_id: TaskId, question: String },
    /// Update on task progress
    StatusUpdate {
        task_id: TaskId,
        status: TaskStatus,
        message: String,
    },
}

/// Context provided with a task assignment
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskContext {
    /// The original high-level task from the user
    pub original_task: String,
    /// Previous work done (e.g., plan from planner)
    pub previous_work: Vec<String>,
    /// Files that have been modified
    pub modified_files: Vec<String>,
    /// Any relevant notes or constraints
    pub notes: Vec<String>,
}

impl TaskContext {
    pub fn new(original_task: impl Into<String>) -> Self {
        Self {
            original_task: original_task.into(),
            previous_work: Vec::new(),
            modified_files: Vec::new(),
            notes: Vec::new(),
        }
    }

    pub fn with_previous_work(mut self, work: impl Into<String>) -> Self {
        self.previous_work.push(work.into());
        self
    }

    pub fn with_modified_file(mut self, file: impl Into<String>) -> Self {
        self.modified_files.push(file.into());
        self
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }
}

/// Result of a completed task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    /// Whether the task succeeded
    pub success: bool,
    /// Summary of what was done
    pub summary: String,
    /// Files that were modified
    pub modified_files: Vec<String>,
    /// Any issues encountered
    pub issues: Vec<String>,
}

impl TaskResult {
    pub fn success(summary: impl Into<String>) -> Self {
        Self {
            success: true,
            summary: summary.into(),
            modified_files: Vec::new(),
            issues: Vec::new(),
        }
    }

    pub fn failure(summary: impl Into<String>) -> Self {
        Self {
            success: false,
            summary: summary.into(),
            modified_files: Vec::new(),
            issues: Vec::new(),
        }
    }

    pub fn with_modified_files(mut self, files: Vec<String>) -> Self {
        self.modified_files = files;
        self
    }

    pub fn with_issues(mut self, issues: Vec<String>) -> Self {
        self.issues = issues;
        self
    }
}

/// Status of a task in progress
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    /// Task is waiting to be started
    Pending,
    /// Task is currently being worked on
    InProgress,
    /// Task completed successfully
    Completed,
    /// Task failed
    Failed,
    /// Task needs review
    NeedsReview,
}
