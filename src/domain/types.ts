// Frontend domain types — mirror of the Rust domain entities (serde shapes).

export interface Workspace {
  id: string;
  name: string;
  allow_external: boolean;
  instructions: string;
  created_at: string;
}

export type SourceKind = "local" | "git" | "github_issues";

export interface Repository {
  id: string;
  workspace_id: string;
  name: string;
  root_path: string;
  remote_url: string | null;
  source_kind: SourceKind;
  created_at: string;
}

export interface WorkspaceStats {
  documents: number;
  chunks: number;
}

export interface IndexReport {
  files_indexed: number;
  files_unchanged: number;
  chunks_created: number;
  files_with_secrets_redacted: number;
  embeddings_created: number;
  embedding_available: boolean;
}

export interface Chunk {
  id: string;
  document_id: string;
  workspace_id: string;
  seq: number;
  content: string;
  start_line: number;
  end_line: number;
}

export interface SearchHit {
  chunk: Chunk;
  rel_path: string;
  score: number;
}

export type ProviderKind =
  | "ollama"
  | "openai"
  | "anthropic"
  | "gemini"
  | "mistral"
  | "x_ai"
  | "openai_compatible";

export const PROVIDER_LABELS: Record<ProviderKind, string> = {
  ollama: "Ollama (Local)",
  openai: "OpenAI",
  anthropic: "Anthropic",
  gemini: "Google Gemini",
  mistral: "Mistral",
  x_ai: "xAI (Grok)",
  openai_compatible: "OpenAI-compatible",
};

export const EXTERNAL_PROVIDERS: ProviderKind[] = [
  "openai",
  "anthropic",
  "gemini",
  "mistral",
  "x_ai",
  "openai_compatible",
];

export interface ProviderConfig {
  kind: ProviderKind;
  enabled: boolean;
  base_url: string;
  default_model: string;
  allow_send_code: boolean;
  monthly_budget_usd: number | null;
  has_api_key: boolean;
}

export interface UsageRecord {
  id: string;
  created_at: string;
  provider: string;
  model: string;
  workspace_id: string | null;
  prompt_chars: number;
  completion_chars: number;
  est_cost_usd: number;
  sources: string[];
}

export interface ProviderUsageSummary {
  provider: string;
  month_total_usd: number;
  monthly_budget_usd: number | null;
}

export interface OllamaStatus {
  reachable: boolean;
  chat_model: string;
  chat_model_present: boolean;
  embedding_model: string;
  embedding_model_present: boolean;
}

export interface ChatSession {
  id: string;
  workspace_id: string;
  title: string;
  created_at: string;
}

export interface Citation {
  marker: number;
  chunk_id: string;
  rel_path: string;
  start_line: number;
  end_line: number;
  snippet: string;
}

export interface Message {
  id: string;
  session_id: string;
  role: "user" | "assistant";
  content: string;
  citations: Citation[];
  provider: string | null;
  model: string | null;
  created_at: string;
}

export interface SourcePreview {
  rel_path: string;
  start_line: number;
  end_line: number;
}

export interface ChatSearchHit {
  session_id: string;
  session_title: string;
  message_id: string;
  role: string;
  excerpt: string;
  created_at: string;
}

export interface ToolEvent {
  name: string;
  summary: string;
  result: string;
  blocked: boolean;
}

export interface AgentOutcome {
  message: Message;
  tool_events: ToolEvent[];
}

export interface AskPreparation {
  provider: ProviderKind;
  model: string;
  is_external: boolean;
  requires_consent: boolean;
  sources: SourcePreview[];
}

export interface CommandError {
  code: string;
  message: string;
}

export function isCommandError(value: unknown): value is CommandError {
  return (
    typeof value === "object" &&
    value !== null &&
    "code" in value &&
    "message" in value
  );
}
