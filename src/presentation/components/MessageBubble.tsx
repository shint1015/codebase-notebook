import { useState } from "react";
import { useTranslation } from "react-i18next";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import type { Message, VerificationReport } from "../../domain/types";
import { groundingVerdict } from "../../domain/types";
import { api } from "../../infrastructure/api";

export function MessageBubble({
  message,
  workspaceId,
  onFork,
  onOpenSource,
}: {
  message: Message;
  workspaceId: string;
  /** Fork the conversation up to and including this message. */
  onFork: (messageId: string) => void;
  /** Open a cited file in the in-app code editor. */
  onOpenSource: (relPath: string, line: number) => void;
}) {
  const [openCitation, setOpenCitation] = useState<number | null>(null);
  const [revealError, setRevealError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [verifying, setVerifying] = useState(false);
  const [verification, setVerification] = useState<VerificationReport | null>(
    null,
  );
  const { t } = useTranslation();

  const verify = async () => {
    setVerifying(true);
    setVerification(null);
    try {
      setVerification(await api.verifyAnswer(message.session_id, message.id));
    } catch (e) {
      setRevealError(String(e));
    } finally {
      setVerifying(false);
    }
  };

  const copy = async () => {
    await navigator.clipboard.writeText(message.content);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };

  return (
    <div className={`message ${message.role}`}>
      <div className="message-actions">
        <button title={copied ? t("chat.copied") : t("chat.copyMessage")} onClick={() => void copy()}>
          {copied ? "✓" : "⧉"}
        </button>
        <button title={t("chat.forkHere")} onClick={() => onFork(message.id)}>
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
          {message.grounding && (
            <span
              className={`grounding-badge ${groundingVerdict(message.grounding)}`}
              title={[
                t("grounding.claims", {
                  cited: message.grounding.cited_claims,
                  total: message.grounding.total_claims,
                }),
                message.grounding.invalid_markers.length > 0
                  ? t("grounding.invalidMarkers", {
                      markers: message.grounding.invalid_markers.join(", "),
                    })
                  : "",
              ]
                .filter(Boolean)
                .join(" · ")}
            >
              {t(`grounding.${groundingVerdict(message.grounding)}`)}
            </span>
          )}
          {message.citations.length > 0 && (
            <button
              className="verify-button"
              title={t("grounding.verifyHint")}
              disabled={verifying}
              onClick={() => void verify()}
            >
              {verifying ? t("grounding.verifying") : t("grounding.verify")}
            </button>
          )}
          {verification && (
            <div
              className={`verification-result ${verification.supported ? "ok" : "warn"}`}
            >
              {verification.supported
                ? t("grounding.verifySupported", { model: verification.model })
                : t("grounding.verifyUnsupported", { model: verification.model })}
              {verification.issues.length > 0 && (
                <ul>
                  {verification.issues.map((issue, i) => (
                    <li key={i}>{issue}</li>
                  ))}
                </ul>
              )}
            </div>
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
                    <span className="citation-open-group">
                      <button
                        className="citation-open"
                        title={t("citation.openInApp")}
                        onClick={() => onOpenSource(c.rel_path, c.start_line)}
                      >
                        {t("citation.open")}
                      </button>
                      <button
                        className="citation-open"
                        title={t("citation.openExternal")}
                        onClick={() => {
                          setRevealError(null);
                          api
                            .revealSource(workspaceId, c.rel_path, c.start_line)
                            .catch((e) => setRevealError(String(e)));
                        }}
                      >
                        ↗
                      </button>
                    </span>
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
