import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import type { ChatSession, Workspace } from "../../domain/types";
import { api } from "../../infrastructure/api";
import { useRepositories } from "../../application/useRepositories";

interface Props {
  workspace: Workspace;
  onOpenSession: (sessionId: string) => void;
  onNewChat: () => void;
  onDeleteWorkspace: (id: string) => Promise<void>;
}

/**
 * Landing view for a workspace: manage its repositories, run indexing, and
 * jump into a chat (existing session or a new one).
 */
export function WorkspaceHome({
  workspace,
  onOpenSession,
  onNewChat,
  onDeleteWorkspace,
}: Props) {
  const repos = useRepositories(workspace.id);
  const [sessions, setSessions] = useState<ChatSession[]>([]);
  const [gitUrl, setGitUrl] = useState("");

  useEffect(() => {
    void api.listChatSessions(workspace.id).then(setSessions);
  }, [workspace.id]);

  const addLocalFolder = async () => {
    const dir = await open({ directory: true, multiple: false });
    if (typeof dir === "string") await repos.addLocal(dir);
  };

  const addFromGit = async () => {
    const url = gitUrl.trim();
    if (!url) return;
    await repos.addGit(url);
    setGitUrl("");
  };

  return (
    <main className="home">
      <header className="home-header">
        <h2>{workspace.name}</h2>
        <button
          className="danger"
          onClick={() => {
            if (
              confirm(
                `Delete workspace "${workspace.name}"? Local folders stay on disk; app-managed clones are removed.`,
              )
            ) {
              void onDeleteWorkspace(workspace.id);
            }
          }}
        >
          Delete workspace
        </button>
      </header>

      <section className="home-section">
        <div className="home-section-header">
          <h3>Repositories</h3>
          <div className="repo-add">
            <button onClick={() => void addLocalFolder()}>+ Local folder</button>
            <input
              value={gitUrl}
              placeholder="https://github.com/org/repo.git"
              onChange={(e) => setGitUrl(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") void addFromGit();
              }}
            />
            <button onClick={() => void addFromGit()} disabled={repos.cloning || !gitUrl.trim()}>
              {repos.cloning ? "Cloning…" : "Clone"}
            </button>
          </div>
        </div>

        <ul className="repo-list">
          {repos.repositories.map((repo) => (
            <li key={repo.id}>
              <div>
                <span className="repo-name">{repo.name}</span>
                {repo.remote_url && <span className="badge external">cloned</span>}
                <div className="workspace-path" title={repo.remote_url ?? repo.root_path}>
                  {repo.remote_url ?? repo.root_path}
                </div>
              </div>
              <button
                className="danger"
                onClick={() => {
                  if (confirm(`Remove repository "${repo.name}" from this workspace?`)) {
                    void repos.remove(repo.id);
                  }
                }}
              >
                Remove
              </button>
            </li>
          ))}
          {repos.repositories.length === 0 && (
            <li className="empty">
              Add a local folder or clone a git repository to start.
            </li>
          )}
        </ul>

        <div className="index-row">
          <button
            className="primary"
            onClick={() => void repos.index()}
            disabled={repos.indexing || repos.repositories.length === 0}
          >
            {repos.indexing ? "Indexing…" : "Index all repositories"}
          </button>
          {repos.lastReport && (
            <span className="index-summary">
              {repos.lastReport.files_indexed} files indexed (
              {repos.lastReport.files_unchanged} unchanged),{" "}
              {repos.lastReport.chunks_created} chunks
              {repos.lastReport.files_with_secrets_redacted > 0 &&
                ` · 🔒 secrets redacted in ${repos.lastReport.files_with_secrets_redacted} file(s)`}
              {repos.lastReport.embedding_available
                ? ` · ${repos.lastReport.embeddings_created} embeddings`
                : " · keyword search only"}
            </span>
          )}
        </div>
        {repos.error && <div className="error">{repos.error}</div>}
      </section>

      <section className="home-section">
        <div className="home-section-header">
          <h3>Chats</h3>
          <button className="primary" onClick={onNewChat}>
            + New chat
          </button>
        </div>
        <ul className="session-list">
          {sessions.map((session) => (
            <li key={session.id} onClick={() => onOpenSession(session.id)}>
              <span className="session-title">{session.title}</span>
              <span className="session-date">
                {new Date(session.created_at).toLocaleString()}
              </span>
            </li>
          ))}
          {sessions.length === 0 && (
            <li className="empty">No chats yet — start one with "New chat".</li>
          )}
        </ul>
      </section>
    </main>
  );
}
