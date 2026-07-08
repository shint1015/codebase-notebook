// Thin adapter over Tauri invoke — the only file that knows about IPC.
import { invoke } from "@tauri-apps/api/core";
import type {
  AskPreparation,
  ChatSession,
  IndexReport,
  Message,
  ProviderConfig,
  ProviderKind,
  SearchHit,
  Workspace,
  WorkspaceStats,
} from "../domain/types";

export const api = {
  listWorkspaces: () => invoke<Workspace[]>("list_workspaces"),
  createWorkspace: (name: string, rootPath: string) =>
    invoke<Workspace>("create_workspace", { name, rootPath }),
  deleteWorkspace: (workspaceId: string) =>
    invoke<void>("delete_workspace", { workspaceId }),
  setWorkspaceAllowExternal: (workspaceId: string, allow: boolean) =>
    invoke<void>("set_workspace_allow_external", { workspaceId, allow }),
  workspaceStats: (workspaceId: string) =>
    invoke<WorkspaceStats>("workspace_stats", { workspaceId }),

  indexWorkspace: (workspaceId: string) =>
    invoke<IndexReport>("index_workspace", { workspaceId }),
  searchWorkspace: (workspaceId: string, query: string, limit?: number) =>
    invoke<SearchHit[]>("search_workspace", { workspaceId, query, limit }),

  listProviders: () => invoke<ProviderConfig[]>("list_providers"),
  configureProvider: (
    config: Omit<ProviderConfig, "has_api_key">,
    apiKey: string | null,
  ) =>
    invoke<ProviderConfig>("configure_provider", {
      input: { ...config, api_key: apiKey },
    }),
  testProvider: (provider: ProviderKind) =>
    invoke<string>("test_provider", { provider }),

  createChatSession: (workspaceId: string, title: string) =>
    invoke<ChatSession>("create_chat_session", { workspaceId, title }),
  listChatSessions: (workspaceId: string) =>
    invoke<ChatSession[]>("list_chat_sessions", { workspaceId }),
  listChatMessages: (sessionId: string) =>
    invoke<Message[]>("list_chat_messages", { sessionId }),

  prepareAsk: (workspaceId: string, question: string, provider: ProviderKind) =>
    invoke<AskPreparation>("prepare_ask", { workspaceId, question, provider }),
  ask: (
    sessionId: string,
    workspaceId: string,
    question: string,
    provider: ProviderKind,
    consentGranted: boolean,
  ) =>
    invoke<Message>("ask", {
      sessionId,
      workspaceId,
      question,
      provider,
      consentGranted,
    }),
};
