import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import type { Workspace } from "../../domain/types";
import { useRepositories } from "../../application/useRepositories";
import { PublishPanel } from "./PublishPanel";

interface Props {
  workspace: Workspace;
  onDeleteWorkspace: (id: string) => Promise<void>;
}

/**
 * Landing view for a workspace: manage its repositories and run indexing.
 * Chats are navigated from the sidebar.
 */
export function WorkspaceHome({ workspace, onDeleteWorkspace }: Props) {
  const repos = useRepositories(workspace.id);
  const [gitUrl, setGitUrl] = useState("");
  const [issuesSpec, setIssuesSpec] = useState("");

  const addLocalFolder = async () => {
    const dir = await open({ directory: true, multiple: false });
    if (typeof dir === "string") await repos.addLocal(dir);
  };

  const addLocalFile = async () => {
    const file = await open({ directory: false, multiple: false });
    if (typeof file === "string") await repos.addLocal(file);
  };

  const addFromGit = async () => {
    const url = gitUrl.trim();
    if (!url) return;
    await repos.addGit(url);
    setGitUrl("");
  };

  const addIssues = async () => {
    const spec = issuesSpec.trim();
    if (!spec) return;
    await repos.addGithubIssues(spec);
    setIssuesSpec("");
  };

  const sourceBadge = (kind: string) => {
    if (kind === "git") return <span className="badge external">cloned</span>;
    if (kind === "github_issues") return <span className="badge external">issues</span>;
    return null;
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
          <h3>Sources</h3>
          <div className="repo-add">
            <button onClick={() => void addLocalFolder()}>+ Folder</button>
            <button onClick={() => void addLocalFile()}>+ File</button>
          </div>
        </div>

        <div className="repo-add remote-row">
          <input
            value={gitUrl}
            placeholder="git URL — repo or wiki (…/repo.wiki.git)"
            onChange={(e) => setGitUrl(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void addFromGit();
            }}
          />
          <button onClick={() => void addFromGit()} disabled={repos.cloning || !gitUrl.trim()}>
            {repos.cloning ? "Working…" : "Clone"}
          </button>
        </div>
        <div className="repo-add remote-row">
          <input
            value={issuesSpec}
            placeholder="GitHub issues — owner/repo"
            onChange={(e) => setIssuesSpec(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void addIssues();
            }}
          />
          <button
            onClick={() => void addIssues()}
            disabled={repos.cloning || !issuesSpec.trim()}
          >
            {repos.cloning ? "Working…" : "Fetch issues"}
          </button>
        </div>

        <ul className="repo-list">
          {repos.repositories.map((repo) => (
            <li key={repo.id}>
              <div>
                <span className="repo-name">{repo.name}</span>
                {sourceBadge(repo.source_kind)}
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
              Add a local folder/file, clone a git repository or wiki, or fetch
              GitHub issues to start.
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

      <PublishPanel
        repositories={repos.repositories}
        onWikiPublished={() => void repos.index()}
      />

      <p className="home-hint">
        Chats live in the sidebar — pick one or start a "+ New chat".
      </p>
    </main>
  );
}
