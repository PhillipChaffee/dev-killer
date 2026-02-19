use std::collections::HashMap;
use std::sync::Arc;

use super::Tool;

/// Registry for tools
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a tool
    pub fn register(&mut self, tool: impl Tool + 'static) {
        let name = tool.name().to_string();
        self.tools.insert(name, Arc::new(tool));
    }

    /// Get a tool by name
    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    /// Get all tools
    pub fn all(&self) -> Vec<&dyn Tool> {
        self.tools.values().map(|t| t.as_ref()).collect()
    }

    /// Get tool names
    pub fn names(&self) -> Vec<&str> {
        self.tools.keys().map(|s| s.as_str()).collect()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use async_trait::async_trait;
    use serde_json::Value;

    struct FakeTool {
        tool_name: &'static str,
    }

    #[async_trait]
    impl Tool for FakeTool {
        fn name(&self) -> &str {
            self.tool_name
        }
        fn description(&self) -> &str {
            "fake"
        }
        fn schema(&self) -> Value {
            serde_json::json!({})
        }
        async fn execute(&self, _params: Value) -> Result<String> {
            Ok("ok".into())
        }
    }

    #[test]
    fn register_and_get_tool() {
        let mut registry = ToolRegistry::new();
        registry.register(FakeTool {
            tool_name: "test_tool",
        });

        assert!(registry.get("test_tool").is_some());
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn all_returns_registered_tools() {
        let mut registry = ToolRegistry::new();
        registry.register(FakeTool { tool_name: "a" });
        registry.register(FakeTool { tool_name: "b" });

        let all = registry.all();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn names_returns_registered_names() {
        let mut registry = ToolRegistry::new();
        registry.register(FakeTool { tool_name: "foo" });
        registry.register(FakeTool { tool_name: "bar" });

        let mut names = registry.names();
        names.sort();
        assert_eq!(names, vec!["bar", "foo"]);
    }

    #[test]
    fn duplicate_registration_overwrites() {
        let mut registry = ToolRegistry::new();
        registry.register(FakeTool { tool_name: "dup" });
        registry.register(FakeTool { tool_name: "dup" });

        // Should still have 1 entry
        assert_eq!(registry.names().len(), 1);
    }
}
