import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { open, save } from "@tauri-apps/plugin-dialog";
import type { Workspace } from "../../domain/types";
import { api } from "../../infrastructure/api";
import { useRepositories } from "../../application/useRepositories";
import { PublishPanel } from "./PublishPanel";
import { OllamaBanner } from "./OllamaBanner";

interface Props {
  workspace: Workspace;
  onDeleteWorkspace: (id: string) => Promise<void>;
  onNewDocument: () => void;
  onOpenDocument: (name: string) => void;
}

/**
 * Landing view for a workspace: manage its repositories and run indexing.
 * Chats are navigated from the sidebar.
 */
export function WorkspaceHome({
  workspace,
  onDeleteWorkspace,
  onNewDocument,
  onOpenDocument,
}: Props) {
  const repos = useRepositories(workspace.id);
  const [gitUrl, setGitUrl] = useState("");
  const [issuesSpec, setIssuesSpec] = useState("");
  const [notes, setNotes] = useState<{ name: string; updated_at: string }[]>([]);
  const [instructions, setInstructions] = useState(workspace.instructions);
  const [instructionsStatus, setInstructionsStatus] = useState<string | null>(null);
  const [backupStatus, setBackupStatus] = useState<string | null>(null);
  const { t } = useTranslation();

  useEffect(() => setInstructions(workspace.instructions), [workspace.id, workspace.instructions]);

  const saveInstructions = async () => {
    setInstructionsStatus(null);
    try {
      await api.setWorkspaceInstructions(workspace.id, instructions);
      setInstructionsStatus(t("workspace.saved"));
      setTimeout(() => setInstructionsStatus(null), 1500);
    } catch (e) {
      setInstructionsStatus(String(e));
    }
  };

  const exportWorkspace = async () => {
    const dest = await save({
      defaultPath: `${workspace.name}.cbnb.json`,
      filters: [{ name: "Workspace export", extensions: ["json"] }],
    });
    if (!dest) return;
    setBackupStatus(null);
    try {
      await api.exportWorkspace(workspace.id, dest);
      setBackupStatus(t("workspace.saved"));
      setTimeout(() => setBackupStatus(null), 1500);
    } catch (e) {
      setBackupStatus(String(e));
    }
  };

  useEffect(() => {
    void api.listNotes(workspace.id).then(setNotes);
  }, [workspace.id]);

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
            if (confirm(t("home.deleteWorkspaceConfirm", { name: workspace.name }))) {
              void onDeleteWorkspace(workspace.id);
            }
          }}
        >
          {t("home.deleteWorkspace")}
        </button>
      </header>

      <OllamaBanner />

      <section className="home-section">
        <div className="home-section-header">
          <h3>{t("home.sources")}</h3>
          <div className="repo-add">
            <button onClick={() => void addLocalFolder()}>{t("home.addFolder")}</button>
            <button onClick={() => void addLocalFile()}>{t("home.addFile")}</button>
          </div>
        </div>

        <div className="repo-add remote-row">
          <input
            value={gitUrl}
            placeholder={t("home.gitPlaceholder")}
            onChange={(e) => setGitUrl(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void addFromGit();
            }}
          />
          <button onClick={() => void addFromGit()} disabled={repos.cloning || !gitUrl.trim()}>
            {repos.cloning ? t("home.working") : t("home.clone")}
          </button>
        </div>
        <div className="repo-add remote-row">
          <input
            value={issuesSpec}
            placeholder={t("home.issuesPlaceholder")}
            onChange={(e) => setIssuesSpec(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void addIssues();
            }}
          />
          <button
            onClick={() => void addIssues()}
            disabled={repos.cloning || !issuesSpec.trim()}
          >
            {repos.cloning ? t("home.working") : t("home.fetchIssues")}
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
              <div className="repo-actions">
                {repo.source_kind !== "local" && (
                  <button
                    title={t("home.syncTitle")}
                    disabled={repos.cloning || repos.indexing}
                    onClick={() => void repos.sync(repo.id)}
                  >
                    {t("home.sync")}
                  </button>
                )}
                <button
                  className="danger"
                  onClick={() => {
                    if (confirm(t("home.removeRepoConfirm", { name: repo.name }))) {
                      void repos.remove(repo.id);
                    }
                  }}
                >
                  {t("home.remove")}
                </button>
              </div>
            </li>
          ))}
          {repos.repositories.length === 0 && (
            <li className="empty">{t("home.noSources")}</li>
          )}
        </ul>

        <div className="index-row">
          <button
            className="primary"
            onClick={() => void repos.index()}
            disabled={repos.indexing || repos.repositories.length === 0}
          >
            {repos.indexing ? t("home.indexing") : t("home.indexAll")}
          </button>
          {repos.lastReport && (
            <span className="index-summary">
              {t("home.indexSummary", {
                files: repos.lastReport.files_indexed,
                unchanged: repos.lastReport.files_unchanged,
                chunks: repos.lastReport.chunks_created,
              })}
              {repos.lastReport.files_with_secrets_redacted > 0 &&
                t("home.secretsRedacted", {
                  count: repos.lastReport.files_with_secrets_redacted,
                })}
              {repos.lastReport.embedding_available
                ? t("home.embeddings", { count: repos.lastReport.embeddings_created })
                : t("home.keywordOnly")}
            </span>
          )}
        </div>
        {repos.error && <div className="error">{repos.error}</div>}
      </section>

      <section className="home-section">
        <div className="home-section-header">
          <h3>{t("home.documents")}</h3>
          <button className="primary" onClick={onNewDocument}>
            {t("home.newDocument")}
          </button>
        </div>
        <ul className="session-list">
          {notes.map((note) => (
            <li key={note.name} onClick={() => onOpenDocument(note.name)}>
              <span className="session-title">{note.name.replace(/\.md$/, "")}</span>
              <span className="session-date">
                {note.updated_at ? new Date(note.updated_at).toLocaleString() : ""}
              </span>
            </li>
          ))}
          {notes.length === 0 && (
            <li className="empty">{t("home.noDocuments")}</li>
          )}
        </ul>
      </section>

      <section className="home-section">
        <div className="home-section-header">
          <h3>{t("workspace.instructions")}</h3>
          <div className="index-row">
            {instructionsStatus && (
              <span className="index-summary">{instructionsStatus}</span>
            )}
            <button className="primary" onClick={() => void saveInstructions()}>
              {t("editor.save")}
            </button>
          </div>
        </div>
        <p className="settings-note">{t("workspace.instructionsHint")}</p>
        <textarea
          className="instructions-input"
          rows={3}
          value={instructions}
          placeholder={t("workspace.instructionsPlaceholder")}
          onChange={(e) => setInstructions(e.target.value)}
        />
      </section>

      <section className="home-section">
        <div className="home-section-header">
          <h3>{t("workspace.backup")}</h3>
          <div className="repo-add">
            <button onClick={() => void exportWorkspace()}>{t("workspace.export")}</button>
            {backupStatus && <span className="index-summary">{backupStatus}</span>}
          </div>
        </div>
        <p className="settings-note">{t("workspace.exportHint")}</p>
      </section>

      <PublishPanel
        repositories={repos.repositories}
        onWikiPublished={() => void repos.index()}
      />

      <p className="home-hint">
        {t("home.chatsHint")}
      </p>
    </main>
  );
}
