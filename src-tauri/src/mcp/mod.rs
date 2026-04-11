pub mod manager;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum McpServerType {
    Gmail,
    Calendar,
    Drive,
    Obsidian,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum McpHealthStatus {
    Healthy,
    Reconnecting,
    Down,
    Stopped,
}

#[derive(Debug, Clone, Serialize)]
pub struct McpStatus {
    pub server_type: McpServerType,
    pub status: McpHealthStatus,
    pub pid: Option<u32>,
    pub restart_count: u32,
}
