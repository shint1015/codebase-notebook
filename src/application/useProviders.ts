import { useCallback, useEffect, useState } from "react";
import { api } from "../infrastructure/api";
import type { ProviderConfig, ProviderKind } from "../domain/types";

export function useProviders() {
  const [providers, setProviders] = useState<ProviderConfig[]>([]);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      setProviders(await api.listProviders());
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const configure = useCallback(
    async (config: Omit<ProviderConfig, "has_api_key">, apiKey: string | null) => {
      const saved = await api.configureProvider(config, apiKey);
      await refresh();
      return saved;
    },
    [refresh],
  );

  const test = useCallback(
    (kind: ProviderKind) => api.testProvider(kind),
    [],
  );

  const enabledProviders = providers.filter((p) => p.enabled);

  return { providers, enabledProviders, configure, test, error, refresh };
}
