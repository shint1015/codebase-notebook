import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { LANGUAGES, setLanguage, type Language } from "../../infrastructure/i18n";
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
  const { t, i18n } = useTranslation();
  return (
    <div className="modal-backdrop">
      <div className="modal settings">
        <div className="settings-header">
          <h2>{t("settings.title")}</h2>
          <button onClick={onClose}>✕</button>
        </div>
        <p className="settings-note">{t("settings.note")}</p>

        <div className="provider-card">
          <div className="provider-title">
            <strong>{t("settings.language")}</strong>
          </div>
          <div className="provider-grid">
            <label>
              {t("settings.language")}
              <select
                value={i18n.language.startsWith("ja") ? "ja" : "en"}
                onChange={(e) => setLanguage(e.target.value as Language)}
              >
                {Object.entries(LANGUAGES).map(([code, label]) => (
                  <option key={code} value={code}>
                    {label}
                  </option>
                ))}
              </select>
            </label>
          </div>
        </div>
        {providers.map((p) => (
          <ProviderCard key={p.kind} config={p} onConfigure={onConfigure} onTest={onTest} />
        ))}
        <SearchSettingsCard />
        <ConnectorsCard />
        <UsageCard />
      </div>
    </div>
  );
}

const CONNECTOR_LABELS: Record<string, string> = {
  slack: "Slack (bot token xoxb-…)",
  notion: "Notion (integration token)",
  asana: "Asana (personal access token)",
  backlog: "Backlog (apiKey — see below)",
  confluence: "Confluence (email:api_token or token)",
};

function ConnectorsCard() {
  const { t } = useTranslation();
  const [connectors, setConnectors] = useState<
    { name: string; connected: boolean }[]
  >([]);
  const [tokens, setTokens] = useState<Record<string, string>>({});
  const [status, setStatus] = useState<string | null>(null);

  const refresh = () => void api.listConnectors().then(setConnectors);
  useEffect(refresh, []);

  const save = async (name: string) => {
    setStatus(null);
    try {
      await api.setConnectorToken(name, tokens[name] ?? "");
      setTokens((prev) => ({ ...prev, [name]: "" }));
      refresh();
      setStatus(`${name} saved.`);
    } catch (e) {
      setStatus(isCommandError(e) ? e.message : String(e));
    }
  };

  return (
    <div className="provider-card">
      <div className="provider-title">
        <strong>{t("settings.connectors")}</strong>
        <span className="badge external">agent tools ↗</span>
      </div>
      <p className="settings-note">{t("settings.connectorsNote")}</p>
      {connectors.map((c) => (
        <div className="connector-row" key={c.name}>
          <label>
            {CONNECTOR_LABELS[c.name] ?? c.name}
            {c.connected && <em> {t("settings.connected")}</em>}
          </label>
          <div className="connector-input">
            <input
              type="password"
              value={tokens[c.name] ?? ""}
              placeholder={c.connected ? t("settings.keyKeepPlaceholder") : t("settings.keyPlaceholder")}
              onChange={(e) =>
                setTokens((prev) => ({ ...prev, [c.name]: e.target.value }))
              }
            />
            <button onClick={() => void save(c.name)}>
              {c.connected && !(tokens[c.name] ?? "").trim() ? t("settings.disconnect") : t("settings.save")}
            </button>
          </div>
        </div>
      ))}
      {status && <span className="provider-status">{status}</span>}
    </div>
  );
}

function UsageCard() {
  const { t } = useTranslation();
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
        <strong>{t("settings.usage")}</strong>
      </div>
      {summary.length > 0 && (
        <div className="usage-summary">
          {summary.map((s) => (
            <span key={s.provider} className="usage-chip">
              {s.provider}: ${s.month_total_usd.toFixed(2)}
              {s.monthly_budget_usd !== null &&
                ` / $${s.monthly_budget_usd.toFixed(0)}`}{" "}
              {t("settings.thisMonth")}
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
              {t("settings.sources", { count: r.sources.length })}
            </span>
            <span>${r.est_cost_usd.toFixed(4)}</span>
          </li>
        ))}
        {records.length === 0 && (
          <li className="empty">{t("settings.noUsage")}</li>
        )}
      </ul>
      <p className="settings-note">{t("settings.usageNote")}</p>
    </div>
  );
}

function SearchSettingsCard() {
  const { t } = useTranslation();
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
      setStatus(t("settings.searchSaved"));
    } catch (e) {
      setStatus(isCommandError(e) ? e.message : String(e));
    }
  };

  return (
    <div className="provider-card">
      <div className="provider-title">
        <strong>{t("settings.search")}</strong>
        <span className="badge local">{t("settings.local")}</span>
      </div>
      <div className="provider-grid">
        <label>
          {t("settings.embeddingModel")}
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
          {t("settings.rerank")}
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

  const { t } = useTranslation();
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
      setStatus(t("settings.saved"));
    } catch (e) {
      setStatus(isCommandError(e) ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  };

  const test = async () => {
    setStatus(t("settings.testing"));
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
          <span className="badge external">{t("settings.external")}</span>
        ) : (
          <span className="badge local">{t("settings.local")}</span>
        )}
      </div>
      <div className="provider-grid">
        <label>
          {t("settings.baseUrl")}
          <input value={baseUrl} onChange={(e) => setBaseUrl(e.target.value)} />
        </label>
        <label>
          {t("settings.defaultModel")}
          <input
            value={model}
            onChange={(e) => setModel(e.target.value)}
            placeholder="qwen2.5-coder:14b"
          />
        </label>
        {isExternal && (
          <>
            <label>
              {t("settings.apiKey")} {config.has_api_key && <em>{t("settings.storedInKeychain")}</em>}
              <input
                type="password"
                value={apiKey}
                onChange={(e) => setApiKey(e.target.value)}
                placeholder={config.has_api_key ? t("settings.keyKeepPlaceholder") : t("settings.keyPlaceholder")}
              />
            </label>
            <label>
              {t("settings.monthlyBudget")}
              <input
                type="number"
                min="0"
                step="1"
                value={budget}
                onChange={(e) => setBudget(e.target.value)}
                placeholder={t("settings.noLimit")}
              />
            </label>
            <label className="checkbox-row">
              <input
                type="checkbox"
                checked={allowSendCode}
                onChange={(e) => setAllowSendCode(e.target.checked)}
              />
              {t("settings.allowSendCode")}
            </label>
          </>
        )}
      </div>
      <div className="provider-actions">
        <button className="primary" onClick={() => void save()} disabled={saving}>
          {t("settings.save")}
        </button>
        <button onClick={() => void test()}>{t("settings.testConnection")}</button>
        {status && <span className="provider-status">{status}</span>}
      </div>
    </div>
  );
}
