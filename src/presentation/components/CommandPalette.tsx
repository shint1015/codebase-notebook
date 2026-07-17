import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import type { ChatSearchHit, ChatSession, Workspace } from "../../domain/types";
import { api } from "../../infrastructure/api";

interface Props {
  workspace: Workspace | null;
  sessions: ChatSession[];
  onClose: () => void;
  onOpenSession: (sessionId: string) => void;
  onNewChat: () => void;
  onNewDocument: () => void;
  onOpenSettings: () => void;
  onIndex: () => void;
  onGoHome: () => void;
}

type Item =
  | { kind: "action"; id: string; label: string; run: () => void }
  | { kind: "chat"; id: string; label: string; run: () => void }
  | { kind: "message"; id: string; label: string; detail: string; run: () => void };

/**
 * Cmd/Ctrl+K palette: jump to a chat, run an action, or search across every
 * message in the workspace.
 */
export function CommandPalette({
  workspace,
  sessions,
  onClose,
  onOpenSession,
  onNewChat,
  onNewDocument,
  onOpenSettings,
  onIndex,
  onGoHome,
}: Props) {
  const { t } = useTranslation();
  const [query, setQuery] = useState("");
  const [messageHits, setMessageHits] = useState<ChatSearchHit[]>([]);
  const [active, setActive] = useState(0);

  // Search chat contents (debounced) once the query is meaningful.
  useEffect(() => {
    if (!workspace || query.trim().length < 2) {
      setMessageHits([]);
      return;
    }
    const handle = setTimeout(() => {
      api
        .searchChats(workspace.id, query.trim(), 20)
        .then(setMessageHits)
        .catch(() => setMessageHits([]));
    }, 180);
    return () => clearTimeout(handle);
  }, [workspace, query]);

  const items = useMemo<Item[]>(() => {
    const q = query.trim().toLowerCase();
    const allActions: Item[] = [
      { kind: "action", id: "new-chat", label: t("palette.newChat"), run: onNewChat },
      { kind: "action", id: "new-doc", label: t("palette.newDocument"), run: onNewDocument },
      { kind: "action", id: "index", label: t("palette.index"), run: onIndex },
      { kind: "action", id: "home", label: t("palette.home"), run: onGoHome },
      { kind: "action", id: "settings", label: t("palette.settings"), run: onOpenSettings },
    ];
    const actions = allActions.filter((a) => !q || a.label.toLowerCase().includes(q));

    const chats: Item[] = sessions
      .filter((s) => !q || s.title.toLowerCase().includes(q))
      .slice(0, 8)
      .map((s) => ({
        kind: "chat",
        id: `chat-${s.id}`,
        label: s.title,
        run: () => onOpenSession(s.id),
      }));

    const messages: Item[] = messageHits.map((h) => ({
      kind: "message",
      id: `msg-${h.message_id}`,
      label: h.session_title,
      detail: h.excerpt,
      run: () => onOpenSession(h.session_id),
    }));

    return [...actions, ...chats, ...messages];
  }, [
    query,
    sessions,
    messageHits,
    t,
    onNewChat,
    onNewDocument,
    onIndex,
    onGoHome,
    onOpenSettings,
    onOpenSession,
  ]);

  useEffect(() => setActive(0), [query, messageHits.length]);

  const choose = (item: Item) => {
    item.run();
    onClose();
  };

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="palette" onClick={(e) => e.stopPropagation()}>
        <input
          autoFocus
          className="palette-input"
          value={query}
          placeholder={t("palette.placeholder")}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "ArrowDown") {
              e.preventDefault();
              setActive((i) => Math.min(i + 1, items.length - 1));
            } else if (e.key === "ArrowUp") {
              e.preventDefault();
              setActive((i) => Math.max(i - 1, 0));
            } else if (e.key === "Enter" && items[active]) {
              e.preventDefault();
              choose(items[active]);
            } else if (e.key === "Escape") {
              onClose();
            }
          }}
        />
        <ul className="palette-list">
          {items.map((item, i) => (
            <li
              key={item.id}
              className={i === active ? "active" : ""}
              onMouseEnter={() => setActive(i)}
              onClick={() => choose(item)}
            >
              <span className="palette-kind">
                {item.kind === "action" ? "⚡" : item.kind === "chat" ? "💬" : "🔎"}
              </span>
              <span className="palette-label">{item.label}</span>
              {item.kind === "message" && (
                <span className="palette-detail">{item.detail}</span>
              )}
            </li>
          ))}
          {items.length === 0 && <li className="empty">{t("palette.noResults")}</li>}
        </ul>
        <div className="palette-hint">{t("palette.hint")}</div>
      </div>
    </div>
  );
}
