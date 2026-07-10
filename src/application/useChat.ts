import { useCallback, useEffect, useState } from "react";
import { api } from "../infrastructure/api";
import type {
  AskPreparation,
  ChatSession,
  Message,
  ProviderKind,
  ToolEvent,
} from "../domain/types";
import { isCommandError } from "../domain/types";

/** A question waiting for the user's external-send approval. */
export interface PendingConsent {
  question: string;
  provider: ProviderKind;
  preparation: AskPreparation;
}

/**
 * Chat for one (workspace, session). `sessionId === null` means a new chat:
 * the session is created on the first question and reported upward via
 * `onSessionCreated` so navigation state stays in the parent.
 */
export function useChat(
  workspaceId: string | null,
  sessionId: string | null,
  onSessionCreated: (session: ChatSession) => void,
) {
  const [messages, setMessages] = useState<Message[]>([]);
  const [busy, setBusy] = useState(false);
  /// Incremental assistant text while the model generates (null = idle).
  const [streamingText, setStreamingText] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pendingConsent, setPendingConsent] = useState<PendingConsent | null>(null);
  /// Tool calls from the most recent agent turn, keyed by the message id.
  const [toolEvents, setToolEvents] = useState<Record<string, ToolEvent[]>>({});

  useEffect(() => {
    setError(null);
    setPendingConsent(null);
    setToolEvents({});
    if (!sessionId) {
      setMessages([]);
      return;
    }
    void api.listChatMessages(sessionId).then(setMessages);
  }, [sessionId]);

  const ensureSession = useCallback(
    async (firstQuestion: string): Promise<string> => {
      if (sessionId) return sessionId;
      if (!workspaceId) throw new Error("no workspace selected");
      const session = await api.createChatSession(
        workspaceId,
        firstQuestion.slice(0, 60),
      );
      onSessionCreated(session);
      return session.id;
    },
    [sessionId, workspaceId, onSessionCreated],
  );

  const runAsk = useCallback(
    async (question: string, provider: ProviderKind, consentGranted: boolean) => {
      if (!workspaceId) return;
      setBusy(true);
      setError(null);
      setStreamingText("");
      try {
        const sid = await ensureSession(question);
        await api.ask(sid, workspaceId, question, provider, consentGranted, (token) =>
          setStreamingText((current) => (current ?? "") + token),
        );
        setMessages(await api.listChatMessages(sid));
      } catch (e) {
        if (isCommandError(e) && e.code === "consent_required") {
          // Backstop — the prepare step should normally catch this first.
          const preparation = await api.prepareAsk(workspaceId, question, provider, sessionId);
          setPendingConsent({ question, provider, preparation });
        } else {
          setError(isCommandError(e) ? e.message : String(e));
        }
      } finally {
        setBusy(false);
        setStreamingText(null);
      }
    },
    [workspaceId, sessionId, ensureSession],
  );

  const runAgent = useCallback(
    async (question: string, provider: ProviderKind, allowWrites: boolean) => {
      if (!workspaceId) return;
      setBusy(true);
      setError(null);
      setStreamingText("");
      try {
        const sid = await ensureSession(question);
        const outcome = await api.agentAsk(sid, workspaceId, question, provider, allowWrites);
        setMessages(await api.listChatMessages(sid));
        if (outcome.tool_events.length > 0) {
          setToolEvents((prev) => ({
            ...prev,
            [outcome.message.id]: outcome.tool_events,
          }));
        }
      } catch (e) {
        setError(isCommandError(e) ? e.message : String(e));
      } finally {
        setBusy(false);
        setStreamingText(null);
      }
    },
    [workspaceId, ensureSession],
  );

  /**
   * Entry point from the input box. In agent mode the model can call tools
   * (search, create issues, write wiki); `allowWrites` opts into external
   * actions for this message. In plain mode it's grounded Q&A with the
   * external-send consent gate.
   */
  const send = useCallback(
    async (
      question: string,
      provider: ProviderKind,
      agentMode: boolean,
      allowWrites: boolean,
    ) => {
      if (!workspaceId) return;
      setError(null);
      const trimmed = question.trim();
      if (!trimmed) return;
      if (agentMode) {
        await runAgent(trimmed, provider, allowWrites);
        return;
      }
      try {
        const preparation = await api.prepareAsk(workspaceId, trimmed, provider, sessionId);
        if (preparation.requires_consent) {
          setPendingConsent({ question: trimmed, provider, preparation });
          return;
        }
        await runAsk(trimmed, provider, false);
      } catch (e) {
        setError(isCommandError(e) ? e.message : String(e));
      }
    },
    [workspaceId, sessionId, runAsk, runAgent],
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
    messages,
    busy,
    streamingText,
    error,
    send,
    toolEvents,
    pendingConsent,
    approveConsent,
    declineConsent,
  };
}
