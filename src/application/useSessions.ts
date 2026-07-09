import { useCallback, useEffect, useState } from "react";
import { api } from "../infrastructure/api";
import type { ChatSession } from "../domain/types";

export function useSessions(workspaceId: string | null) {
  const [sessions, setSessions] = useState<ChatSession[]>([]);

  const refresh = useCallback(async () => {
    if (!workspaceId) {
      setSessions([]);
      return;
    }
    setSessions(await api.listChatSessions(workspaceId));
  }, [workspaceId]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  return { sessions, refresh };
}
