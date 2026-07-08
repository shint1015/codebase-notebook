import { useState } from "react";
import type { Message } from "../../domain/types";

export function MessageBubble({ message }: { message: Message }) {
  const [openCitation, setOpenCitation] = useState<number | null>(null);

  return (
    <div className={`message ${message.role}`}>
      <div className="message-content">{message.content}</div>
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
                    {c.rel_path} (lines {c.start_line}–{c.end_line})
                  </div>
                  {c.snippet}
                </pre>
              ))}
        </div>
      )}
    </div>
  );
}
