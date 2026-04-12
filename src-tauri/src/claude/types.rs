use serde::{Deserialize, Serialize};

/// Raw stream-json event from Claude CLI stdout.
/// Every line is one of these. The parser must handle unknown variants gracefully.
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum StreamEvent {
    #[serde(rename = "system")]
    System(SystemEvent),
    #[serde(rename = "assistant")]
    Assistant(AssistantEvent),
    #[serde(rename = "user")]
    User(UserEvent),
    #[serde(rename = "rate_limit_event")]
    RateLimit(serde_json::Value),
    #[serde(rename = "result")]
    Result(ResultEvent),
}

#[derive(Debug, Deserialize)]
pub struct SystemEvent {
    pub subtype: String,
    pub session_id: Option<String>,
    /// Present on init events
    pub tools: Option<Vec<String>>,
    pub model: Option<String>,
    pub cwd: Option<String>,
    pub claude_code_version: Option<String>,
    /// Present on hook events
    pub hook_id: Option<String>,
    pub hook_name: Option<String>,
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct AssistantEvent {
    pub message: AssistantMessage,
}

#[derive(Debug, Deserialize)]
pub struct AssistantMessage {
    pub content: Vec<ContentBlock>,
    #[serde(default)]
    pub usage: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "thinking")]
    Thinking {
        thinking: String,
        signature: Option<String>,
    },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: Option<serde_json::Value>,
        #[serde(default)]
        is_error: bool,
    },
}

#[derive(Debug, Deserialize)]
pub struct UserEvent {
    pub message: Option<UserMessage>,
    pub tool_use_result: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UserMessage {
    pub content: Vec<ContentBlock>,
}

#[derive(Debug, Deserialize)]
pub struct ResultEvent {
    pub subtype: String,
    #[serde(default)]
    pub is_error: bool,
    pub result: Option<String>,
    pub session_id: Option<String>,
    pub total_cost_usd: Option<f64>,
    pub duration_ms: Option<u64>,
    pub num_turns: Option<u32>,
    pub stop_reason: Option<String>,
}

/// Typed Tauri event payloads emitted to the frontend
#[derive(Debug, Clone, Serialize)]
pub struct SessionReadyPayload {
    pub session_id: String,
    pub tools: Vec<String>,
    pub model: String,
    pub cwd: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TextDeltaPayload {
    pub text: String,
    pub message_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolStartPayload {
    pub tool_id: String,
    pub tool_name: String,
    pub friendly_label: String,
    pub tool_input: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolEndPayload {
    pub tool_id: String,
    pub success: bool,
    pub summary: String,
    pub result_content: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TurnCompletePayload {
    pub session_id: String,
    pub cost_usd: f64,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ErrorPayload {
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionEndPayload {
    pub session_id: String,
    pub exit_code: Option<i32>,
    pub reason: String,
}

/// Auth status returned by `claude auth status`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthStatus {
    #[serde(rename = "loggedIn")]
    pub logged_in: bool,
    #[serde(rename = "authMethod")]
    pub auth_method: Option<String>,
    #[serde(rename = "apiProvider")]
    pub api_provider: Option<String>,
}

/// CLI version check
#[derive(Debug, Clone, Serialize)]
pub struct VersionCheck {
    pub installed: bool,
    pub version: Option<String>,
    pub meets_minimum: bool,
}

pub const MIN_CLAUDE_VERSION: &str = "2.1.0";
