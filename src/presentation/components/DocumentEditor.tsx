import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import type { Workspace } from "../../domain/types";
import { api } from "../../infrastructure/api";
import { isCommandError } from "../../domain/types";
import { CodeEditor } from "./CodeEditor";

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
  const { t } = useTranslation();

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
      setStatus(t("editor.needTitle"));
      return;
    }
    setStatus(t("editor.saving"));
    try {
      await api.saveNote(workspace.id, name, content);
      setDirty(false);
      setStatus(t("editor.savedIndexing"));
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
    <CodeEditor
      value={content}
      fileName={`${title || "untitled"}.md`}
      onChange={(next) => {
        setContent(next);
        setDirty(true);
      }}
      onSave={() => void save()}
    />
  );

  return (
    <main className="doc-editor">
      <header className="chat-header">
        <div className="chat-title">
          <button onClick={onClose} title={t("chat.back")}>
            ←
          </button>
          <input
            className="doc-title-input"
            value={title}
            placeholder={t("editor.title")}
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
                {m === "split" ? t("editor.split") : m === "edit" ? t("editor.edit") : t("editor.preview")}
              </button>
            ))}
          </div>
          <button className="primary" onClick={() => void save()}>
            {t("editor.save")}
          </button>
        </div>
      </header>

      {status && <div className="doc-status">{status}</div>}

      {loading ? (
        <div className="doc-loading">{t("editor.loading")}</div>
      ) : (
        <div className={`doc-body ${mode}`}>
          {mode !== "preview" && <div className="doc-pane">{editor}</div>}
          {mode !== "edit" && <div className="doc-pane">{preview}</div>}
        </div>
      )}
    </main>
  );
}
