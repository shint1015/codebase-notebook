import { useEffect, useRef, useState } from "react";
import type { ProviderConfig, ProviderKind, Workspace } from "../../domain/types";
import { EXTERNAL_PROVIDERS, PROVIDER_LABELS } from "../../domain/types";
import { useChat } from "../../application/useChat";
import { ConsentDialog } from "./ConsentDialog";
import { MessageBubble } from "./MessageBubble";

interface Props {
  workspace: Workspace | null;
  providers: ProviderConfig[];
}

export function ChatView({ workspace, providers }: Props) {
  const chat = useChat(workspace?.id ?? null);
  const [input, setInput] = useState("");
  const [provider, setProvider] = useState<ProviderKind>("ollama");
  const bottomRef = useRef<HTMLDivElement>(null);

  const enabled = providers.filter((p) => p.enabled);

  useEffect(() => {
    if (!enabled.some((p) => p.kind === provider)) {
      setProvider(enabled[0]?.kind ?? "ollama");
    }
  }, [providers]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [chat.messages.length, chat.busy]);

  if (!workspace) {
    return (
      <main className="chat-empty">
        <p>Select or add a workspace to start.</p>
      </main>
    );
  }

  const submit = async () => {
    const question = input;
    setInput("");
    await chat.send(question, provider);
  };

  return (
    <main className="chat">
      <header className="chat-header">
        <div>
          <h2>{workspace.name}</h2>
          <span className="workspace-path">{workspace.root_path}</span>
        </div>
        <div className="session-controls">
          <select
            value={chat.sessionId ?? ""}
            onChange={(e) => e.target.value && chat.selectSession(e.target.value)}
          >
            <option value="">— new chat —</option>
            {chat.sessions.map((s) => (
              <option key={s.id} value={s.id}>
                {s.title}
              </option>
            ))}
          </select>
          <button onClick={chat.startNewSession}>New chat</button>
        </div>
      </header>

      <div className="messages">
        {chat.messages.length === 0 && !chat.busy && (
          <div className="chat-hint">
            Ask about this workspace — answers cite the indexed sources.
            <br />
            Local model by default; external providers always ask before sending.
          </div>
        )}
        {chat.messages.map((m) => (
          <MessageBubble key={m.id} message={m} />
        ))}
        {chat.busy && <div className="thinking">Thinking…</div>}
        {chat.error && <div className="error">{chat.error}</div>}
        <div ref={bottomRef} />
      </div>

      <footer className="composer">
        <select
          value={provider}
          onChange={(e) => setProvider(e.target.value as ProviderKind)}
          title="Model provider for this question"
        >
          {enabled.map((p) => (
            <option key={p.kind} value={p.kind}>
              {PROVIDER_LABELS[p.kind]}
              {EXTERNAL_PROVIDERS.includes(p.kind) ? " ↗" : ""}
            </option>
          ))}
        </select>
        <textarea
          value={input}
          placeholder="Ask about your code and docs… (Enter to send, Shift+Enter for newline)"
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
              e.preventDefault();
              if (!chat.busy && input.trim()) void submit();
            }
          }}
          rows={2}
        />
        <button
          className="primary"
          disabled={chat.busy || !input.trim()}
          onClick={() => void submit()}
        >
          Send
        </button>
      </footer>

      {chat.pendingConsent && (
        <ConsentDialog
          pending={chat.pendingConsent}
          onApprove={() => void chat.approveConsent()}
          onLocalOnly={() => void chat.declineConsent(true)}
          onCancel={() => void chat.declineConsent(false)}
        />
      )}
    </main>
  );
}
