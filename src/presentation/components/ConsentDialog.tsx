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
  return (
    <div className="modal-backdrop">
      <div className="modal">
        <h2>Send sources to {PROVIDER_LABELS[provider]}?</h2>
        <p>
          This question would send the following parts of your workspace to an
          <strong> external AI API</strong> ({preparation.model}):
        </p>
        <ul className="source-list">
          {preparation.sources.length === 0 && <li>(no source chunks — question text only)</li>}
          {preparation.sources.map((s, i) => (
            <li key={i}>
              <code>{s.rel_path}</code> lines {s.start_line}–{s.end_line}
            </li>
          ))}
        </ul>
        <div className="modal-actions">
          <button className="primary" onClick={onApprove}>
            Allow this time
          </button>
          <button onClick={onLocalOnly}>Use local model instead</button>
          <button onClick={onCancel}>Cancel</button>
        </div>
      </div>
    </div>
  );
}
