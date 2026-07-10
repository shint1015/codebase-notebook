import { useState } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import type { Message } from "../../domain/types";
import { api } from "../../infrastructure/api";

export function MessageBubble({
  message,
  workspaceId,
  onFork,
}: {
  message: Message;
  workspaceId: string;
  /** Fork the conversation up to and including this message. */
  onFork: (messageId: string) => void;
}) {
  const [openCitation, setOpenCitation] = useState<number | null>(null);
  const [revealError, setRevealError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const copy = async () => {
    await navigator.clipboard.writeText(message.content);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };

  return (
    <div className={`message ${message.role}`}>
      <div className="message-actions">
        <button title={copied ? "Copied" : "Copy message"} onClick={() => void copy()}>
          {copied ? "✓" : "⧉"}
        </button>
        <button title="Fork the chat from here" onClick={() => onFork(message.id)}>
          ⑂
        </button>
      </div>
      {message.role === "assistant" ? (
        <div className="message-content markdown">
          <ReactMarkdown remarkPlugins={[remarkGfm]}>{message.content}</ReactMarkdown>
        </div>
      ) : (
        <div className="message-content">{message.content}</div>
      )}
      {message.role === "assistant" && (
        <div className="message-meta">
          {message.provider && (
            <span className="model-tag">
              {message.provider} · {message.model}
            </span>
          )}
          {message.citations.length > 0 && (
            <div className="citations">
              {message.citations.map((c) => (
                <button
                  key={c.marker}
                  className="citation-chip"
                  onClick={() =>
                    setOpenCitation(openCitation === c.marker ? null : c.marker)
                  }
                  title={`${c.rel_path} lines ${c.start_line}-${c.end_line}`}
                >
                  [{c.marker}] {c.rel_path}:{c.start_line}
                </button>
              ))}
            </div>
          )}
          {openCitation !== null &&
            message.citations
              .filter((c) => c.marker === openCitation)
              .map((c) => (
                <pre key={c.marker} className="citation-snippet">
                  <div className="citation-source">
                    <span>
                      {c.rel_path} (lines {c.start_line}–{c.end_line})
                    </span>
                    <button
                      className="citation-open"
                      title="Open in editor"
                      onClick={() => {
                        setRevealError(null);
                        api
                          .revealSource(workspaceId, c.rel_path, c.start_line)
                          .catch((e) => setRevealError(String(e)));
                      }}
                    >
                      Open ↗
                    </button>
                  </div>
                  {c.snippet}
                </pre>
              ))}
          {revealError && <div className="error">{revealError}</div>}
        </div>
      )}
    </div>
  );
}
