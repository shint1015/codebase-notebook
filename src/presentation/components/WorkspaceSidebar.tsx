import { useState } from "react";
import type { Workspace } from "../../domain/types";
import { useAppVersion } from "../../application/useAppVersion";

interface Props {
  workspaces: Workspace[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  onCreate: (name: string) => Promise<unknown>;
  onOpenSettings: () => void;
}

export function WorkspaceSidebar({
  workspaces,
  selectedId,
  onSelect,
  onCreate,
  onOpenSettings,
}: Props) {
  const [adding, setAdding] = useState(false);
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const version = useAppVersion();

  const create = async () => {
    const trimmed = name.trim();
    if (!trimmed) return;
    setError(null);
    try {
      await onCreate(trimmed);
      setName("");
      setAdding(false);
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <aside className="sidebar">
      <div className="sidebar-header">
        <h1>Codebase Notebook</h1>
        <p className="tagline">Local-first · source-grounded</p>
      </div>

      {adding ? (
        <div className="workspace-add">
          <input
            autoFocus
            value={name}
            placeholder="Workspace name"
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void create();
              if (e.key === "Escape") setAdding(false);
            }}
          />
          <button className="primary" onClick={() => void create()} disabled={!name.trim()}>
            Add
          </button>
        </div>
      ) : (
        <button className="primary" onClick={() => setAdding(true)}>
          + Add workspace
        </button>
      )}

      <ul className="workspace-list">
        {workspaces.map((ws) => (
          <li
            key={ws.id}
            className={ws.id === selectedId ? "selected" : ""}
            onClick={() => onSelect(ws.id)}
          >
            <div className="workspace-name">{ws.name}</div>
          </li>
        ))}
        {workspaces.length === 0 && (
          <li className="empty">Create a workspace to start.</li>
        )}
      </ul>

      {error && <div className="error">{error}</div>}

      <div className="sidebar-footer">
        <button onClick={onOpenSettings}>⚙ AI Providers</button>
        {version && <span className="app-version">v{version}</span>}
      </div>
    </aside>
  );
}
