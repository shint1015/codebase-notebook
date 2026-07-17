import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type { OllamaStatus } from "../../domain/types";
import { api } from "../../infrastructure/api";

/**
 * Onboarding: when Ollama is missing or its models aren't pulled yet, show
 * what to do — and pull models directly from the app with live progress.
 */
export function OllamaBanner() {
  const [status, setStatus] = useState<OllamaStatus | null>(null);
  const [progress, setProgress] = useState<string | null>(null);
  const [pulling, setPulling] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const { t } = useTranslation();

  const refresh = useCallback(async () => {
    try {
      setStatus(await api.ollamaStatus());
    } catch {
      setStatus(null);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  if (!status) return null;
  const allGood =
    status.reachable && status.chat_model_present && status.embedding_model_present;
  if (allGood) return null;

  const pull = async (model: string) => {
    setPulling(true);
    setError(null);
    try {
      await api.pullOllamaModel(model, setProgress);
      setProgress(null);
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setPulling(false);
    }
  };

  return (
    <div className="ollama-banner">
      {!status.reachable ? (
        <>
          <strong>{t("ollama.notRunning")}</strong>{" "}
          <span>{t("ollama.installHint")}</span>
        </>
      ) : (
        <>
          <strong>{t("ollama.modelsMissing")}</strong>
          <div className="ollama-actions">
            {!status.chat_model_present && (
              <button disabled={pulling} onClick={() => void pull(status.chat_model)}>
                {t("ollama.pullChat", { model: status.chat_model })}
              </button>
            )}
            {!status.embedding_model_present && (
              <button
                disabled={pulling}
                onClick={() => void pull(status.embedding_model)}
              >
                {t("ollama.pullEmbedding", { model: status.embedding_model })}
              </button>
            )}
          </div>
          {progress && <div className="ollama-progress">{progress}</div>}
          {error && <div className="error">{error}</div>}
        </>
      )}
    </div>
  );
}
