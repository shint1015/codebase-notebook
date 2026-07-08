import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import type { IndexReport, Workspace } from "../../domain/types";
import { useAppVersion } from "../../application/useAppVersion";

interface Props {
  workspaces: Workspace[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  onCreate: (name: string, rootPath: string) => Promise<unknown>;
  onDelete: (id: string) => Promise<void>;
  onIndex: (id: string) => Promise<IndexReport>;
  indexing: boolean;
  lastReport: IndexReport | null;
  onOpenSettings: () => void;
}

export function WorkspaceSidebar({
  workspaces,
  selectedId,
  onSelect,
  onCreate,
  onDelete,
  onIndex,
  indexing,
  lastReport,
  onOpenSettings,
}: Props) {
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const version = useAppVersion();

  const addWorkspace = async () => {
    setError(null);
    const dir = await open({ directory: true, multiple: false });
    if (typeof dir !== "string") return;
    const name = dir.split(/[/\\]/).filter(Boolean).pop() ?? dir;
    setCreating(true);
    try {
      await onCreate(name, dir);
    } catch (e) {
      setError(String(e));
    } finally {
      setCreating(false);
    }
  };

  return (
    <aside className="sidebar">
      <div className="sidebar-header">
        <h1>Codebase Notebook</h1>
        <p className="tagline">Local-first · source-grounded</p>
      </div>

      <button className="primary" onClick={addWorkspace} disabled={creating}>
        + Add workspace
      </button>

      <ul className="workspace-list">
        {workspaces.map((ws) => (
          <li
            key={ws.id}
            className={ws.id === selectedId ? "selected" : ""}
            onClick={() => onSelect(ws.id)}
          >
            <div className="workspace-name">{ws.name}</div>
            <div className="workspace-path" title={ws.root_path}>
              {ws.root_path}
            </div>
            {ws.id === selectedId && (
              <div className="workspace-actions">
                <button
                  onClick={(e) => {
                    e.stopPropagation();
                    void onIndex(ws.id);
                  }}
                  disabled={indexing}
                >
                  {indexing ? "Indexing…" : "Re-index"}
                </button>
                <button
                  className="danger"
                  onClick={(e) => {
                    e.stopPropagation();
                    if (confirm(`Delete workspace "${ws.name}"? The source files stay on disk.`)) {
                      void onDelete(ws.id);
                    }
                  }}
                >
                  Delete
                </button>
              </div>
            )}
          </li>
        ))}
        {workspaces.length === 0 && (
          <li className="empty">Add a folder to start asking questions about it.</li>
        )}
      </ul>

      {lastReport && (
        <div className="index-report">
          <strong>Last index</strong>
          <div>{lastReport.files_indexed} files indexed ({lastReport.files_unchanged} unchanged)</div>
          <div>{lastReport.chunks_created} chunks</div>
          {lastReport.files_with_secrets_redacted > 0 && (
            <div className="warn">
              🔒 secrets redacted in {lastReport.files_with_secrets_redacted} file(s)
            </div>
          )}
          <div>
            {lastReport.embedding_available
              ? `${lastReport.embeddings_created} embeddings (hybrid search on)`
              : "keyword search only (no local embedder found)"}
          </div>
        </div>
      )}
      {error && <div className="error">{error}</div>}

      <div className="sidebar-footer">
        <button onClick={onOpenSettings}>⚙ AI Providers</button>
        {version && <span className="app-version">v{version}</span>}
      </div>
    </aside>
  );
}
