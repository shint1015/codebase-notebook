import { useCallback, useEffect, useState } from "react";
import { api } from "../infrastructure/api";
import type { IndexReport, Workspace } from "../domain/types";

export function useWorkspaces() {
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [indexing, setIndexing] = useState(false);
  const [lastReport, setLastReport] = useState<IndexReport | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const list = await api.listWorkspaces();
      setWorkspaces(list);
      setSelectedId((current) =>
        current && list.some((w) => w.id === current)
          ? current
          : (list[0]?.id ?? null),
      );
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const create = useCallback(
    async (name: string, rootPath: string) => {
      const workspace = await api.createWorkspace(name, rootPath);
      await refresh();
      setSelectedId(workspace.id);
      return workspace;
    },
    [refresh],
  );

  const remove = useCallback(
    async (id: string) => {
      await api.deleteWorkspace(id);
      setLastReport(null);
      await refresh();
    },
    [refresh],
  );

  const index = useCallback(async (id: string) => {
    setIndexing(true);
    setError(null);
    try {
      const report = await api.indexWorkspace(id);
      setLastReport(report);
      return report;
    } finally {
      setIndexing(false);
    }
  }, []);

  const setAllowExternal = useCallback(
    async (id: string, allow: boolean) => {
      await api.setWorkspaceAllowExternal(id, allow);
      await refresh();
    },
    [refresh],
  );

  const selected = workspaces.find((w) => w.id === selectedId) ?? null;

  return {
    workspaces,
    selected,
    selectedId,
    setSelectedId,
    create,
    remove,
    index,
    indexing,
    lastReport,
    setAllowExternal,
    error,
    refresh,
  };
}
