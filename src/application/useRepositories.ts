import { useCallback, useEffect, useState } from "react";
import { api } from "../infrastructure/api";
import type { IndexReport, Repository } from "../domain/types";
import { isCommandError } from "../domain/types";

export function useRepositories(workspaceId: string | null) {
  const [repositories, setRepositories] = useState<Repository[]>([]);
  const [cloning, setCloning] = useState(false);
  const [indexing, setIndexing] = useState(false);
  const [lastReport, setLastReport] = useState<IndexReport | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    if (!workspaceId) {
      setRepositories([]);
      return;
    }
    setRepositories(await api.listRepositories(workspaceId));
  }, [workspaceId]);

  useEffect(() => {
    setLastReport(null);
    setError(null);
    void refresh();
  }, [refresh]);

  const wrap = useCallback(
    async (action: () => Promise<void>) => {
      setError(null);
      try {
        await action();
      } catch (e) {
        setError(isCommandError(e) ? e.message : String(e));
      }
    },
    [],
  );

  const addLocal = useCallback(
    (rootPath: string) =>
      wrap(async () => {
        if (!workspaceId) return;
        await api.addLocalRepository(workspaceId, rootPath);
        await refresh();
      }),
    [workspaceId, refresh, wrap],
  );

  const addGit = useCallback(
    (url: string) =>
      wrap(async () => {
        if (!workspaceId) return;
        setCloning(true);
        try {
          await api.addGitRepository(workspaceId, url);
          await refresh();
        } finally {
          setCloning(false);
        }
      }),
    [workspaceId, refresh, wrap],
  );

  const remove = useCallback(
    (repositoryId: string) =>
      wrap(async () => {
        await api.deleteRepository(repositoryId);
        await refresh();
      }),
    [refresh, wrap],
  );

  const index = useCallback(
    () =>
      wrap(async () => {
        if (!workspaceId) return;
        setIndexing(true);
        try {
          setLastReport(await api.indexWorkspace(workspaceId));
        } finally {
          setIndexing(false);
        }
      }),
    [workspaceId, wrap],
  );

  return {
    repositories,
    addLocal,
    addGit,
    remove,
    index,
    cloning,
    indexing,
    lastReport,
    error,
  };
}
