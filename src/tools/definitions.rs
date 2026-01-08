use smol_str::SmolStr;

/// A tool call extracted from LLM output
#[derive(Debug, Clone)]
pub struct ToolCall {
    /// The LLM's reasoning before calling the tool
    pub thinking: String,
    /// The tool name
    pub tool_name: SmolStr,
    /// Arguments to the tool (space-separated tokens)
    pub args: Vec<SmolStr>,
}

impl ToolCall {
    /// Get argument at index
    pub fn get_arg(&self, index: usize) -> Option<&SmolStr> {
        self.args.get(index)
    }

    /// Get the first argument (most common case)
    pub fn first_arg(&self) -> Option<&SmolStr> {
        self.args.first()
    }

    /// Join all arguments into a single string
    pub fn args_joined(&self) -> String {
        self.args.iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Result of executing a tool
#[derive(Debug, Clone)]
pub struct ToolResult {
    /// Whether the tool executed successfully
    pub success: bool,
    /// Description of what happened (shown to LLM and user)
    pub description: String,
    /// Short summary for action history
    pub summary: String,
}

impl ToolResult {
    pub fn success(description: String, summary: String) -> Self {
        Self {
            success: true,
            description,
            summary,
        }
    }

    pub fn failure(description: String) -> Self {
        Self {
            success: false,
            description: description.clone(),
            summary: description,
        }
    }
}
