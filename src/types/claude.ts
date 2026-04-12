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
