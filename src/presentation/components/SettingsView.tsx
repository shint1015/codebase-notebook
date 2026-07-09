import { useEffect, useState } from "react";
import type { ProviderConfig, ProviderKind } from "../../domain/types";
import { EXTERNAL_PROVIDERS, PROVIDER_LABELS } from "../../domain/types";
import { isCommandError } from "../../domain/types";
import { api } from "../../infrastructure/api";

interface Props {
  providers: ProviderConfig[];
  onConfigure: (
    config: Omit<ProviderConfig, "has_api_key">,
    apiKey: string | null,
  ) => Promise<unknown>;
  onTest: (kind: ProviderKind) => Promise<string>;
  onClose: () => void;
}

export function SettingsView({ providers, onConfigure, onTest, onClose }: Props) {
  return (
    <div className="modal-backdrop">
      <div className="modal settings">
        <div className="settings-header">
          <h2>AI Providers</h2>
          <button onClick={onClose}>✕</button>
        </div>
        <p className="settings-note">
          Local (Ollama) is the default and needs no key. External providers are
          BYOK — keys are stored in the OS keychain, never in the database, and
          every external request shows you what would be sent first.
        </p>
        {providers.map((p) => (
          <ProviderCard key={p.kind} config={p} onConfigure={onConfigure} onTest={onTest} />
        ))}
        <SearchSettingsCard />
        <UsageCard />
      </div>
    </div>
  );
}

function UsageCard() {
  const [summary, setSummary] = useState<
    import("../../domain/types").ProviderUsageSummary[]
  >([]);
  const [records, setRecords] = useState<
    import("../../domain/types").UsageRecord[]
  >([]);

  useEffect(() => {
    void api.usageSummary().then(setSummary);
    void api.listUsage(20).then(setRecords);
  }, []);

  return (
    <div className="provider-card">
      <div className="provider-title">
        <strong>Usage & audit</strong>
      </div>
      {summary.length > 0 && (
        <div className="usage-summary">
          {summary.map((s) => (
            <span key={s.provider} className="usage-chip">
              {s.provider}: ${s.month_total_usd.toFixed(2)}
              {s.monthly_budget_usd !== null &&
                ` / $${s.monthly_budget_usd.toFixed(0)}`}{" "}
              this month
            </span>
          ))}
        </div>
      )}
      <ul className="usage-list">
        {records.map((r) => (
          <li key={r.id}>
            <span className="usage-when">
              {new Date(r.created_at).toLocaleString()}
            </span>
            <span>
              {r.provider} · {r.model}
            </span>
            <span title={r.sources.join("\n")}>
              {r.sources.length} sources
            </span>
            <span>${r.est_cost_usd.toFixed(4)}</span>
          </li>
        ))}
        {records.length === 0 && (
          <li className="empty">No model calls recorded yet.</li>
        )}
      </ul>
      <p className="settings-note">
        Every model call is logged locally with the exact source files that
        were included in the prompt. Costs are rough estimates.
      </p>
    </div>
  );
}

function SearchSettingsCard() {
  const [embeddingModel, setEmbeddingModel] = useState("");
  const [rerankEnabled, setRerankEnabled] = useState(false);
  const [status, setStatus] = useState<string | null>(null);

  useEffect(() => {
    void api.getSearchSettings().then((s) => {
      setEmbeddingModel(s.embedding_model);
      setRerankEnabled(s.rerank_enabled);
    });
  }, []);

  const save = async () => {
    setStatus(null);
    try {
      await api.setSearchSettings(embeddingModel, rerankEnabled);
      setStatus("Saved. Re-index workspaces after changing the embedding model.");
    } catch (e) {
      setStatus(isCommandError(e) ? e.message : String(e));
    }
  };

  return (
    <div className="provider-card">
      <div className="provider-title">
        <strong>Search</strong>
        <span className="badge local">local</span>
      </div>
      <div className="provider-grid">
        <label>
          Embedding model (Ollama)
          <input
            value={embeddingModel}
            onChange={(e) => setEmbeddingModel(e.target.value)}
            placeholder="nomic-embed-text / bge-m3"
          />
        </label>
        <label className="checkbox-row">
          <input
            type="checkbox"
            checked={rerankEnabled}
            onChange={(e) => setRerankEnabled(e.target.checked)}
          />
          Rerank results with the local LLM (slower, higher quality)
        </label>
      </div>
      <div className="provider-actions">
        <button className="primary" onClick={() => void save()}>
          Save
        </button>
        {status && <span className="provider-status">{status}</span>}
      </div>
    </div>
  );
}

function ProviderCard({
  config,
  onConfigure,
  onTest,
}: {
  config: ProviderConfig;
  onConfigure: Props["onConfigure"];
  onTest: Props["onTest"];
}) {
  const [enabled, setEnabled] = useState(config.enabled);
  const [baseUrl, setBaseUrl] = useState(config.base_url);
  const [model, setModel] = useState(config.default_model);
  const [allowSendCode, setAllowSendCode] = useState(config.allow_send_code);
  const [budget, setBudget] = useState(
    config.monthly_budget_usd === null ? "" : String(config.monthly_budget_usd),
  );
  const [apiKey, setApiKey] = useState("");
  const [status, setStatus] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  const isExternal = EXTERNAL_PROVIDERS.includes(config.kind);

  const save = async () => {
    setSaving(true);
    setStatus(null);
    try {
      const parsedBudget = budget.trim() === "" ? null : Number(budget);
      await onConfigure(
        {
          kind: config.kind,
          enabled,
          base_url: baseUrl,
          default_model: model,
          allow_send_code: allowSendCode,
          monthly_budget_usd:
            parsedBudget !== null && Number.isFinite(parsedBudget) && parsedBudget > 0
              ? parsedBudget
              : null,
        },
        apiKey.trim() ? apiKey.trim() : null,
      );
      setApiKey("");
      setStatus("Saved.");
    } catch (e) {
      setStatus(isCommandError(e) ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  };

  const test = async () => {
    setStatus("Testing…");
    try {
      setStatus(await onTest(config.kind));
    } catch (e) {
      setStatus(isCommandError(e) ? e.message : String(e));
    }
  };

  return (
    <div className="provider-card">
      <div className="provider-title">
        <label>
          <input
            type="checkbox"
            checked={enabled}
            onChange={(e) => setEnabled(e.target.checked)}
          />
          <strong>{PROVIDER_LABELS[config.kind]}</strong>
        </label>
        {isExternal ? (
          <span className="badge external">external ↗</span>
        ) : (
          <span className="badge local">local</span>
        )}
      </div>
      <div className="provider-grid">
        <label>
          Base URL
          <input value={baseUrl} onChange={(e) => setBaseUrl(e.target.value)} />
        </label>
        <label>
          Default model
          <input
            value={model}
            onChange={(e) => setModel(e.target.value)}
            placeholder="e.g. qwen2.5-coder:14b"
          />
        </label>
        {isExternal && (
          <>
            <label>
              API key {config.has_api_key && <em>(stored in keychain)</em>}
              <input
                type="password"
                value={apiKey}
                onChange={(e) => setApiKey(e.target.value)}
                placeholder={config.has_api_key ? "•••••• (leave blank to keep)" : "paste key"}
              />
            </label>
            <label>
              Monthly budget (USD)
              <input
                type="number"
                min="0"
                step="1"
                value={budget}
                onChange={(e) => setBudget(e.target.value)}
                placeholder="no limit"
              />
            </label>
            <label className="checkbox-row">
              <input
                type="checkbox"
                checked={allowSendCode}
                onChange={(e) => setAllowSendCode(e.target.checked)}
              />
              Allow sending code snippets to this provider
            </label>
          </>
        )}
      </div>
      <div className="provider-actions">
        <button className="primary" onClick={() => void save()} disabled={saving}>
          Save
        </button>
        <button onClick={() => void test()}>Test connection</button>
        {status && <span className="provider-status">{status}</span>}
      </div>
    </div>
  );
}
