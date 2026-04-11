use crate::mcp::{manager::McpProcessManager, McpServerType, McpStatus};
use serde::Deserialize;
use std::collections::HashMap;
use tauri::State;

#[derive(Deserialize)]
pub struct SpawnMcpArgs {
    pub server_type: McpServerType,
    pub command: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
}

#[tauri::command]
pub async fn spawn_mcp(
    state: State<'_, McpProcessManager>,
    args: SpawnMcpArgs,
) -> Result<u32, String> {
    state.spawn(args.server_type, &args.command, &args.args, &args.env)
}

#[tauri::command]
pub async fn kill_mcp(
    state: State<'_, McpProcessManager>,
    server_type: McpServerType,
) -> Result<(), String> {
    state.kill(&server_type)
}

#[tauri::command]
pub async fn kill_all_mcp(
    state: State<'_, McpProcessManager>,
) -> Result<(), String> {
    state.kill_all()
}

#[tauri::command]
pub async fn mcp_health(
    state: State<'_, McpProcessManager>,
    server_type: McpServerType,
) -> Result<McpStatus, String> {
    state.health_check(&server_type)
}

#[tauri::command]
pub async fn restart_mcp(
    state: State<'_, McpProcessManager>,
    server_type: McpServerType,
) -> Result<u32, String> {
    state.restart(&server_type)
}
