// Thin adapter over Tauri invoke — the only file that knows about IPC.
import { Channel, invoke } from "@tauri-apps/api/core";
import type {
  AskPreparation,
  ChatSession,
  IndexReport,
  Message,
  ProviderConfig,
  ProviderKind,
  Repository,
  SearchHit,
  Workspace,
  WorkspaceStats,
} from "../domain/types";

export const api = {
  listWorkspaces: () => invoke<Workspace[]>("list_workspaces"),
  createWorkspace: (name: string) =>
    invoke<Workspace>("create_workspace", { name }),
  deleteWorkspace: (workspaceId: string) =>
    invoke<void>("delete_workspace", { workspaceId }),

  setWorkspaceInstructions: (workspaceId: string, instructions: string) =>
    invoke<void>("set_workspace_instructions", { workspaceId, instructions }),
  exportWorkspace: (workspaceId: string, destPath: string) =>
    invoke<void>("export_workspace", { workspaceId, destPath }),
  importWorkspace: (srcPath: string) =>
    invoke<string>("import_workspace", { srcPath }),

  listRepositories: (workspaceId: string) =>
    invoke<Repository[]>("list_repositories", { workspaceId }),
  addLocalRepository: (workspaceId: string, rootPath: string) =>
    invoke<Repository>("add_local_repository", { workspaceId, rootPath }),
  addGitRepository: (workspaceId: string, url: string) =>
    invoke<Repository>("add_git_repository", { workspaceId, url }),
  addGithubIssuesRepository: (workspaceId: string, spec: string) =>
    invoke<Repository>("add_github_issues_repository", { workspaceId, spec }),
  deleteRepository: (repositoryId: string) =>
    invoke<void>("delete_repository", { repositoryId }),
  syncRepository: (repositoryId: string) =>
    invoke<Repository>("sync_repository", { repositoryId }),
  rebuildWatchers: () => invoke<void>("rebuild_watchers"),

  createGithubIssue: (spec: string, title: string, body: string) =>
    invoke<string>("create_github_issue", { spec, title, body }),
  writeWikiPage: (repositoryId: string, title: string, content: string) =>
    invoke<string>("write_wiki_page", { repositoryId, title, content }),
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
  renameChatSession: (sessionId: string, title: string) =>
    invoke<void>("rename_chat_session", { sessionId, title }),
  deleteChatSession: (sessionId: string) =>
    invoke<void>("delete_chat_session", { sessionId }),
  exportChat: (sessionId: string, destPath: string) =>
    invoke<void>("export_chat", { sessionId, destPath }),
  forkChatSession: (sessionId: string, upToMessageId?: string) =>
    invoke<ChatSession>("fork_chat_session", { sessionId, upToMessageId }),
  chatMarkdown: (sessionId: string) =>
    invoke<string>("chat_markdown", { sessionId }),
  chatToDocument: (workspaceId: string, sessionId: string, title: string) =>
    invoke<string>("chat_to_document", { workspaceId, sessionId, title }),
  revealSource: (workspaceId: string, relPath: string, line: number) =>
    invoke<void>("reveal_source", { workspaceId, relPath, line }),
  searchChats: (workspaceId: string, query: string, limit?: number) =>
    invoke<import("../domain/types").ChatSearchHit[]>("search_chats", {
      workspaceId,
      query,
      limit,
    }),
  listSourcePaths: (workspaceId: string) =>
    invoke<string[]>("list_source_paths", { workspaceId }),
  readSourceFile: (workspaceId: string, relPath: string) =>
    invoke<string>("read_source_file", { workspaceId, relPath }),
  writeSourceFile: (workspaceId: string, relPath: string, content: string) =>
    invoke<void>("write_source_file", { workspaceId, relPath, content }),

  prepareAsk: (
    workspaceId: string,
    question: string,
    provider: ProviderKind,
    sessionId: string | null,
  ) =>
    invoke<AskPreparation>("prepare_ask", {
      workspaceId,
      question,
      provider,
      sessionId,
    }),

  listUsage: (limit?: number) =>
    invoke<import("../domain/types").UsageRecord[]>("list_usage", { limit }),
  usageSummary: () =>
    invoke<import("../domain/types").ProviderUsageSummary[]>("usage_summary"),
  ollamaStatus: () =>
    invoke<import("../domain/types").OllamaStatus>("ollama_status"),
  pullOllamaModel: (model: string, onProgress: (line: string) => void) => {
    const channel = new Channel<string>();
    channel.onmessage = onProgress;
    return invoke<void>("pull_ollama_model", { model, onProgress: channel });
  },

  listNotes: (workspaceId: string) =>
    invoke<{ name: string; updated_at: string }[]>("list_notes", { workspaceId }),
  readNote: (workspaceId: string, name: string) =>
    invoke<string>("read_note", { workspaceId, name }),
  saveNote: (workspaceId: string, name: string, content: string) =>
    invoke<string>("save_note", { workspaceId, name, content }),
  deleteNote: (workspaceId: string, name: string) =>
    invoke<void>("delete_note", { workspaceId, name }),

  listConnectors: () =>
    invoke<{ name: string; connected: boolean }[]>("list_connectors"),
  setConnectorToken: (connector: string, token: string) =>
    invoke<void>("set_connector_token", { connector, token }),

  agentAsk: (
    sessionId: string,
    workspaceId: string,
    question: string,
    provider: ProviderKind,
    allowWrites: boolean,
  ) =>
    invoke<import("../domain/types").AgentOutcome>("agent_ask", {
      sessionId,
      workspaceId,
      question,
      provider,
      allowWrites,
    }),

  getSearchSettings: () =>
    invoke<{ embedding_model: string; rerank_enabled: boolean }>(
      "get_search_settings",
    ),
  setSearchSettings: (embeddingModel: string, rerankEnabled: boolean) =>
    invoke<void>("set_search_settings", { embeddingModel, rerankEnabled }),
  ask: (
    sessionId: string,
    workspaceId: string,
    question: string,
    provider: ProviderKind,
    consentGranted: boolean,
    onToken?: (token: string) => void,
  ) => {
    const channel = new Channel<string>();
    channel.onmessage = (token) => onToken?.(token);
    return invoke<Message>("ask", {
      sessionId,
      workspaceId,
      question,
      provider,
      consentGranted,
      onToken: channel,
    });
  },
};
