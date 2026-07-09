import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
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
    // Keep the filesystem watcher aligned with the current source set.
    void api.rebuildWatchers().catch(() => {});
  }, [workspaceId]);

  useEffect(() => {
    setLastReport(null);
    setError(null);
    void refresh();
  }, [refresh]);

  // Background re-index (file watcher) results update the report banner.
  useEffect(() => {
    if (!workspaceId) return;
    const unlisten = listen<{ workspaceId: string; report: IndexReport }>(
      "workspace-indexed",
      (event) => {
        if (event.payload.workspaceId === workspaceId) {
          setLastReport(event.payload.report);
        }
      },
    );
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [workspaceId]);

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

  const addGithubIssues = useCallback(
    (spec: string) =>
      wrap(async () => {
        if (!workspaceId) return;
        setCloning(true);
        try {
          await api.addGithubIssuesRepository(workspaceId, spec);
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

  /// Refresh a managed source from its remote, then re-index.
  const sync = useCallback(
    (repositoryId: string) =>
      wrap(async () => {
        if (!workspaceId) return;
        setCloning(true);
        try {
          await api.syncRepository(repositoryId);
        } finally {
          setCloning(false);
        }
        setIndexing(true);
        try {
          setLastReport(await api.indexWorkspace(workspaceId));
        } finally {
          setIndexing(false);
        }
      }),
    [workspaceId, wrap],
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
    addGithubIssues,
    remove,
    sync,
    index,
    cloning,
    indexing,
    lastReport,
    error,
  };
}
