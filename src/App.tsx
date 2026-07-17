import { useEffect, useState } from "react";
import "./App.css";
import type { ChatSession } from "./domain/types";
import { api } from "./infrastructure/api";
import { useWorkspaces } from "./application/useWorkspaces";
import { useProviders } from "./application/useProviders";
import { useSessions } from "./application/useSessions";
import { WorkspaceSidebar } from "./presentation/components/WorkspaceSidebar";
import { WorkspaceHome } from "./presentation/components/WorkspaceHome";
import { ChatView } from "./presentation/components/ChatView";
import { DocumentEditor } from "./presentation/components/DocumentEditor";
import { SourceEditor } from "./presentation/components/SourceEditor";
import { SettingsView } from "./presentation/components/SettingsView";

type View =
  | { kind: "home" }
  | { kind: "chat"; session: ChatSession | null }
  | { kind: "doc"; name: string | null }
  | { kind: "source"; relPath: string; line: number; from: ChatSession | null };

function App() {
  const ws = useWorkspaces();
  const providers = useProviders();
  const sessions = useSessions(ws.selectedId);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [view, setView] = useState<View>({ kind: "home" });

  // Selecting another workspace always lands on its home view.
  useEffect(() => {
    setView({ kind: "home" });
  }, [ws.selectedId]);

  const openSession = (sessionId: string) => {
    const session = sessions.sessions.find((s) => s.id === sessionId) ?? null;
    if (session) setView({ kind: "chat", session });
  };

  const activeSessionId = view.kind === "chat" ? (view.session?.id ?? null) : null;

  return (
    <div className="app">
      <WorkspaceSidebar
        workspaces={ws.workspaces}
        selectedId={ws.selectedId}
        sessions={sessions.sessions}
        activeSessionId={activeSessionId}
        collapsed={sidebarCollapsed}
        onToggleCollapse={() => setSidebarCollapsed((c) => !c)}
        onSelectWorkspace={(id) => {
          ws.setSelectedId(id);
          setView({ kind: "home" });
        }}
        onOpenSession={openSession}
        onNewChat={() => setView({ kind: "chat", session: null })}
        onCreateWorkspace={ws.create}
        onRenameSession={async (id, title) => {
          await api.renameChatSession(id, title);
          await sessions.refresh();
          setView((v) =>
            v.kind === "chat" && v.session?.id === id
              ? { kind: "chat", session: { ...v.session, title } }
              : v,
          );
        }}
        onDeleteSession={async (id) => {
          await api.deleteChatSession(id);
          await sessions.refresh();
          setView((v) =>
            v.kind === "chat" && v.session?.id === id ? { kind: "home" } : v,
          );
        }}
        onOpenSettings={() => setSettingsOpen(true)}
      />

      {!ws.selected ? (
        <main className="chat-empty">
          <p>Create or select a workspace to start.</p>
        </main>
      ) : view.kind === "home" ? (
        <WorkspaceHome
          workspace={ws.selected}
          onDeleteWorkspace={ws.remove}
          onNewDocument={() => setView({ kind: "doc", name: null })}
          onOpenDocument={(name) => setView({ kind: "doc", name })}
        />
      ) : view.kind === "source" ? (
        <SourceEditor
          workspace={ws.selected}
          relPath={view.relPath}
          line={view.line}
          onClose={() =>
            setView(
              view.from
                ? { kind: "chat", session: view.from }
                : { kind: "home" },
            )
          }
        />
      ) : view.kind === "doc" ? (
        <DocumentEditor
          workspace={ws.selected}
          noteName={view.name}
          onClose={() => setView({ kind: "home" })}
          onSaved={() => void api.indexWorkspace(ws.selected!.id)}
        />
      ) : (
        <ChatView
          workspace={ws.selected}
          session={view.session}
          providers={providers.providers}
          onSessionCreated={(session) => {
            setView({ kind: "chat", session });
            void sessions.refresh();
          }}
          onForked={(session) => {
            void sessions.refresh();
            setView({ kind: "chat", session });
          }}
          onDocumentized={() => void api.indexWorkspace(ws.selected!.id)}
          onOpenSource={(relPath, line) =>
            setView((v) => ({
              kind: "source",
              relPath,
              line,
              from: v.kind === "chat" ? v.session : null,
            }))
          }
          onBack={() => setView({ kind: "home" })}
        />
      )}

      {settingsOpen && (
        <SettingsView
          providers={providers.providers}
          onConfigure={providers.configure}
          onTest={providers.test}
          onClose={() => setSettingsOpen(false)}
        />
      )}
    </div>
  );
}

export default App;
