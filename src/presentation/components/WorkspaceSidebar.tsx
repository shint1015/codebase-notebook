import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { ChatSession, Workspace } from "../../domain/types";
import { useAppVersion } from "../../application/useAppVersion";

interface Props {
  workspaces: Workspace[];
  selectedId: string | null;
  sessions: ChatSession[];
  activeSessionId: string | null;
  collapsed: boolean;
  onToggleCollapse: () => void;
  onSelectWorkspace: (id: string) => void;
  onOpenSession: (sessionId: string) => void;
  onNewChat: () => void;
  onCreateWorkspace: (name: string) => Promise<unknown>;
  onRenameSession: (sessionId: string, title: string) => Promise<void>;
  onDeleteSession: (sessionId: string) => Promise<void>;
  onOpenSettings: () => void;
}

export function WorkspaceSidebar({
  workspaces,
  selectedId,
  sessions,
  activeSessionId,
  collapsed,
  onToggleCollapse,
  onSelectWorkspace,
  onOpenSession,
  onNewChat,
  onCreateWorkspace,
  onRenameSession,
  onDeleteSession,
  onOpenSettings,
}: Props) {
  const [adding, setAdding] = useState(false);
  const [name, setName] = useState("");
  const [renamingId, setRenamingId] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState("");
  const [error, setError] = useState<string | null>(null);
  const version = useAppVersion();
  const { t } = useTranslation();

  const commitRename = async () => {
    if (renamingId && renameValue.trim()) {
      await onRenameSession(renamingId, renameValue.trim());
    }
    setRenamingId(null);
  };

  const create = async () => {
    const trimmed = name.trim();
    if (!trimmed) return;
    setError(null);
    try {
      await onCreateWorkspace(trimmed);
      setName("");
      setAdding(false);
    } catch (e) {
      setError(String(e));
    }
  };

  if (collapsed) {
    return (
      <aside className="sidebar collapsed">
        <button className="icon-button" onClick={onToggleCollapse} title={t("sidebar.expand")}>
          »
        </button>
        <div className="rail-workspaces">
          {workspaces.map((ws) => (
            <button
              key={ws.id}
              className={`icon-button rail-item ${ws.id === selectedId ? "active" : ""}`}
              title={ws.name}
              onClick={() => onSelectWorkspace(ws.id)}
            >
              {ws.name.slice(0, 1).toUpperCase()}
            </button>
          ))}
        </div>
        <div className="rail-footer">
          <button className="icon-button" onClick={onOpenSettings} title={t("sidebar.providers")}>
            ⚙
          </button>
        </div>
      </aside>
    );
  }

  return (
    <aside className="sidebar">
      <div className="sidebar-header">
        <div>
          <h1>Codebase Notebook</h1>
          <p className="tagline">{t("app.tagline")}</p>
        </div>
        <button className="icon-button" onClick={onToggleCollapse} title={t("sidebar.collapse")}>
          «
        </button>
      </div>

      {adding ? (
        <div className="workspace-add">
          <input
            autoFocus
            value={name}
            placeholder={t("sidebar.workspaceName")}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void create();
              if (e.key === "Escape") setAdding(false);
            }}
          />
          <button className="primary" onClick={() => void create()} disabled={!name.trim()}>
            {t("sidebar.add")}
          </button>
        </div>
      ) : (
        <button className="primary" onClick={() => setAdding(true)}>
          {t("sidebar.addWorkspace")}
        </button>
      )}

      <ul className="workspace-list">
        {workspaces.map((ws) => (
          <li key={ws.id} className={ws.id === selectedId ? "selected" : ""}>
            <div className="workspace-row" onClick={() => onSelectWorkspace(ws.id)}>
              <span className="workspace-name">{ws.name}</span>
            </div>
            {ws.id === selectedId && (
              <div className="session-nav">
                <button className="new-chat" onClick={onNewChat}>
                  {t("sidebar.newChat")}
                </button>
                <ul>
                  {sessions.map((session) => (
                    <li
                      key={session.id}
                      className={session.id === activeSessionId ? "active" : ""}
                      onClick={(e) => {
                        e.stopPropagation();
                        if (renamingId !== session.id) onOpenSession(session.id);
                      }}
                      title={session.title}
                    >
                      {renamingId === session.id ? (
                        <input
                          autoFocus
                          value={renameValue}
                          onClick={(e) => e.stopPropagation()}
                          onChange={(e) => setRenameValue(e.target.value)}
                          onKeyDown={(e) => {
                            if (e.key === "Enter") void commitRename();
                            if (e.key === "Escape") setRenamingId(null);
                          }}
                          onBlur={() => void commitRename()}
                        />
                      ) : (
                        <>
                          <span className="session-label">{session.title}</span>
                          <span className="session-actions">
                            <button
                              title={t("sidebar.rename")}
                              onClick={(e) => {
                                e.stopPropagation();
                                setRenamingId(session.id);
                                setRenameValue(session.title);
                              }}
                            >
                              ✎
                            </button>
                            <button
                              title={t("sidebar.delete")}
                              onClick={(e) => {
                                e.stopPropagation();
                                if (confirm(t("sidebar.deleteChatConfirm", { title: session.title }))) {
                                  void onDeleteSession(session.id);
                                }
                              }}
                            >
                              ×
                            </button>
                          </span>
                        </>
                      )}
                    </li>
                  ))}
                  {sessions.length === 0 && <li className="empty">{t("sidebar.noChats")}</li>}
                </ul>
              </div>
            )}
          </li>
        ))}
        {workspaces.length === 0 && (
          <li className="empty">{t("sidebar.createToStart")}</li>
        )}
      </ul>

      {error && <div className="error">{error}</div>}

      <div className="sidebar-footer">
        <button onClick={onOpenSettings}>{t("sidebar.providers")}</button>
        {version && <span className="app-version">v{version}</span>}
      </div>
    </aside>
  );
}
