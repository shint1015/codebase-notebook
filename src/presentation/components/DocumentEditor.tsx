import { useEffect, useState } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import type { Workspace } from "../../domain/types";
import { api } from "../../infrastructure/api";
import { isCommandError } from "../../domain/types";

interface Props {
  workspace: Workspace;
  /** Existing note file name, or null for a new document. */
  noteName: string | null;
  onClose: () => void;
  /** Called after a save so the workspace can re-index. */
  onSaved: () => void;
}

type ViewMode = "split" | "edit" | "preview";

const NEW_TEMPLATE = "# Untitled\n\nStart writing…\n";

export function DocumentEditor({ workspace, noteName, onClose, onSaved }: Props) {
  const [title, setTitle] = useState(noteName?.replace(/\.md$/, "") ?? "");
  const [content, setContent] = useState(noteName ? "" : NEW_TEMPLATE);
  const [mode, setMode] = useState<ViewMode>("split");
  const [status, setStatus] = useState<string | null>(null);
  const [dirty, setDirty] = useState(false);
  const [loading, setLoading] = useState(!!noteName);

  useEffect(() => {
    if (!noteName) return;
    setLoading(true);
    api
      .readNote(workspace.id, noteName)
      .then((text) => {
        setContent(text);
        setDirty(false);
      })
      .catch((e) => setStatus(isCommandError(e) ? e.message : String(e)))
      .finally(() => setLoading(false));
  }, [workspace.id, noteName]);

  const save = async () => {
    const name = title.trim();
    if (!name) {
      setStatus("Give the document a title first.");
      return;
    }
    setStatus("Saving…");
    try {
      await api.saveNote(workspace.id, name, content);
      setDirty(false);
      setStatus("Saved · indexing…");
      onSaved();
    } catch (e) {
      setStatus(isCommandError(e) ? e.message : String(e));
    }
  };

  const preview = (
    <div className="doc-preview markdown">
      <ReactMarkdown remarkPlugins={[remarkGfm]}>{content}</ReactMarkdown>
    </div>
  );
  const editor = (
    <textarea
      className="doc-textarea"
      value={content}
      spellCheck={false}
      onChange={(e) => {
        setContent(e.target.value);
        setDirty(true);
      }}
      onKeyDown={(e) => {
        if (e.key === "s" && (e.ctrlKey || e.metaKey)) {
          e.preventDefault();
          void save();
        }
      }}
    />
  );

  return (
    <main className="doc-editor">
      <header className="chat-header">
        <div className="chat-title">
          <button onClick={onClose} title="Back to workspace">
            ←
          </button>
          <input
            className="doc-title-input"
            value={title}
            placeholder="Document title"
            onChange={(e) => {
              setTitle(e.target.value);
              setDirty(true);
            }}
          />
          {dirty && <span className="doc-dirty">●</span>}
        </div>
        <div className="doc-actions">
          <div className="doc-mode">
            {(["split", "edit", "preview"] as ViewMode[]).map((m) => (
              <button
                key={m}
                className={mode === m ? "active" : ""}
                onClick={() => setMode(m)}
              >
                {m === "split" ? "◫ Split" : m === "edit" ? "✎ Edit" : "▤ Preview"}
              </button>
            ))}
          </div>
          <button className="primary" onClick={() => void save()}>
            Save
          </button>
        </div>
      </header>

      {status && <div className="doc-status">{status}</div>}

      {loading ? (
        <div className="doc-loading">Loading…</div>
      ) : (
        <div className={`doc-body ${mode}`}>
          {mode !== "preview" && <div className="doc-pane">{editor}</div>}
          {mode !== "edit" && <div className="doc-pane">{preview}</div>}
        </div>
      )}
    </main>
  );
}
