import { useEffect, useState } from "react";
import type { Workspace } from "../../domain/types";
import { isCommandError } from "../../domain/types";
import { api } from "../../infrastructure/api";
import { CodeEditor } from "./CodeEditor";

interface Props {
  workspace: Workspace;
  /** Citation-style path: "<repo>/<path in repo>". */
  relPath: string;
  /** Line to jump to on open (from a citation). */
  line?: number;
  onClose: () => void;
}

/** View and edit an indexed source file in-app, with syntax highlighting. */
export function SourceEditor({ workspace, relPath, line, onClose }: Props) {
  const [content, setContent] = useState("");
  const [loading, setLoading] = useState(true);
  const [dirty, setDirty] = useState(false);
  const [status, setStatus] = useState<string | null>(null);

  useEffect(() => {
    setLoading(true);
    setStatus(null);
    api
      .readSourceFile(workspace.id, relPath)
      .then((text) => {
        setContent(text);
        setDirty(false);
      })
      .catch((e) => setStatus(isCommandError(e) ? e.message : String(e)))
      .finally(() => setLoading(false));
  }, [workspace.id, relPath]);

  const save = async () => {
    setStatus("Saving…");
    try {
      await api.writeSourceFile(workspace.id, relPath, content);
      setDirty(false);
      // The file watcher re-indexes local sources automatically.
      setStatus("Saved");
      setTimeout(() => setStatus(null), 1500);
    } catch (e) {
      setStatus(isCommandError(e) ? e.message : String(e));
    }
  };

  const fileName = relPath.split("/").pop() ?? relPath;

  return (
    <main className="doc-editor">
      <header className="chat-header">
        <div className="chat-title">
          <button onClick={onClose} title="Back">
            ←
          </button>
          <div>
            <h2>
              {fileName}
              {dirty && <span className="doc-dirty"> ●</span>}
            </h2>
            <span className="workspace-path" title={relPath}>
              {relPath}
              {line ? `:${line}` : ""}
            </span>
          </div>
        </div>
        <div className="doc-actions">
          <button
            title="Open in your external editor"
            onClick={() =>
              api.revealSource(workspace.id, relPath, line ?? 1).catch((e) => setStatus(String(e)))
            }
          >
            ↗ Open externally
          </button>
          <button className="primary" onClick={() => void save()} disabled={!dirty}>
            Save
          </button>
        </div>
      </header>

      {status && <div className="doc-status">{status}</div>}

      {loading ? (
        <div className="doc-loading">Loading…</div>
      ) : (
        <div className="doc-body edit">
          <div className="doc-pane">
            <CodeEditor
              value={content}
              fileName={fileName}
              jumpToLine={line}
              onChange={(next) => {
                setContent(next);
                setDirty(true);
              }}
              onSave={() => void save()}
            />
          </div>
        </div>
      )}
    </main>
  );
}
