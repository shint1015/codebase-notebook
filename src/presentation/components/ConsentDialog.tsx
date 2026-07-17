import { useTranslation } from "react-i18next";
import type { PendingConsent } from "../../application/useChat";
import { PROVIDER_LABELS } from "../../domain/types";

interface Props {
  pending: PendingConsent;
  onApprove: () => void;
  onLocalOnly: () => void;
  onCancel: () => void;
}

/**
 * Transparency gate: shows exactly which sources would leave the machine
 * before an external provider is called. Nothing is sent until the user
 * explicitly approves.
 */
export function ConsentDialog({ pending, onApprove, onLocalOnly, onCancel }: Props) {
  const { preparation, provider } = pending;
  const { t } = useTranslation();
  return (
    <div className="modal-backdrop">
      <div className="modal">
        <h2>{t("consent.title", { provider: PROVIDER_LABELS[provider] })}</h2>
        <p>{t("consent.body", { model: preparation.model })}</p>
        <ul className="source-list">
          {preparation.sources.length === 0 && <li>{t("consent.noSources")}</li>}
          {preparation.sources.map((s, i) => (
            <li key={i}>
              <code>{s.rel_path}</code>{" "}
              {t("citation.lines", { from: s.start_line, to: s.end_line })}
            </li>
          ))}
        </ul>
        <div className="modal-actions">
          <button className="primary" onClick={onApprove}>
            {t("consent.allowOnce")}
          </button>
          <button onClick={onLocalOnly}>{t("consent.useLocal")}</button>
          <button onClick={onCancel}>{t("consent.cancel")}</button>
        </div>
      </div>
    </div>
  );
}
