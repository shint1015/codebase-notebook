import { useCallback, useEffect, useState } from "react";
import { api } from "../infrastructure/api";
import type {
  AskPreparation,
  ChatSession,
  Message,
  ProviderKind,
} from "../domain/types";
import { isCommandError } from "../domain/types";

/** A question waiting for the user's external-send approval. */
export interface PendingConsent {
  question: string;
  provider: ProviderKind;
  preparation: AskPreparation;
}

export function useChat(workspaceId: string | null) {
  const [sessions, setSessions] = useState<ChatSession[]>([]);
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [messages, setMessages] = useState<Message[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [pendingConsent, setPendingConsent] = useState<PendingConsent | null>(null);

  useEffect(() => {
    setSessions([]);
    setSessionId(null);
    setMessages([]);
    setPendingConsent(null);
    if (!workspaceId) return;
    void api.listChatSessions(workspaceId).then((list) => {
      setSessions(list);
      setSessionId(list[0]?.id ?? null);
    });
  }, [workspaceId]);

  useEffect(() => {
    if (!sessionId) {
      setMessages([]);
      return;
    }
    void api.listChatMessages(sessionId).then(setMessages);
  }, [sessionId]);

  const selectSession = useCallback((id: string) => {
    setSessionId(id);
    setPendingConsent(null);
  }, []);

  const ensureSession = useCallback(
    async (firstQuestion: string): Promise<string> => {
      if (sessionId) return sessionId;
      if (!workspaceId) throw new Error("no workspace selected");
      const session = await api.createChatSession(
        workspaceId,
        firstQuestion.slice(0, 60),
      );
      setSessions((prev) => [session, ...prev]);
      setSessionId(session.id);
      return session.id;
    },
    [sessionId, workspaceId],
  );

  const runAsk = useCallback(
    async (question: string, provider: ProviderKind, consentGranted: boolean) => {
      if (!workspaceId) return;
      setBusy(true);
      setError(null);
      try {
        const sid = await ensureSession(question);
        await api.ask(sid, workspaceId, question, provider, consentGranted);
        setMessages(await api.listChatMessages(sid));
      } catch (e) {
        if (isCommandError(e) && e.code === "consent_required") {
          // Backstop — the prepare step should normally catch this first.
          const preparation = await api.prepareAsk(workspaceId, question, provider);
          setPendingConsent({ question, provider, preparation });
        } else {
          setError(isCommandError(e) ? e.message : String(e));
        }
      } finally {
        setBusy(false);
      }
    },
    [workspaceId, ensureSession],
  );

  /** Entry point from the input box: checks consent before anything is sent. */
  const send = useCallback(
    async (question: string, provider: ProviderKind) => {
      if (!workspaceId) return;
      setError(null);
      const trimmed = question.trim();
      if (!trimmed) return;
      try {
        const preparation = await api.prepareAsk(workspaceId, trimmed, provider);
        if (preparation.requires_consent) {
          setPendingConsent({ question: trimmed, provider, preparation });
          return;
        }
        await runAsk(trimmed, provider, false);
      } catch (e) {
        setError(isCommandError(e) ? e.message : String(e));
      }
    },
    [workspaceId, runAsk],
  );

  const approveConsent = useCallback(async () => {
    if (!pendingConsent) return;
    const { question, provider } = pendingConsent;
    setPendingConsent(null);
    await runAsk(question, provider, true);
  }, [pendingConsent, runAsk]);

  const declineConsent = useCallback(
    async (fallbackToLocal: boolean) => {
      if (!pendingConsent) return;
      const { question } = pendingConsent;
      setPendingConsent(null);
      if (fallbackToLocal) {
        await runAsk(question, "ollama", false);
      }
    },
    [pendingConsent, runAsk],
  );

  return {
    sessions,
    sessionId,
    selectSession,
    startNewSession: () => {
      setSessionId(null);
      setMessages([]);
    },
    messages,
    busy,
    error,
    send,
    pendingConsent,
    approveConsent,
    declineConsent,
  };
}
