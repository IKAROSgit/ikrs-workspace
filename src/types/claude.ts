export type ClaudeSessionStatus =
  | "disconnected"
  | "connecting"
  | "connected"
  | "thinking"
  | "error";

export interface ChatMessage {
  id: string;
  role: "user" | "assistant";
  text: string;
  timestamp: Date;
  isStreaming: boolean;
}

export interface ToolActivity {
  toolId: string;
  toolName: string;
  friendlyLabel: string;
  status: "running" | "success" | "error";
  summary?: string;
  toolInput?: string;
  resultContent?: string;
  startedAt: Date;
  completedAt?: Date;
}

export interface SessionReadyPayload {
  session_id: string;
  tools: string[];
  model: string;
  cwd: string;
}

export interface TextDeltaPayload {
  text: string;
  message_id: string;
}

export interface ToolStartPayload {
  tool_id: string;
  tool_name: string;
  friendly_label: string;
  tool_input: string | null;
}

export interface ToolEndPayload {
  tool_id: string;
  success: boolean;
  summary: string;
  result_content: string | null;
}

export interface TurnCompletePayload {
  session_id: string;
  cost_usd: number;
  duration_ms: number;
}

export interface ErrorPayload {
  message: string;
}

export interface SessionEndPayload {
  session_id: string;
  exit_code: number | null;
  reason: string;
}

export interface AuthStatus {
  loggedIn: boolean;
  authMethod: string | null;
  apiProvider: string | null;
}

export interface VersionCheck {
  installed: boolean;
  version: string | null;
  meets_minimum: boolean;
}

export interface McpAuthErrorPayload {
  server_name: string;
  error_hint: string;
}

// Emitted after every Write / Edit / NotebookEdit tool-result. The
// Rust side stats the target file and reports ground-truth state
// alongside what Claude claimed. `verified=false` when
// `claude_claimed_success=true` = the lie-class-of-bug that caused
// Moe's transcript + triaged tracker to vanish 2026-04-20.
export interface WriteVerificationPayload {
  tool_id: string;
  tool_name: "Write" | "Edit" | "NotebookEdit" | string;
  path: string;
  verified: boolean;
  size_bytes: number | null;
  reason: string | null;
  claude_claimed_success: boolean;
}

export interface WriteVerificationEntry extends WriteVerificationPayload {
  timestamp: Date;
}
