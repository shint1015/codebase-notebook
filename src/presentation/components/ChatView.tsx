import { useEffect, useRef, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import type {
  ChatSession,
  ProviderConfig,
  ProviderKind,
  Workspace,
} from "../../domain/types";
import { EXTERNAL_PROVIDERS, PROVIDER_LABELS } from "../../domain/types";
import { useChat } from "../../application/useChat";
import { api } from "../../infrastructure/api";
import { ConsentDialog } from "./ConsentDialog";
import { MessageBubble } from "./MessageBubble";

interface Props {
  workspace: Workspace;
  session: ChatSession | null;
  providers: ProviderConfig[];
  onSessionCreated: (session: ChatSession) => void;
  onForked: (session: ChatSession) => void;
  onDocumentized: () => void;
  onOpenSource: (relPath: string, line: number) => void;
  onBack: () => void;
}

export function ChatView({
  workspace,
  session,
  providers,
  onSessionCreated,
  onForked,
  onDocumentized,
  onOpenSource,
  onBack,
}: Props) {
  const chat = useChat(workspace.id, session?.id ?? null, onSessionCreated);
  const [input, setInput] = useState("");
  const [provider, setProvider] = useState<ProviderKind>("ollama");
  const [agentMode, setAgentMode] = useState(false);
  const [allowWrites, setAllowWrites] = useState(false);
  const bottomRef = useRef<HTMLDivElement>(null);

  const enabled = providers.filter((p) => p.enabled);

  useEffect(() => {
    if (!enabled.some((p) => p.kind === provider)) {
      setProvider(enabled[0]?.kind ?? "ollama");
    }
  }, [providers]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [chat.messages.length, chat.busy, chat.streamingText]);

  const [actionStatus, setActionStatus] = useState<string | null>(null);

  const submit = async () => {
    const question = input;
    setInput("");
    await chat.send(question, provider, agentMode, agentMode && allowWrites);
  };

  const fork = async (upToMessageId?: string) => {
    if (!session) return;
    const forked = await api.forkChatSession(session.id, upToMessageId);
    onForked(forked);
  };

  const copy = async () => {
    if (!session) return;
    const md = await api.chatMarkdown(session.id);
    await navigator.clipboard.writeText(md);
    setActionStatus("Copied to clipboard");
    setTimeout(() => setActionStatus(null), 2000);
  };

  const documentize = async () => {
    if (!session) return;
    const title = prompt("Save chat as document — title:", session.title);
    if (!title) return;
    await api.chatToDocument(workspace.id, session.id, title);
    setActionStatus("Saved as document");
    setTimeout(() => setActionStatus(null), 2000);
    onDocumentized();
  };

  return (
    <main className="chat">
      <header className="chat-header">
        <div className="chat-title">
          <button onClick={onBack} title="Back to workspace">
            ←
          </button>
          <div>
            <h2>{session?.title ?? "New chat"}</h2>
            <span className="workspace-path">{workspace.name}</span>
          </div>
        </div>
        <div className="chat-header-actions">
          {actionStatus && <span className="action-status">{actionStatus}</span>}
          {session && chat.messages.length > 0 && (
            <>
              <button title="Copy the whole transcript" onClick={() => void copy()}>
                ⧉ Copy
              </button>
              <button title="Save chat as an in-app document" onClick={() => void documentize()}>
                ▤ Save as doc
              </button>
              <button
                title="Export chat as a markdown file"
                onClick={async () => {
                  const dest = await save({
                    defaultPath: `${session.title.slice(0, 40)}.md`,
                    filters: [{ name: "Markdown", extensions: ["md"] }],
                  });
                  if (dest) await api.exportChat(session.id, dest);
                }}
              >
                ⤓ Export
              </button>
            </>
          )}
          <select
            className="provider-select"
            value={provider}
            onChange={(e) => setProvider(e.target.value as ProviderKind)}
            title="Model provider for this chat"
          >
            {enabled.map((p) => (
              <option key={p.kind} value={p.kind}>
                {PROVIDER_LABELS[p.kind]}
                {EXTERNAL_PROVIDERS.includes(p.kind) ? " ↗" : ""}
              </option>
            ))}
          </select>
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
          <div key={m.id} className={`message-row ${m.role}`}>
            {chat.toolEvents[m.id] && (
              <div className="tool-trace">
                {chat.toolEvents[m.id].map((event, i) => (
                  <div
                    key={i}
                    className={`tool-event ${event.blocked ? "blocked" : ""}`}
                    title={event.result}
                  >
                    {event.blocked ? "⛔" : "🛠"} {event.summary}
                    {event.blocked && " (needs approval)"}
                  </div>
                ))}
              </div>
            )}
            <MessageBubble
              message={m}
              workspaceId={workspace.id}
              onFork={(messageId) => void fork(messageId)}
              onOpenSource={onOpenSource}
            />
          </div>
        ))}
        {chat.streamingText !== null &&
          (chat.streamingText === "" ? (
            <div className="thinking">Thinking…</div>
          ) : (
            <div className="message assistant streaming">
              <div className="message-content">{chat.streamingText}</div>
            </div>
          ))}
        {chat.error && <div className="error">{chat.error}</div>}
        <div ref={bottomRef} />
      </div>

      <div className="composer-tools">
        <label className="agent-toggle">
          <input
            type="checkbox"
            checked={agentMode}
            onChange={(e) => setAgentMode(e.target.checked)}
          />
          🛠 Agent mode
        </label>
        {agentMode && (
          <>
            <label className="agent-toggle" title="Let the agent create issues / write wiki pages">
              <input
                type="checkbox"
                checked={allowWrites}
                onChange={(e) => setAllowWrites(e.target.checked)}
              />
              Allow actions (create issues, write wiki)
            </label>
            <span className="agent-hint">
              The agent can search sources and take the enabled actions.
            </span>
          </>
        )}
      </div>

      <footer className="composer">
        <textarea
          value={input}
          placeholder={
            agentMode
              ? "Tell the agent what to do… (Ctrl+Enter or ⌘+Enter)"
              : "Ask about your code and docs… (Ctrl+Enter or ⌘+Enter to send)"
          }
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => {
            if (
              e.key === "Enter" &&
              (e.ctrlKey || e.metaKey) &&
              !e.nativeEvent.isComposing
            ) {
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
