import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import CodeMirror from "@uiw/react-codemirror";
import { oneDark } from "@codemirror/theme-one-dark";
import { EditorView } from "@codemirror/view";
import { unifiedMergeView } from "@codemirror/merge";
import type { Citation } from "../../domain/types";
import { api } from "../../infrastructure/api";
import { mergeSnippet } from "../../application/patch";
import { languageFor } from "./CodeEditor";

/**
 * Review-and-apply dialog for a code block proposed in chat: pick the target
 * file, see a unified diff of the automatic merge, adjust it, then write.
 */
export function ApplyPatchDialog({
  workspaceId,
  snippet,
  suggestedPath,
  citations,
  onClose,
  onApplied,
}: {
  workspaceId: string;
  snippet: string;
  /** From the fence's `path=` annotation, if the model provided one. */
  suggestedPath: string | null;
  citations: Citation[];
  onClose: () => void;
  onApplied: (relPath: string) => void;
}) {
  const { t } = useTranslation();
  const [paths, setPaths] = useState<string[]>([]);
  const [path, setPath] = useState(
    suggestedPath ?? citations[0]?.rel_path ?? "",
  );
  const [original, setOriginal] = useState<string | null>(null);
  const [updated, setUpdated] = useState("");
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    api
      .listSourcePaths(workspaceId)
      .then(setPaths)
      .catch((e) => setError(String(e)));
  }, [workspaceId]);

  // Load the target file and run the automatic merge whenever it changes.
  useEffect(() => {
    if (!path) {
      setOriginal(null);
      return;
    }
    let stale = false;
    setError(null);
    setNotice(null);
    api
      .readSourceFile(workspaceId, path)
      .then((content) => {
        if (stale) return;
        setOriginal(content);
        const citation = citations.find((c) => c.rel_path === path);
        const outcome = mergeSnippet(content, snippet, citation);
        if (outcome.kind === "already") {
          setUpdated(content);
          setNotice(t("apply.alreadyApplied"));
        } else if (outcome.kind === "merged") {
          setUpdated(outcome.updated);
        } else {
          setUpdated(content);
          setNotice(t("apply.manualMerge"));
        }
      })
      .catch((e) => {
        if (stale) return;
        setOriginal(null);
        setError(String(e));
      });
    return () => {
      stale = true;
    };
  }, [workspaceId, path, snippet, citations, t]);

  const extensions = useMemo(
    () =>
      original === null
        ? []
        : [
            ...languageFor(path),
            EditorView.lineWrapping,
            unifiedMergeView({ original, mergeControls: false }),
          ],
    [original, path],
  );

  const apply = async () => {
    setSaving(true);
    setError(null);
    try {
      await api.writeSourceFile(workspaceId, path, updated);
      onApplied(path);
    } catch (e) {
      setError(String(e));
      setSaving(false);
    }
  };

  return (
    <div className="modal-backdrop">
      <div className="modal apply-patch-modal">
        <h2>{t("apply.title")}</h2>
        <label className="apply-target">
          {t("apply.targetFile")}
          <input
            list="apply-source-paths"
            value={path}
            onChange={(e) => setPath(e.target.value)}
            placeholder={t("apply.targetPlaceholder")}
          />
          <datalist id="apply-source-paths">
            {paths.map((p) => (
              <option key={p} value={p} />
            ))}
          </datalist>
        </label>
        {notice && <div className="apply-notice">{notice}</div>}
        {error && <div className="error">{error}</div>}
        {original !== null && (
          <div className="apply-diff">
            <CodeMirror
              value={updated}
              height="100%"
              theme={oneDark}
              extensions={extensions}
              basicSetup={{ lineNumbers: true }}
              onChange={setUpdated}
            />
          </div>
        )}
        <div className="modal-actions">
          <button onClick={onClose}>{t("apply.cancel")}</button>
          <button
            className="primary"
            disabled={saving || original === null || updated === original}
            onClick={() => void apply()}
          >
            {saving ? t("apply.applying") : t("apply.apply")}
          </button>
        </div>
      </div>
    </div>
  );
}
