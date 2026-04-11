use super::{McpHealthStatus, McpServerType, McpStatus};
use std::collections::HashMap;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

const MAX_RESTARTS: u32 = 3;

struct McpProcess {
    child: Child,
    server_type: McpServerType,
    restart_count: u32,
    command: String,
    args: Vec<String>,
    env: HashMap<String, String>,
}

pub struct McpProcessManager {
    processes: Mutex<HashMap<String, McpProcess>>,
}

impl McpProcessManager {
    pub fn new() -> Self {
        Self {
            processes: Mutex::new(HashMap::new()),
        }
    }

    pub fn spawn(
        &self,
        server_type: McpServerType,
        command: &str,
        args: &[String],
        env: &HashMap<String, String>,
    ) -> Result<u32, String> {
        let key = format!("{:?}", server_type);
        self.kill(&server_type).ok();

        let mut cmd = Command::new(command);
        cmd.args(args)
            .envs(env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let child = cmd.spawn().map_err(|e| format!("Failed to spawn MCP {key}: {e}"))?;
        let pid = child.id();

        let process = McpProcess {
            child,
            server_type: server_type.clone(),
            restart_count: 0,
            command: command.to_string(),
            args: args.to_vec(),
            env: env.clone(),
        };

        let mut processes = self.processes.lock().map_err(|e| e.to_string())?;
        processes.insert(key, process);
        Ok(pid)
    }

    pub fn kill(&self, server_type: &McpServerType) -> Result<(), String> {
        let key = format!("{:?}", server_type);
        let mut processes = self.processes.lock().map_err(|e| e.to_string())?;
        if let Some(mut process) = processes.remove(&key) {
            process.child.kill().map_err(|e| format!("Failed to kill MCP {key}: {e}"))?;
            process.child.wait().ok();
        }
        Ok(())
    }

    pub fn kill_all(&self) -> Result<(), String> {
        let mut processes = self.processes.lock().map_err(|e| e.to_string())?;
        for (_, mut process) in processes.drain() {
            process.child.kill().ok();
            process.child.wait().ok();
        }
        Ok(())
    }

    pub fn health_check(&self, server_type: &McpServerType) -> Result<McpStatus, String> {
        let key = format!("{:?}", server_type);
        let processes = self.processes.lock().map_err(|e| e.to_string())?;

        Ok(match processes.get(&key) {
            None => McpStatus {
                server_type: server_type.clone(),
                status: McpHealthStatus::Stopped,
                pid: None,
                restart_count: 0,
            },
            Some(process) => {
                let pid = process.child.id();
                McpStatus {
                    server_type: server_type.clone(),
                    status: McpHealthStatus::Healthy,
                    pid: Some(pid),
                    restart_count: process.restart_count,
                }
            }
        })
    }

    pub fn restart(&self, server_type: &McpServerType) -> Result<u32, String> {
        let key = format!("{:?}", server_type);
        let (command, args, env, restart_count) = {
            let mut processes = self.processes.lock().map_err(|e| e.to_string())?;
            let process = processes.remove(&key).ok_or(format!("No MCP process for {key}"))?;
            if process.restart_count >= MAX_RESTARTS {
                return Err(format!("Max restarts ({MAX_RESTARTS}) exceeded for {key}"));
            }
            let mut child = process.child;
            child.kill().ok();
            child.wait().ok();
            (process.command, process.args, process.env, process.restart_count + 1)
        };

        let mut cmd = Command::new(&command);
        cmd.args(&args)
            .envs(&env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let child = cmd.spawn().map_err(|e| format!("Failed to restart MCP {key}: {e}"))?;
        let pid = child.id();

        let process = McpProcess {
            child,
            server_type: server_type.clone(),
            restart_count,
            command,
            args,
            env,
        };

        let mut processes = self.processes.lock().map_err(|e| e.to_string())?;
        processes.insert(key, process);
        Ok(pid)
    }
}

impl Default for McpProcessManager {
    fn default() -> Self {
        Self::new()
    }
}
