import { useState } from "react";
import "./App.css";
import { useWorkspaces } from "./application/useWorkspaces";
import { useProviders } from "./application/useProviders";
import { WorkspaceSidebar } from "./presentation/components/WorkspaceSidebar";
import { ChatView } from "./presentation/components/ChatView";
import { SettingsView } from "./presentation/components/SettingsView";

function App() {
  const ws = useWorkspaces();
  const providers = useProviders();
  const [settingsOpen, setSettingsOpen] = useState(false);

  return (
    <div className="app">
      <WorkspaceSidebar
        workspaces={ws.workspaces}
        selectedId={ws.selectedId}
        onSelect={ws.setSelectedId}
        onCreate={ws.create}
        onDelete={ws.remove}
        onIndex={ws.index}
        indexing={ws.indexing}
        lastReport={ws.lastReport}
        onOpenSettings={() => setSettingsOpen(true)}
      />
      <ChatView workspace={ws.selected} providers={providers.providers} />
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
