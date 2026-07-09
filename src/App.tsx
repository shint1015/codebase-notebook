import { useEffect, useState } from "react";
import "./App.css";
import type { ChatSession } from "./domain/types";
import { api } from "./infrastructure/api";
import { useWorkspaces } from "./application/useWorkspaces";
import { useProviders } from "./application/useProviders";
import { WorkspaceSidebar } from "./presentation/components/WorkspaceSidebar";
import { WorkspaceHome } from "./presentation/components/WorkspaceHome";
import { ChatView } from "./presentation/components/ChatView";
import { SettingsView } from "./presentation/components/SettingsView";

type View = { kind: "home" } | { kind: "chat"; session: ChatSession | null };

function App() {
  const ws = useWorkspaces();
  const providers = useProviders();
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [view, setView] = useState<View>({ kind: "home" });

  // Selecting another workspace always lands on its home view.
  useEffect(() => {
    setView({ kind: "home" });
  }, [ws.selectedId]);

  const openSession = async (sessionId: string) => {
    if (!ws.selectedId) return;
    const sessions = await api.listChatSessions(ws.selectedId);
    const session = sessions.find((s) => s.id === sessionId) ?? null;
    setView({ kind: "chat", session });
  };

  return (
    <div className="app">
      <WorkspaceSidebar
        workspaces={ws.workspaces}
        selectedId={ws.selectedId}
        onSelect={ws.setSelectedId}
        onCreate={ws.create}
        onOpenSettings={() => setSettingsOpen(true)}
      />

      {!ws.selected ? (
        <main className="chat-empty">
          <p>Create or select a workspace to start.</p>
        </main>
      ) : view.kind === "home" ? (
        <WorkspaceHome
          workspace={ws.selected}
          onOpenSession={(id) => void openSession(id)}
          onNewChat={() => setView({ kind: "chat", session: null })}
          onDeleteWorkspace={ws.remove}
        />
      ) : (
        <ChatView
          workspace={ws.selected}
          session={view.session}
          providers={providers.providers}
          onSessionCreated={(session) => setView({ kind: "chat", session })}
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
